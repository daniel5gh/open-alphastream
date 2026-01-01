// Transport abstraction module

use thiserror::Error;
use bytes::Bytes;
use std::future::Future;
use std::pin::Pin;

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
    fn open(uri: &str) -> Pin<Box<dyn Future<Output = Result<Self::Reader, TransportError>> + Send>>;
    fn len(reader: &Self::Reader) -> u64;
    fn read_range(reader: &Self::Reader, offset: u64, size: u32) -> Pin<Box<dyn Future<Output = Result<Bytes, TransportError>> + Send>>;
    // Add cancelation/backpressure hooks as needed
}

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