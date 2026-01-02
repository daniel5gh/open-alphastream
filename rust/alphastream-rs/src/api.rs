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
use crate::scheduler::{Scheduler, Task};

/// High-level AlphaStream processor
pub struct AlphaStreamProcessor {
    /// Frame cache for decoded polystream data
    cache: FrameCache,
    /// Task scheduler for frame processing
    scheduler: Arc<Mutex<Scheduler>>,
    /// Format parser (either ASVR or ASVP)
    format: Box<dyn ASFormat + Send + Sync>,
    /// Output dimensions
    width: u32,
    height: u32,
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
    ) -> Result<Self, FormatError> {
        let file = File::open(file_path)?;
        let format = Box::new(ASVRFormat::new(file, scene_id, version, base_url)?);
        let cache = FrameCache::default();
        let scheduler = Arc::new(Mutex::new(Scheduler::new()));

        Ok(Self {
            cache,
            scheduler,
            format,
            width,
            height,
        })
    }

    /// Create a new processor for ASVP (plaintext) files
    pub fn new_asvp(file_path: &str, width: u32, height: u32) -> Result<Self, FormatError> {
        let file = File::open(file_path)?;
        let format = Box::new(ASVPFormat::new(file)?);
        let cache = FrameCache::default();
        let scheduler = Arc::new(Mutex::new(Scheduler::new()));

        Ok(Self {
            cache,
            scheduler,
            format,
            width,
            height,
        })
    }

    /// Get metadata about the stream
    pub fn metadata(&mut self) -> Result<crate::formats::Metadata, FormatError> {
        self.format.metadata()
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

    /// Process pending tasks (decode frames)
    pub async fn process_tasks(&mut self) -> Result<(), FormatError> {
        let mut scheduler = self.scheduler.lock().await;
        while let Some(task) = scheduler.next_task() {
            let frame_data = self.format.decode_frame(task.frame_index as u32)?;
            self.cache.insert(task.frame_index, frame_data);
            scheduler.complete_task();
        }
        Ok(())
    }

    /// Get a rasterized frame (R8 mask)
    pub async fn get_frame(&mut self, frame_index: u32) -> Result<Vec<u8>, FormatError> {
        // Ensure frame is available
        self.request_frame(frame_index).await?;
        self.process_tasks().await?;

        // Get from cache and rasterize
        if let Some(frame_data) = self.cache.get(frame_index as usize) {
            let mut mask = vec![0u8; (self.width * self.height) as usize];

            // Rasterize each channel
            let mut offset = 0;
            for &size in &frame_data.channel_sizes {
                let channel_data = &frame_data.channel_data[offset..offset + size as usize];
                let channel_mask = PolystreamRasterizer::rasterize(channel_data, self.width, self.height);

                // Combine with existing mask (OR operation for multiple channels)
                for (i, &pixel) in channel_mask.iter().enumerate() {
                    if pixel > 0 {
                        mask[i] = 255;
                    }
                }
                offset += size as usize;
            }

            Ok(mask)
        } else {
            Err(FormatError::InvalidFormat("Frame not found in cache".to_string()))
        }
    }

    /// Get triangle strip vertices for a frame
    pub async fn get_triangle_strip(&mut self, frame_index: u32) -> Result<Vec<f32>, FormatError> {
        // Ensure frame is available
        self.request_frame(frame_index).await?;
        self.process_tasks().await?;

        if let Some(frame_data) = self.cache.get(frame_index as usize) {
            let mut vertices = Vec::new();

            // Convert each channel to triangle strip
            let mut offset = 0;
            for &size in &frame_data.channel_sizes {
                let channel_data = &frame_data.channel_data[offset..offset + size as usize];
                let channel_strip = PolystreamRasterizer::polystream_to_triangle_strip(channel_data);
                vertices.extend(channel_strip);
                offset += size as usize;
            }

            Ok(vertices)
        } else {
            Err(FormatError::InvalidFormat("Frame not found in cache".to_string()))
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
        let mut processor = AlphaStreamProcessor::new_asvp(
            test_file.path().to_str().unwrap(),
            16,
            16,
        ).unwrap();

        let metadata = processor.metadata().unwrap();
        assert_eq!(metadata.frame_count, 1);

        let frame = processor.get_frame(0).await.unwrap();
        assert_eq!(frame.len(), 256); // 16x16

        let vertices = processor.get_triangle_strip(0).await.unwrap();
        // Empty polyline should give empty vertices
        assert_eq!(vertices.len(), 0);
    }
}