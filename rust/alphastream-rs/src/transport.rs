// Transport abstraction module

use thiserror::Error;
use bytes::Bytes;
use std::future::Future;
use std::pin::Pin;
use reqwest::{Client, header::RANGE};
use std::time::Duration;
use tokio::time::sleep;
use std::fs::File;
use memmap2::Mmap;
use tokio::fs::File as AsyncFile;
use tokio::io::{AsyncReadExt, AsyncSeekExt, BufReader as AsyncBufReader};
use tokio::fs;

#[derive(Error, Debug)]
pub enum TransportError {
    #[error("Not found")]
    NotFound,
    #[error("Timeout")]
    Timeout,
    #[error("Transport error: {0}")]
    Other(String),
}

pub trait Transport {
    type Reader: Send + Sync;
    /// Opens a reader for the given URI, returning a future that resolves to the reader or an error.
    ///
    /// The return type `Pin<Box<dyn Future<Output = Result<Self::Reader, TransportError>> + Send>>` is used because:
    /// - `Pin`: Required for heap-allocated futures that may contain self-references, ensuring memory safety.
    /// - `Box`: Allocates the future on the heap to avoid stack size issues with potentially large futures.
    /// - `dyn Future`: Enables dynamic dispatch, allowing different future implementations to be returned.
    /// - `+ Send`: Ensures the future can be sent across threads in async runtimes like tokio.
    /// - `Output`: Specifies the async result type as `Result<Self::Reader, TransportError>`.
    fn open(uri: &str) -> Pin<Box<dyn Future<Output = Result<Self::Reader, TransportError>> + Send + '_>>;
    fn len(reader: &Self::Reader) -> u64;
    fn read_range(reader: &Self::Reader, offset: u64, size: u32) -> Pin<Box<dyn Future<Output = Result<Bytes, TransportError>> + Send + 'static>>;
    // Add cancelation/backpressure hooks as needed
}

type TransportFuture = Pin<Box<dyn Future<Output = Result<Bytes, TransportError>> + Send + 'static>>;

pub struct MockReader {
    data: Bytes,
}

pub struct MockTransport;

impl Transport for MockTransport {
    type Reader = MockReader;

    fn open(_uri: &str) -> Pin<Box<dyn Future<Output = Result<Self::Reader, TransportError>> + Send>> {
        Box::pin(async move {
            Ok(MockReader {
                data: Bytes::from("mock data for testing"),
            })
        })
    }

    fn len(reader: &Self::Reader) -> u64 {
        reader.data.len() as u64
    }

    fn read_range(reader: &Self::Reader, offset: u64, size: u32) -> Pin<Box<dyn Future<Output = Result<Bytes, TransportError>> + Send>> {
        let data = reader.data.clone();
        Box::pin(async move {
            let start = offset as usize;
            let end = start + size as usize;
            if start > data.len() {
                return Err(TransportError::Other("Offset out of bounds".to_string()));
            }
            let end = end.min(data.len());
            Ok(data.slice(start..end))
        })
    }
}

// LocalTransport implementation using memory mapping for efficiency, with buffered fallback
pub enum LocalData {
    // Memory-mapped file for fast random access
    Mmap(Mmap),
    // Path for buffered reading when memory mapping fails
    BufferedPath(String),
}

pub struct LocalReader {
    // The data source, either memory mapped or buffered
    data: LocalData,
    // Cached file length
    len: u64,
}

pub struct LocalTransport;

impl Transport for LocalTransport {
    type Reader = LocalReader;

    // Opens a local file reader, preferring memory mapping for performance
    fn open(uri: &str) -> Pin<Box<dyn Future<Output = Result<Self::Reader, TransportError>> + Send>> {
        let uri = uri.to_string();
        Box::pin(async move {
            open_local(&uri)
        })
    }

    // Returns the cached file length
    fn len(reader: &Self::Reader) -> u64 {
        reader.len
    }

    // Reads a range of bytes, using direct slicing for mmap or async read for buffered
    fn read_range(reader: &Self::Reader, offset: u64, size: u32) -> Pin<Box<dyn Future<Output = Result<Bytes, TransportError>> + Send + 'static>> {
        let future: TransportFuture = match &reader.data {
            LocalData::Mmap(mmap) => {
                // For memory mapped files, slice directly from memory
                let start = offset as usize;
                let end = (offset + size as u64) as usize;
                let result = if start > mmap.len() {
                    Err(TransportError::Other("Offset out of bounds".to_string()))
                } else {
                    let end = end.min(mmap.len());
                    let data = Bytes::copy_from_slice(&mmap[start..end]);
                    Ok(data)
                };
                Box::pin(async { result }) as TransportFuture
            }
            LocalData::BufferedPath(path) => {
                // For buffered files, open new file, seek to offset and read asynchronously
                let path = path.clone();
                Box::pin(async move {
                    let file = AsyncFile::open(&path).await.map_err(|_| TransportError::NotFound)?;
                    let mut reader = AsyncBufReader::new(file);
                    reader.seek(std::io::SeekFrom::Start(offset)).await.map_err(|e| TransportError::Other(e.to_string()))?;
                    let mut buf = vec![0; size as usize];
                    let read = reader.read(&mut buf).await.map_err(|e| TransportError::Other(e.to_string()))?;
                    buf.truncate(read);
                    Ok(Bytes::from(buf))
                }) as TransportFuture
            }
        };
        future
    }
}

// InMemoryTransport implementation that loads the entire file into memory for fast access
pub struct InMemoryReader {
    // The entire file data loaded into memory as a Bytes slice
    data: Bytes,
}

pub struct InMemoryTransport;

impl Transport for InMemoryTransport {
    type Reader = InMemoryReader;

    // Opens by reading the entire file into memory
    fn open(uri: &str) -> Pin<Box<dyn Future<Output = Result<Self::Reader, TransportError>> + Send + '_>> {
        Box::pin(async move {
            // Read the entire file into bytes
            let data = fs::read(uri).await.map_err(|_| TransportError::NotFound)?;
            Ok(InMemoryReader {
                data: Bytes::from(data),
            })
        })
    }

    // Returns the length of the in-memory data
    fn len(reader: &Self::Reader) -> u64 {
        reader.data.len() as u64
    }

    // Reads a range by slicing the in-memory data
    fn read_range(reader: &Self::Reader, offset: u64, size: u32) -> Pin<Box<dyn Future<Output = Result<Bytes, TransportError>> + Send + 'static>> {
        let data = reader.data.clone();
        Box::pin(async move {
            let start = offset as usize;
            let end = start + size as usize;
            if start > data.len() {
                return Err(TransportError::Other("Offset out of bounds".to_string()));
            }
            let end = end.min(data.len());
            Ok(data.slice(start..end))
        }) as TransportFuture
    }
}

// Helper function to open local file, trying memory mapping first
fn open_local(uri: &str) -> Result<LocalReader, TransportError> {
    // Try to open the file synchronously for memory mapping
    let file = File::open(uri).map_err(|_| TransportError::NotFound)?;
    let metadata = file.metadata().map_err(|e| TransportError::Other(e.to_string()))?;
    let len = metadata.len();
    // Attempt memory mapping first
    match unsafe { Mmap::map(&file) } {
        Ok(mmap) => {
            Ok(LocalReader {
                data: LocalData::Mmap(mmap),
                len,
            })
        }
        Err(_) => {
            // Fallback to storing path for buffered reading
            Ok(LocalReader {
                data: LocalData::BufferedPath(uri.to_string()),
                len,
            })
        }
    }
}

// HttpTransport implementation using reqwest for HTTP-based transport with Range requests
pub struct HttpReader {
    // The URL of the resource to read from
    url: String,
    // HTTP client for making requests
    client: Client,
    // Cached content length to avoid repeated HEAD requests
    content_length: u64,
}

pub struct HttpTransport;

impl Transport for HttpTransport {
    type Reader = HttpReader;

    // Opens an HTTP reader by fetching the content length via a HEAD request
    fn open(uri: &str) -> Pin<Box<dyn Future<Output = Result<Self::Reader, TransportError>> + Send + '_>> {
        Box::pin(async move {
            let client = Client::new();
            // Perform a HEAD request to get the content length
            let response = client.head(uri).send().await.map_err(|e| TransportError::Other(e.to_string()))?;
            if !response.status().is_success() {
                return Err(TransportError::NotFound);
            }
            let content_length = response.content_length().unwrap_or(0);
            Ok(HttpReader {
                url: uri.to_string(),
                client,
                content_length,
            })
        })
    }

    // Returns the cached content length
    fn len(reader: &Self::Reader) -> u64 {
        reader.content_length
    }

    // Reads a range of bytes using HTTP Range requests with retries
    fn read_range(reader: &Self::Reader, offset: u64, size: u32) -> Pin<Box<dyn Future<Output = Result<Bytes, TransportError>> + Send>> {
        let url = reader.url.clone();
        let client = reader.client.clone();
        Box::pin(async move {
            let range_header = format!("bytes={}-{}", offset, offset + size as u64 - 1);
            let mut attempts = 0;
            const MAX_RETRIES: u32 = 3;
            loop {
                attempts += 1;
                let response = client.get(&url)
                    .header(RANGE, &range_header)
                    .send()
                    .await;
                match response {
                    Ok(resp) if resp.status().is_success() => {
                        let bytes = resp.bytes().await.map_err(|e| TransportError::Other(e.to_string()))?;
                        return Ok(bytes);
                    }
                    Ok(resp) if resp.status() == reqwest::StatusCode::RANGE_NOT_SATISFIABLE => {
                        return Err(TransportError::Other("Range not satisfiable".to_string()));
                    }
                    _ if attempts < MAX_RETRIES => {
                        // Simple exponential backoff: wait 2^attempts seconds
                        sleep(Duration::from_secs(1 << attempts)).await;
                        continue;
                    }
                    _ => return Err(TransportError::Other("Failed to read range after retries".to_string())),
                }
            }
        })
    }
}