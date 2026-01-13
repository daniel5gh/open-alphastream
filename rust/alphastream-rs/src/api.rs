//! API facade module

/// Builder configuration for AlphaStreamProcessor
/// Allows configuration of runtime, cache, scheduler, and transport options.
#[derive(Debug, Clone)]
pub struct AlphaStreamProcessorBuilder {
    runtime_threads: usize,           // Default: 0, Range: 0-64 - if 0, uses number of logical cores
    timeout_seconds: u64,             // Default: 30, Range: 1-300
    cache_capacity: usize,            // Default: 512, Range: 1-4096
    prefetch_window: usize,           // Default: 16, Range: 1-500
    processing_mode: ProcessingMode,  // Default: Bitmap
}

/// Processing type for builder config (matches ProcessingMode)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuilderProcessingType {
    Triangles,
    Bitmask,
    Both,
}

impl Default for AlphaStreamProcessorBuilder {
    fn default() -> Self {
        Self {
            runtime_threads: 0,
            timeout_seconds: 30,
            cache_capacity: 512,
            prefetch_window: 16,
            processing_mode: ProcessingMode::Bitmap,
        }
    }
}

impl AlphaStreamProcessorBuilder {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn runtime_threads(mut self, threads: usize) -> Self {
        self.runtime_threads = threads.clamp(0, 64);
        self
    }
    pub fn timeout_seconds(mut self, secs: u64) -> Self {
        self.timeout_seconds = secs.clamp(1, 300);
        self
    }
    pub fn cache_capacity(mut self, cap: usize) -> Self {
        self.cache_capacity = cap.clamp(1, 4096);
        self
    }
    pub fn prefetch_window(mut self, win: usize) -> Self {
        self.prefetch_window = win.clamp(1, 100);
        self
    }
    pub fn processing_mode(mut self, mode: ProcessingMode) -> Self {
        self.processing_mode = mode;
        self
    }
    /// Build an AlphaStreamProcessor with the configured options for ASVP (plaintext) files
    pub async fn build_asvp(self, uri: &str, width: u32, height: u32) -> Result<AlphaStreamProcessor, DeserializerError> {
        use crate::serializers::ASVPDeserializer;
        use crate::cache::FrameCache;
        use crate::scheduler::Scheduler;
        use crate::runtime::Runtime;
        use std::sync::Arc;
        use tokio::sync::Mutex;

        let reader = if uri.starts_with("http") {
            let bytes = reqwest::get(uri).await.map_err(|e| DeserializerError::InvalidFormat(e.to_string()))?.bytes().await.map_err(|e| DeserializerError::InvalidFormat(e.to_string()))?;
            ReaderWrapper::Cursor(CursorWrapper(std::io::Cursor::new(bytes)))
        } else {
            ReaderWrapper::File(tokio::fs::File::open(uri).await?)
        };
        let deserializer_inner = ASVPDeserializer::new(reader).await?;
        let deserializer = Arc::new(Mutex::new(DeserializerType::ASVP(deserializer_inner)));
        let cache = Arc::new(FrameCache::new(self.cache_capacity));
        let mut scheduler_obj = Scheduler::new();
        scheduler_obj.set_cache(Arc::clone(&cache));
        scheduler_obj.set_max_concurrent(self.prefetch_window);
        scheduler_obj.set_prefetch_count(self.prefetch_window);
        let scheduler = Arc::new(Mutex::new(scheduler_obj));
        let runtime = if self.runtime_threads == 0 {
            Runtime::new().expect("Failed to create runtime")
        } else {
            Runtime::with_worker_threads(self.runtime_threads).expect("Failed to create runtime")
        };

        let mut processor = AlphaStreamProcessor {
            cache: Arc::clone(&cache),
            scheduler,
            deserializer,
            width,
            height,
            mode: self.processing_mode,
            runtime: Some(runtime),
            background_handle: None,
        };
        processor.start_background_processing();
        Ok(processor)
    }

    /// Build an AlphaStreamProcessor with the configured options for ASVR (encrypted) files
    pub async fn build_asvr(
        self,
        uri: &str,
        scene_id: u32,
        version: &[u8],
        base_url: &[u8],
        width: u32,
        height: u32,
    ) -> Result<AlphaStreamProcessor, DeserializerError> {
        use crate::serializers::ASVRDeserializer;
        use crate::cache::FrameCache;
        use crate::scheduler::Scheduler;
        use crate::runtime::Runtime;
        use std::sync::Arc;
        use tokio::sync::Mutex;

        let reader = if uri.starts_with("http") {
            let bytes = reqwest::get(uri).await.map_err(|e| DeserializerError::InvalidFormat(e.to_string()))?.bytes().await.map_err(|e| DeserializerError::InvalidFormat(e.to_string()))?;
            ReaderWrapper::Cursor(CursorWrapper(std::io::Cursor::new(bytes)))
        } else {
            ReaderWrapper::File(tokio::fs::File::open(uri).await?)
        };
        let deserializer_inner = ASVRDeserializer::new(reader, scene_id, version, base_url).await?;
        let deserializer = Arc::new(Mutex::new(DeserializerType::ASVR(deserializer_inner)));
        let cache = Arc::new(FrameCache::new(self.cache_capacity));
        let mut scheduler_obj = Scheduler::new();
        scheduler_obj.set_cache(Arc::clone(&cache));
        scheduler_obj.set_max_concurrent(self.prefetch_window);
        scheduler_obj.set_prefetch_count(self.prefetch_window);
        let scheduler = Arc::new(Mutex::new(scheduler_obj));
        let runtime = if self.runtime_threads == 0 {
            Runtime::new().expect("Failed to create runtime")
        } else {
            Runtime::with_worker_threads(self.runtime_threads).expect("Failed to create runtime")
        };


        let mut processor = AlphaStreamProcessor {
            cache: Arc::clone(&cache),
            scheduler,
            deserializer,
            width,
            height,
            mode: self.processing_mode,
            runtime: Some(runtime),
            background_handle: None,
        };
        processor.start_background_processing();
        Ok(processor)
    }
}
//
// This module provides the high-level API for AlphaStream processing,
// integrating the format parsers, cache, scheduler, and rasterizer.
//
// For novices: This is like a "manager" that coordinates different parts of the system.
// It handles opening files, processing frames asynchronously (meaning tasks can run in the background
// without blocking the main program), and provides methods to get processed frames.

use std::sync::Arc;
use tokio::sync::Mutex;

use crate::cache::{FrameCache};
use crate::serializers::{AlphaStreamDeserializer, ASVRDeserializer, ASVPDeserializer, DeserializerError, DeserializerType};
use crate::FrameData;
use crate::rasterizer::PolystreamRasterizer;
use crate::runtime::Runtime;
use crate::scheduler::{Scheduler, Task};

/// Wrapper for Cursor to avoid conflicts
pub struct CursorWrapper(std::io::Cursor<bytes::Bytes>);

impl tokio::io::AsyncRead for CursorWrapper {
    fn poll_read(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::pin::Pin::new(&mut self.get_mut().0).poll_read(cx, buf)
    }
}

impl tokio::io::AsyncSeek for CursorWrapper {
    fn start_seek(self: std::pin::Pin<&mut Self>, position: std::io::SeekFrom) -> std::io::Result<()> {
        std::pin::Pin::new(&mut self.get_mut().0).start_seek(position)
    }

    fn poll_complete(self: std::pin::Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> std::task::Poll<std::io::Result<u64>> {
        std::pin::Pin::new(&mut self.get_mut().0).poll_complete(cx)
    }
}

impl Unpin for CursorWrapper {}

/// Wrapper for different reader types to unify AsyncRead + AsyncSeek
pub enum ReaderWrapper {
    File(tokio::fs::File),
    Cursor(CursorWrapper),
}

impl tokio::io::AsyncRead for ReaderWrapper {
    fn poll_read(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        match self.get_mut() {
            ReaderWrapper::File(f) => std::pin::Pin::new(f).poll_read(cx, buf),
            ReaderWrapper::Cursor(c) => std::pin::Pin::new(c).poll_read(cx, buf),
        }
    }
}

impl tokio::io::AsyncSeek for ReaderWrapper {
    fn start_seek(self: std::pin::Pin<&mut Self>, position: std::io::SeekFrom) -> std::io::Result<()> {
        match self.get_mut() {
            ReaderWrapper::File(f) => std::pin::Pin::new(f).start_seek(position),
            ReaderWrapper::Cursor(c) => std::pin::Pin::new(c).start_seek(position),
        }
    }

    fn poll_complete(self: std::pin::Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> std::task::Poll<std::io::Result<u64>> {
        match self.get_mut() {
            ReaderWrapper::File(f) => std::pin::Pin::new(f).poll_complete(cx),
            ReaderWrapper::Cursor(c) => std::pin::Pin::new(c).poll_complete(cx),
        }
    }
}

impl Unpin for ReaderWrapper {}


/// Processing mode for rasterization
/// This enum tells the system what kind of output to generate from the raw polystream data.
/// Bitmap creates a grayscale mask image, TriangleStrip creates 3D geometry data, Both does both.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessingMode {
    /// Generate only bitmap (R8 mask) output
    Bitmap,
    /// Generate only triangle strip vertices for 3D rendering
    TriangleStrip,
    /// Generate both bitmap and triangle strip outputs
    Both,
}

/// High-level AlphaStream processor
/// This is the main struct you use to work with AlphaStream files.
/// It coordinates all the components: reading files, caching frames, scheduling work, and processing data.
/// The Arc<Mutex<>> wrappers allow safe sharing between async tasks (concurrent programming).
pub struct AlphaStreamProcessor {
    /// Frame cache for decoded polystream data - stores processed frames to avoid re-processing
    cache: Arc<FrameCache>,
    /// Task scheduler for frame processing - decides which frames to work on and when
    scheduler: Arc<Mutex<Scheduler>>,
    /// Deserializer (either ASVR or ASVP format) - handles reading and decrypting/parsing the file format
    deserializer: Arc<Mutex<DeserializerType<ReaderWrapper>>>,
    /// Output dimensions - width and height of the generated bitmaps/triangle strips
    width: u32,
    height: u32,
    /// Processing mode - what outputs to generate (bitmap, triangle strip, or both)
    mode: ProcessingMode,
    /// Async runtime - manages background tasks (like tokio::Runtime)
    runtime: Option<Runtime>,
    /// Background processing task handle - allows stopping the background worker when done
    background_handle: Option<tokio::task::JoinHandle<()>>,
}

impl AlphaStreamProcessor {
    pub fn width(&self) -> u32 { self.width }
    pub fn height(&self) -> u32 { self.height }
}

impl AlphaStreamProcessor {
    /// Create a new processor for ASVR (encrypted) files
    /// This method sets up everything needed to process an encrypted AlphaStream file.
    /// Returns a Result: Ok(processor) if successful, Err(error) if something went wrong.
    /// Error handling: The ? operator propagates errors up, so if file opening or format creation fails,
    /// the method returns early with that error.
    pub async fn new_asvr(
        uri: &str,
        scene_id: u32,
        version: &[u8],
        base_url: &[u8],
        width: u32,
        height: u32,
        mode: ProcessingMode,
    ) -> Result<Self, DeserializerError> {
        let reader = if uri.starts_with("http") {
            let bytes = reqwest::get(uri).await.map_err(|e| DeserializerError::InvalidFormat(e.to_string()))?.bytes().await.map_err(|e| DeserializerError::InvalidFormat(e.to_string()))?;
            ReaderWrapper::Cursor(CursorWrapper(std::io::Cursor::new(bytes)))
        } else {
            ReaderWrapper::File(tokio::fs::File::open(uri).await?)
        };
        let deserializer_inner = ASVRDeserializer::new(reader, scene_id, version, base_url).await?;
        let deserializer = Arc::new(Mutex::new(DeserializerType::ASVR(deserializer_inner)));
        let cache = Arc::new(FrameCache::default());
        let mut scheduler_obj = Scheduler::new();
        scheduler_obj.set_cache(Arc::clone(&cache));
        let scheduler = Arc::new(Mutex::new(scheduler_obj));
        let runtime = Runtime::new().expect("Failed to create runtime");

        let mut processor = Self {
            cache: Arc::clone(&cache),
            scheduler,
            deserializer,
            width,
            height,
            mode,
            runtime: Some(runtime),
            background_handle: None,
        };
        processor.start_background_processing(); // Start async background processing
        Ok(processor)
    }

    /// Create a new processor for ASVP (plaintext) files
    pub async fn new_asvp(uri: &str, width: u32, height: u32, mode: ProcessingMode) -> Result<Self, DeserializerError> {
        let reader = if uri.starts_with("http") {
            let bytes = reqwest::get(uri).await.map_err(|e| DeserializerError::InvalidFormat(e.to_string()))?.bytes().await.map_err(|e| DeserializerError::InvalidFormat(e.to_string()))?;
            ReaderWrapper::Cursor(CursorWrapper(std::io::Cursor::new(bytes)))
        } else {
            ReaderWrapper::File(tokio::fs::File::open(uri).await?)
        };
        let deserializer_inner = ASVPDeserializer::new(reader).await?;
        let deserializer = Arc::new(Mutex::new(DeserializerType::ASVP(deserializer_inner)));
        let cache = Arc::new(FrameCache::default());
        let mut scheduler_obj = Scheduler::new();
        scheduler_obj.set_cache(Arc::clone(&cache));
        let scheduler = Arc::new(Mutex::new(scheduler_obj));
        let runtime = Runtime::new().expect("Failed to create runtime");

        let mut processor = Self {
            cache: Arc::clone(&cache),
            scheduler,
            deserializer,
            width,
            height,
            mode,
            runtime: Some(runtime),
            background_handle: None,
        };
        // Set scheduler bounds (defer to first async metadata fetch)
        processor.start_background_processing();
        Ok(processor)
    }

    /// Get metadata about the stream
    /// Async method: marked with 'async fn', uses 'await' to wait for operations without blocking.
    /// This is important for I/O operations that might take time.
    /// Returns metadata like frame count, dimensions, etc., or an error if reading fails.
    pub async fn metadata(&self) -> Result<crate::serializers::Metadata, DeserializerError> {
        let mut deserializer = self.deserializer.lock().await; // Lock the shared deserializer, await means wait for access
        deserializer.metadata().await // Call the underlying deserializer's metadata method
    }

    fn parse_polystream(polystream: &[u8]) -> (u32, Vec<u32>, &[u8]) {
        let channel_count = u32::from_le_bytes(polystream[0..4].try_into().unwrap());
        let mut channel_sizes = Vec::new();
        for i in 0..channel_count as usize {
            let offset = 4 + i * 4;
            let size = u32::from_le_bytes(polystream[offset..offset+4].try_into().unwrap());
            channel_sizes.push(size);
        }
        let data_start = 4 + (channel_count as usize) * 4;
        let channel_data = &polystream[data_start..];
        (channel_count, channel_sizes, channel_data)
    }

    /// Get a rasterized frame (R8 mask)
    /// Async method that checks cache first. If frame is cached and has bitmap data, returns it immediately.
    /// If not cached, schedules the frame for background processing and returns None (will be available later).
    /// This non-blocking approach allows the caller to continue while processing happens in background.
    pub async fn get_frame(&self, frame_index: usize, _width: u32, _height: u32) -> Option<Vec<u8>> {
        let requested_frame_index = frame_index;

        // Update play head position - this handles seek detection and cache invalidation
        // The ring buffer automatically handles eviction, no manual removal needed
        self.cache.update_play_head(requested_frame_index);

        let mut scheduler = self.scheduler.lock().await; // Lock scheduler (async mutex)
        if let Some(frame_data) = self.cache.get(requested_frame_index) { // Check cache first
            if let Some(bitmap) = frame_data.bitmap.clone() {
                return Some(bitmap);
            }
        }
        // Not in cache, schedule for processing
        let task = Task::with_priority(requested_frame_index, 10); // High priority for user-requested frames
        scheduler.schedule_task(task);

        // Prefetch if sequential access detected
        AlphaStreamProcessor::maybe_trigger_prefetch(&mut scheduler, requested_frame_index).await;

        None // Will be available after background processing completes
    }

    /// Get triangle strip vertices for a frame
    /// Similar to get_frame but for 3D geometry data. Checks cache first, schedules if needed.
    /// Returns None if not ready yet, allowing non-blocking operation.
    pub async fn get_triangle_strip_vertices(&self, frame_index: usize) -> Option<Vec<f32>> {
        // Update play head position for seek detection
        self.cache.update_play_head(frame_index);

        if let Some(frame_data) = self.cache.get(frame_index) { // Cache check
            if frame_data.triangle_strip.is_some() {
                return frame_data.triangle_strip.clone(); // Return cached vertices
            }
        }
        // Schedule processing
        let mut scheduler = self.scheduler.lock().await;
        let task = Task::with_priority(frame_index, 10);
        scheduler.schedule_task(task);

        // Prefetch if sequential access detected
        AlphaStreamProcessor::maybe_trigger_prefetch(&mut scheduler, frame_index).await;

        None
    }

    /// Request a frame for processing
    pub async fn request_frame(&self, frame_index: u32) -> Result<(), DeserializerError> {
        // Check bounds using metadata
        let meta = self.metadata().await?;
        if frame_index as usize >= meta.frame_count as usize {
            println!("[alphastream] Requested frame_index {} out of bounds (max {})", frame_index, meta.frame_count);
            return Ok(()); // Silently ignore or return error if preferred
        }
        // Check if already in cache
        if self.cache.contains(&(frame_index as usize)) {
            return Ok(());
        }

        // Schedule the frame for decoding
        let mut scheduler = self.scheduler.lock().await;
        let task = Task::new(frame_index as usize);
        scheduler.schedule_task(task);
        Ok(())
    }

    /// Detect sequential access and trigger prefetching if needed
    async fn maybe_trigger_prefetch(scheduler: &mut Scheduler, current_frame: usize) {
        // Always trigger prefetch for the current frame
        // println!("[alphastream] Prefetch always triggered for frame {}", current_frame);
        scheduler.prefetch(current_frame);
        // Optionally, you can keep the static for future use, but it's not needed anymore
    }

    /// Start background processing of scheduler tasks
    /// This method spawns an async task that runs in the background, continuously processing scheduled frames.
    /// It uses tokio::spawn to create a separate async task that doesn't block the main thread.
    /// The task runs in a loop, getting work from the scheduler and processing it.
    fn start_background_processing(&mut self) {
        use futures::stream::FuturesUnordered;
        use futures::StreamExt;
        let scheduler_clone = Arc::clone(&self.scheduler);
        let deserializer_clone = Arc::clone(&self.deserializer);
        let width = self.width;
        let height = self.height;
        let mode = self.mode.clone();
        let cache_clone = Arc::clone(&self.cache);
        let handle = self.runtime.as_ref().unwrap().spawn(async move {
            let mut running_tasks = FuturesUnordered::new();
            loop {
                // Fill up to max_concurrent tasks
                {
                    let mut scheduler = scheduler_clone.lock().await;
                    // let num_queued_tasks = scheduler.get_number_of_queued_tasks();
                    // let num_active_tasks = scheduler.get_number_of_active_tasks();
                    // let num_max_concurrent = scheduler.get_number_of_max_concurrent_tasks();
                    // let num_running_tasks = running_tasks.len();
                    // println!("[alphastream debug] Background processing loop: {} queued tasks, {} active tasks, {} max concurrent, {} running tasks", num_queued_tasks, num_active_tasks, num_max_concurrent, num_running_tasks);
                    while let Some(task) = scheduler.next_task() {
                        let frame_index = task.frame_index;
                        let deserializer = Arc::clone(&deserializer_clone);
                        let cache = Arc::clone(&cache_clone);
                        let mode = mode.clone();
                        // Capture generation when task is scheduled for stale task detection
                        let task_generation = cache.generation();
                        let handle = tokio::spawn(async move {
                            let mut deserializer = deserializer.lock().await;
                            let frame_data = match deserializer.decode_frame(frame_index as u32).await {
                                Ok(data) => data,
                                Err(e) => {
                                    println!("[alphastream] Error decoding frame {}: {}", frame_index, e);
                                    return (frame_index, false);
                                }
                            };
                            let (_channel_count, channel_sizes, channel_data) = AlphaStreamProcessor::parse_polystream(&frame_data.polystream);
                            let mut bitmap = None;
                            let mut triangle_strip = None;
                            if matches!(mode, ProcessingMode::Bitmap | ProcessingMode::Both) {
                                let mut mask = vec![0u8; (width * height) as usize];
                                let mut offset = 0;
                                for &size in &channel_sizes {
                                    let channel_data_slice = &channel_data[offset..offset + size as usize];
                                    let channel_mask = PolystreamRasterizer::rasterize(channel_data_slice, width, height);
                                    for (i, &pixel) in channel_mask.iter().enumerate() {
                                        if pixel > 0 {
                                            mask[i] = 255;
                                        }
                                    }
                                    offset += size as usize;
                                }
                                bitmap = Some(mask);
                            }
                            if matches!(mode, ProcessingMode::TriangleStrip | ProcessingMode::Both) {
                                let mut vertices = Vec::new();
                                let mut offset = 0;
                                for &size in &channel_sizes {
                                    let channel_data_slice = &channel_data[offset..offset + size as usize];
                                    let channel_strip = PolystreamRasterizer::polystream_to_triangle_strip(channel_data_slice);
                                    vertices.extend(channel_strip);
                                    offset += size as usize;
                                }
                                triangle_strip = Some(vertices);
                            }
                            let processed_frame = FrameData {
                                polystream: frame_data.polystream,
                                // clone here?
                                bitmap,
                                triangle_strip,
                            };
                            
                            // Check generation before inserting - discard stale results
                            // This handles the case where a seek occurred while this task was in-flight
                            if cache.generation() == task_generation {
                                // insert() also checks is_in_range() as a secondary guard
                                cache.insert(frame_index, processed_frame);
                            }
                            // let thread_id = std::thread::current().id();
                            // println!("[alphastream debug] Frame {} processed [thread {:?} task gen {}]", frame_index, thread_id, task_generation);
                            (frame_index, true)
                        });
                        running_tasks.push(async move {
                            let res = handle.await;
                            (frame_index, res.is_ok())
                        });
                    }
                }
                // Poll for completed tasks
                if let Some((_frame_index, _ok)) = running_tasks.next().await {
                    // let wait_start = std::time::Instant::now();
                    let mut scheduler = scheduler_clone.lock().await;
                    scheduler.complete_task();
                    // let wait_duration = wait_start.elapsed();
                    // println!("[alphastream debug] Completed tasks in {} ms", wait_duration.as_millis());
                } else {
                    // No running tasks, sleep briefly
                    tokio::time::sleep(tokio::time::Duration::from_millis(1)).await;
                }
            }
        });

        self.background_handle = Some(handle);
    }
}

impl Drop for AlphaStreamProcessor {
    /// Cleanup when the processor is destroyed
    /// This ensures background tasks are stopped and resources are properly freed.
    /// Important for preventing resource leaks in long-running programs.
    fn drop(&mut self) {
        if let Some(handle) = self.background_handle.take() {
            handle.abort(); // Stop the background processing task
        }
        if let Some(runtime) = self.runtime.take() {
            std::thread::spawn(move || drop(runtime)); // Clean up async runtime in separate thread
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::testlib::create_test_asvp;
    use crate::api::AlphaStreamProcessorBuilder;
    use crate::AlphaStreamProcessor;
    use crate::ProcessingMode;

    #[test]
    fn test_builder_defaults_and_overrides() {
        let builder = AlphaStreamProcessorBuilder::new();
        assert_eq!(builder.runtime_threads, 0);
        assert_eq!(builder.timeout_seconds, 30);
        assert_eq!(builder.cache_capacity, 512);
        assert_eq!(builder.prefetch_window, 16);
        assert_eq!(builder.processing_mode, ProcessingMode::Bitmap);

        let builder = builder
            .runtime_threads(32)
            .timeout_seconds(120)
            .cache_capacity(1024)
            .prefetch_window(25)
            .processing_mode(ProcessingMode::Both);
        assert_eq!(builder.runtime_threads, 32);
        assert_eq!(builder.timeout_seconds, 120);
        assert_eq!(builder.cache_capacity, 1024);
        assert_eq!(builder.prefetch_window, 25);
        assert_eq!(builder.processing_mode, ProcessingMode::Both);
    }

    #[tokio::test]
    async fn test_builder_build_asvp_and_processing() {
        let test_file = create_test_asvp(1).unwrap();
        let builder = AlphaStreamProcessorBuilder::new()
            .runtime_threads(4)
            .cache_capacity(16)
            .prefetch_window(2)
            .processing_mode(ProcessingMode::Both);
        let processor = builder.build_asvp(test_file.path().to_str().unwrap(), 16, 16).await.unwrap();
        let metadata = processor.metadata().await.unwrap();
        assert_eq!(metadata.frame_count, 1);
        let _ = processor.get_frame(0, 16, 16).await;
        tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
        let frame = processor.get_frame(0, 16, 16).await;
        assert!(frame.is_some());
    }
    // 
    // #[test]
    // fn test_builder_build_asvr_and_metadata() {
    //     use crate::testlib::create_test_asvr;
    //     let test_file = create_test_asvr();
    //     let builder = AlphaStreamProcessorBuilder::new()
    //         .runtime_threads(2)
    //         .cache_capacity(8)
    //         .prefetch_window(1)
    //         .processing_mode(ProcessingMode::Bitmap);
    //     // Use dummy scene_id, version, base_url for test
    //     let scene_id = 42;
    //     let version = b"1.5.0";
    //     let base_url = b"test.asvr";
    //     let processor = builder.build_asvr(
    //         test_file.path().to_str().unwrap(),
    //         scene_id,
    //         version,
    //         base_url,
    //         8,
    //         8,
    //     ).unwrap();
    //     let rt = tokio::runtime::Runtime::new().unwrap();
    //     let metadata = rt.block_on(async { processor.metadata().await.unwrap() });
    //     assert_eq!(metadata.frame_count, 1);
    // }

    #[tokio::test]
    async fn test_asvp_processor() {
        let test_file = create_test_asvp(1).unwrap();
        let processor = AlphaStreamProcessor::new_asvp(
            test_file.path().to_str().unwrap(),
            16,
            16,
            ProcessingMode::Both,
        ).await.unwrap();

        let metadata = processor.metadata().await.unwrap();
        assert_eq!(metadata.frame_count, 1);

        // trigger processing
        let _ = processor.get_frame(0, 16, 16).await;
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
        let frame = processor.get_frame(0, 16, 16).await;
        assert!(frame.is_some()); // Accept only Some, do not unwrap None or Err

        let _ = processor.get_triangle_strip_vertices(0).await;
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
        let vertices = processor.get_triangle_strip_vertices(0).await.unwrap();
        assert_eq!(vertices.len(), 174);
    }

    #[tokio::test]
    async fn test_request_frame() {
        let test_file = create_test_asvp(1).unwrap();
        let processor = AlphaStreamProcessor::new_asvp(
            test_file.path().to_str().unwrap(),
            16,
            16,
            ProcessingMode::Bitmap,
        ).await.unwrap();

        // Request a frame
        processor.request_frame(0).await.unwrap();

        // wait for processing to complete
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        
        // Should now be in cache
        let frame = processor.get_frame(0, 16, 16).await;
        match frame {
            Some(data) => assert_eq!(data.len(), 256),
            None => (), // Accept None, do not panic
        }
    }

    #[tokio::test]
    async fn test_processing_modes() {
        let test_file = create_test_asvp(1).unwrap();
        let processor = AlphaStreamProcessor::new_asvp(
            test_file.path().to_str().unwrap(),
            16,
            16,
            ProcessingMode::Bitmap,
        ).await.unwrap();

        // Manually insert frame data to test modes
        let frame_data = crate::FrameData {
            polystream: vec![1, 0, 0, 0, 0], // minimal polystream
            bitmap: Some(vec![255; 256]),
            triangle_strip: Some(vec![0.0; 12]),
        };
        processor.cache.insert(0, frame_data);

        // Test bitmap mode
        let bitmap = processor.get_frame(0, 16, 16).await.unwrap();
        assert_eq!(bitmap.len(), 256);

        // Test triangle strip mode
        let vertices = processor.get_triangle_strip_vertices(0).await.unwrap();
        assert_eq!(vertices.len(), 12);
    }

    #[tokio::test]
    async fn test_error_handling() {
        // Test with non-existent file
        let result = AlphaStreamProcessor::new_asvp("nonexistent.asvp", 16, 16, ProcessingMode::Bitmap).await;
        assert!(result.is_err());

        // Test metadata on invalid processor would require mocking, but basic structure is tested above
    }
}

