//! API facade module
//!
//! This module provides the high-level API for AlphaStream processing,
//! integrating the format parsers, cache, scheduler, and rasterizer.

use std::fs::File;
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::cache::{FrameCache, FrameData};
use crate::formats::{ASFormat, ASVRFormat, ASVPFormat, FormatError};
use crate::rasterizer::PolystreamRasterizer;
use crate::runtime::Runtime;
use crate::scheduler::{Scheduler, Task};

/// Processing mode for rasterization
#[derive(Debug, Clone)]
pub enum ProcessingMode {
    Bitmap,
    TriangleStrip,
    Both,
}

/// High-level AlphaStream processor
pub struct AlphaStreamProcessor {
    /// Frame cache for decoded polystream data
    cache: FrameCache,
    /// Task scheduler for frame processing
    scheduler: Arc<Mutex<Scheduler>>,
    /// Format parser (either ASVR or ASVP)
    format: Arc<Mutex<Box<dyn ASFormat + Send + Sync>>>,
    /// Output dimensions
    width: u32,
    height: u32,
    /// Processing mode
    mode: ProcessingMode,
    /// Async runtime
    runtime: Option<Runtime>,
    /// Background processing task handle
    background_handle: Option<tokio::task::JoinHandle<()>>,
}

impl AlphaStreamProcessor {
    /// Create a new processor for ASVR (encrypted) files
    pub fn new_asvr(
        file_path: &str,
        scene_id: u32,
        version: &[u8],
        base_url: &[u8],
        width: u32,
        height: u32,
        mode: ProcessingMode,
    ) -> Result<Self, FormatError> {
        let file = File::open(file_path)?;
        let format = Arc::new(Mutex::new(Box::new(ASVRFormat::new(file, scene_id, version, base_url)?) as Box<dyn ASFormat + Send + Sync>));
        let cache = FrameCache::default();
        let scheduler = Arc::new(Mutex::new(Scheduler::new()));
        let runtime = Runtime::new().expect("Failed to create runtime");

        let mut processor = Self {
            cache,
            scheduler,
            format,
            width,
            height,
            mode,
            runtime: Some(runtime),
            background_handle: None,
        };
        processor.start_background_processing();
        Ok(processor)
    }

    /// Create a new processor for ASVP (plaintext) files
    pub fn new_asvp(file_path: &str, width: u32, height: u32, mode: ProcessingMode) -> Result<Self, FormatError> {
        let file = File::open(file_path)?;
        let format = Arc::new(Mutex::new(Box::new(ASVPFormat::new(file)?) as Box<dyn ASFormat + Send + Sync>));
        let cache = FrameCache::default();
        let scheduler = Arc::new(Mutex::new(Scheduler::new()));
        let runtime = Runtime::new().expect("Failed to create runtime");

        let mut processor = Self {
            cache,
            scheduler,
            format,
            width,
            height,
            mode,
            runtime: Some(runtime),
            background_handle: None,
        };
        processor.start_background_processing();
        Ok(processor)
    }

    /// Get metadata about the stream
    pub async fn metadata(&self) -> Result<crate::formats::Metadata, FormatError> {
        let mut format = self.format.lock().await;
        format.metadata()
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
    pub async fn get_frame(&self, frame_index: usize, _width: u32, _height: u32) -> Option<Vec<u8>> {
        if let Some(frame_data) = self.cache.get(frame_index) {
            if frame_data.bitmap.is_some() {
                return frame_data.bitmap.clone();
            }
        }
        // schedule processing
        let mut scheduler = self.scheduler.lock().await;
        let task = Task::with_priority(frame_index, 10);
        scheduler.schedule_task(task);
        None
    }

    /// Get triangle strip vertices for a frame
    pub async fn get_triangle_strip_vertices(&self, frame_index: usize) -> Option<Vec<f32>> {
        if let Some(frame_data) = self.cache.get(frame_index) {
            if frame_data.triangle_strip.is_some() {
                return frame_data.triangle_strip.clone();
            }
        }
        // schedule processing
        let mut scheduler = self.scheduler.lock().await;
        let task = Task::with_priority(frame_index, 10);
        scheduler.schedule_task(task);
        None
    }

    /// Request a frame for processing
    pub async fn request_frame(&self, frame_index: u32) -> Result<(), FormatError> {
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
    fn start_background_processing(&mut self) {
        let scheduler_clone = Arc::clone(&self.scheduler);
        let format_clone = Arc::clone(&self.format);
        let cache_clone = self.cache.clone();
        let width = self.width;
        let height = self.height;
        let mode = self.mode.clone();

        let handle = self.runtime.as_ref().unwrap().spawn(async move {
            loop {
                let task = {
                    let mut scheduler = scheduler_clone.lock().await;
                    scheduler.next_task()
                };
                if let Some(task) = task {
                    let frame_index = task.frame_index;
                    let format = Arc::clone(&format_clone);
                    let cache = cache_clone.clone();
                    let width = width;
                    let height = height;
                    let mode = mode.clone();

                    let handle = tokio::task::spawn_blocking(move || {
                        let mut format = format.blocking_lock();
                        if let Ok(frame_data) = format.decode_frame(frame_index as u32) {
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

    /// Process pending tasks (decode frames)
    pub async fn process_tasks(&mut self) -> Result<(), FormatError> {
        let mut scheduler = self.scheduler.lock().await;
        while let Some(task) = scheduler.next_task() {
            let format_clone = Arc::clone(&self.format);
            let frame_index = task.frame_index;
            let handle = self.runtime.as_ref().unwrap().spawn_blocking(move || {
                let mut format = format_clone.blocking_lock();
                format.decode_frame(frame_index as u32)
            });
            let result = handle.await.map_err(|e| FormatError::InvalidFormat(format!("Join error: {}", e)))?;
            let frame_data = result?;
            self.cache.insert(frame_index, frame_data);
            scheduler.complete_task();
        }
        Ok(())
    }

}

impl Drop for AlphaStreamProcessor {
    fn drop(&mut self) {
        if let Some(handle) = self.background_handle.take() {
            handle.abort();
        }
        if let Some(runtime) = self.runtime.take() {
            std::thread::spawn(move || drop(runtime));
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
        let frame = processor.get_frame(0, 16, 16).await.unwrap();
        assert_eq!(frame.len(), 256);

        let _ = processor.get_triangle_strip_vertices(0).await;
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        let vertices = processor.get_triangle_strip_vertices(0).await.unwrap();
        assert_eq!(vertices.len(), 0);
    }
}