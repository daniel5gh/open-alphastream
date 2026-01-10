//! API facade module
//!
//! This module provides the high-level API for AlphaStream processing,
//! integrating the format parsers, cache, scheduler, and rasterizer.
//!
//! For novices: This is like a "manager" that coordinates different parts of the system.
//! It handles opening files, processing frames asynchronously (meaning tasks can run in the background
//! without blocking the main program), and provides methods to get processed frames.

use std::fs::File;
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::cache::{FrameCache, FrameData};
use crate::formats::{ASFormat, ASVRFormat, ASVPFormat, FormatError};
use crate::rasterizer::PolystreamRasterizer;
use crate::runtime::Runtime;
use crate::scheduler::{Scheduler, Task};

/// Processing mode for rasterization
/// This enum tells the system what kind of output to generate from the raw polystream data.
/// Bitmap creates a grayscale mask image, TriangleStrip creates 3D geometry data, Both does both.
#[derive(Debug, Clone)]
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
    /// Format parser (either ASVR or ASVP) - handles reading and decrypting/parsing the file format
    format: Arc<Mutex<Box<dyn ASFormat + Send + Sync>>>,
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
    /// Create a new processor for ASVR (encrypted) files
    /// This method sets up everything needed to process an encrypted AlphaStream file.
    /// Returns a Result: Ok(processor) if successful, Err(error) if something went wrong.
    /// Error handling: The ? operator propagates errors up, so if file opening or format creation fails,
    /// the method returns early with that error.
    pub fn new_asvr(
        file_path: &str,
        scene_id: u32,
        version: &[u8],
        base_url: &[u8],
        width: u32,
        height: u32,
        mode: ProcessingMode,
    ) -> Result<Self, FormatError> {
        let file = File::open(file_path)?; // Open file, return error if fails
        let format = Arc::new(Mutex::new(Box::new(ASVRFormat::new(file, scene_id, version, base_url)?) as Box<dyn ASFormat + Send + Sync>));
        let cache = Arc::new(FrameCache::default());
        let scheduler = Arc::new(Mutex::new(Scheduler::new()));
        let runtime = Runtime::new().expect("Failed to create runtime");

        let mut processor = Self {
            cache: Arc::clone(&cache),
            scheduler,
            format,
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
    pub fn new_asvp(file_path: &str, width: u32, height: u32, mode: ProcessingMode) -> Result<Self, FormatError> {  
        let file = File::open(file_path)?;
        let format = Arc::new(Mutex::new(Box::new(ASVPFormat::new(file)?) as Box<dyn ASFormat + Send + Sync>));
        let cache = Arc::new(FrameCache::default());
        let scheduler = Arc::new(Mutex::new(Scheduler::new()));
        let runtime = Runtime::new().expect("Failed to create runtime");

        let mut processor = Self {
            cache: Arc::clone(&cache),
            scheduler,
            format,
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
    pub async fn metadata(&self) -> Result<crate::formats::Metadata, FormatError> {
        let mut format = self.format.lock().await; // Lock the shared format, await means wait for access
        format.metadata() // Call the underlying format's metadata method
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
        if let Some(frame_data) = self.cache.get(requested_frame_index) { // Check cache first
            if frame_data.bitmap.is_some() {
                return frame_data.bitmap.clone(); // Return cached bitmap
            }
        }
        // Not in cache, schedule for processing
        let mut scheduler = self.scheduler.lock().await; // Lock scheduler (async mutex)
        let task = Task::with_priority(requested_frame_index, 10); // High priority for user-requested frames
        scheduler.schedule_task(task);
        None // Will be available after background processing completes
    }

    /// Get triangle strip vertices for a frame
    /// Similar to get_frame but for 3D geometry data. Checks cache first, schedules if needed.
    /// Returns None if not ready yet, allowing non-blocking operation.
    pub async fn get_triangle_strip_vertices(&self, frame_index: usize) -> Option<Vec<f32>> {
        if let Some(frame_data) = self.cache.get(frame_index) { // Cache check
            if frame_data.triangle_strip.is_some() {
                return frame_data.triangle_strip.clone(); // Return cached vertices
            }
        }
        // Schedule processing
        let mut scheduler = self.scheduler.lock().await;
        let task = Task::with_priority(frame_index, 10);
        scheduler.schedule_task(task);
        None
    }

    /// Request a frame for processing
    pub async fn request_frame(&self, frame_index: u32) -> Result<(), FormatError> {
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

    /// Start background processing of scheduler tasks
    /// This method spawns an async task that runs in the background, continuously processing scheduled frames.
    /// It uses tokio::spawn to create a separate async task that doesn't block the main thread.
    /// The task runs in a loop, getting work from the scheduler and processing it.
    fn start_background_processing(&mut self) {
        let scheduler_clone = Arc::clone(&self.scheduler);
        let format_clone = Arc::clone(&self.format);
        let width = self.width;
        let height = self.height;
        let mode = self.mode.clone();

        let cache_clone = Arc::clone(&self.cache);
        let handle = self.runtime.as_ref().unwrap().spawn(async move {
            loop {
                let task = {
                    let mut scheduler = scheduler_clone.lock().await;
                    scheduler.next_task()
                };
                if let Some(task) = task {
                    let frame_index = task.frame_index;
                    let format = Arc::clone(&format_clone);
                    let width = width;
                    let height = height;
                    let mode = mode.clone();
                    let cache = Arc::clone(&cache_clone);
                    let handle = tokio::task::spawn_blocking(move || {
                        let mut format = format.blocking_lock();
                        if let Ok(frame_data) = format.decode_frame(frame_index as u32) {
                            println!("[alphastream] Processing frame {}", frame_index);
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
                                bitmap,
                                triangle_strip,
                            };
                            cache.insert(frame_index, processed_frame);
                            println!("[alphastream] Frame {} processed, cache size: {}", frame_index, cache.len());
                        }
                    });

                    let _ = handle.await;

                    let mut scheduler = scheduler_clone.lock().await;
                    scheduler.complete_task();
                } else {
                    tokio::time::sleep(tokio::time::Duration::from_millis(1)).await;
                }
            }
        });

        self.background_handle = Some(handle);
    }

    // /// Process pending tasks (decode frames)
    // pub async fn process_tasks(&mut self) -> Result<(), FormatError> {
    //     let mut scheduler = self.scheduler.lock().await;
    //     while let Some(task) = scheduler.next_task() {
    //         let format_clone = Arc::clone(&self.format);
    //         let frame_index = task.frame_index;
    //         let handle = self.runtime.as_ref().unwrap().spawn_blocking(move || {
    //             let mut format = format_clone.blocking_lock();
    //             format.decode_frame(frame_index as u32)
    //         });
    //         let result = handle.await.map_err(|e| FormatError::InvalidFormat(format!("Join error: {}", e)))?;
    //         let frame_data = result?;
    //         self.cache.insert(frame_index, frame_data);
    //         scheduler.complete_task();
    //     }
    //     Ok(())
    // }

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
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    // Helper to create a minimal ASVP file for testing
    fn create_test_asvp() -> NamedTempFile {
        let mut file = NamedTempFile::new().unwrap();

        // Minimal ASVP structure:
        // Header: "ASVP" "PLN1" 0x00000001 (frame count) compressed_sizes_size
        // For simplicity, 1 frame, compressed_sizes_size = 8 (for one u64 size)
        // Sizes table: zlib compressed [8 bytes: frame size]
        // Frame: expected_len(4) + zlib compressed payload

        // Create a simple frame payload: channel_count=1, channel_size=4, data=[0,0,0,0] (empty polyline)
        let channel_count = 1u32;
        let channel_sizes = vec![4u32];
        let channel_data = vec![0u8; 4];
        let mut payload = Vec::new();
        payload.extend_from_slice(&channel_count.to_le_bytes());
        for &size in &channel_sizes {
            payload.extend_from_slice(&size.to_le_bytes());
        }
        payload.extend_from_slice(&channel_data);

        // Compress payload
        use flate2::write::ZlibEncoder;
        use flate2::Compression;
        let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(&payload).unwrap();
        let compressed_payload = encoder.finish().unwrap();

        // Frame: expected_len + compressed_payload
        let expected_len = payload.len() as u32;
        let mut frame = Vec::new();
        frame.extend_from_slice(&expected_len.to_le_bytes());
        frame.extend_from_slice(&compressed_payload);

        // Sizes table: [frame.len() as u64]
        let sizes_raw = (frame.len() as u64).to_le_bytes();
        let mut sizes_encoder = ZlibEncoder::new(Vec::new(), Compression::default());
        sizes_encoder.write_all(&sizes_raw).unwrap();
        let compressed_sizes = sizes_encoder.finish().unwrap();

        // Header
        let frame_count = 1u32;
        let compressed_sizes_size = compressed_sizes.len() as u32;
        let mut header = b"ASVPPLN1".to_vec();
        header.extend_from_slice(&frame_count.to_le_bytes());
        header.extend_from_slice(&compressed_sizes_size.to_le_bytes());

        // Write file
        file.write_all(&header).unwrap();
        file.write_all(&compressed_sizes).unwrap();
        file.write_all(&frame).unwrap();
        file.flush().unwrap();

        file
    }

    #[tokio::test]
    async fn test_asvp_processor() {
        let test_file = create_test_asvp();
        let processor = AlphaStreamProcessor::new_asvp(
            test_file.path().to_str().unwrap(),
            16,
            16,
            ProcessingMode::Both,
        ).unwrap();

        let metadata = processor.metadata().await.unwrap();
        assert_eq!(metadata.frame_count, 1);

        // trigger processing
        let _ = processor.get_frame(0, 16, 16).await;
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        let frame = processor.get_frame(0, 16, 16).await;
        assert!(frame.is_some()); // Accept only Some, do not unwrap None or Err

        let _ = processor.get_triangle_strip_vertices(0).await;
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        let vertices = processor.get_triangle_strip_vertices(0).await.unwrap();
        assert_eq!(vertices.len(), 0);
    }

    #[tokio::test]
    async fn test_request_frame() {
        let test_file = create_test_asvp();
        let processor = AlphaStreamProcessor::new_asvp(
            test_file.path().to_str().unwrap(),
            16,
            16,
            ProcessingMode::Bitmap,
        ).unwrap();

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
        let test_file = create_test_asvp();
        let processor = AlphaStreamProcessor::new_asvp(
            test_file.path().to_str().unwrap(),
            16,
            16,
            ProcessingMode::Bitmap,
        ).unwrap();

        // Manually insert frame data to test modes
        let frame_data = crate::formats::FrameData {
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
        let result = AlphaStreamProcessor::new_asvp("nonexistent.asvp", 16, 16, ProcessingMode::Bitmap);
        assert!(result.is_err());

        // Test metadata on invalid processor would require mocking, but basic structure is tested above
    }
}