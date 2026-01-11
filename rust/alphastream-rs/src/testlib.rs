//! Test utilities for alphastream-rs
//! Provides helpers for creating test ASVP files and other test resources

use crate::formats::{ASVPWriter, ASVRWriter, FrameData, FormatError};
use tempfile::NamedTempFile;

/// Generate random polystream channel data
/// Creates a single channel with random byte data
fn generate_random_polystream_data(size: usize) -> Vec<u8> {
    // Create frame payload with channel header
    let channel_count = 1u32; // Single channel
    let channel_size = size as u32;

    let mut payload = Vec::new();
    payload.extend_from_slice(&channel_count.to_le_bytes());
    payload.extend_from_slice(&channel_size.to_le_bytes());

    // Generate predictable data
    // range of numbers from 0 to size
    let channel_data: Vec<u8> = (0..size).map(|x| x as u8).collect();
    payload.extend_from_slice(&channel_data);

    payload
}

/// Create a test ASVP file as a temporary file
///
/// Generates `frame_count` frames with random polystream data
/// and writes them using ASVPWriter
pub fn create_test_asvp(frame_count: u32) -> Result<NamedTempFile, FormatError> {
    let file = NamedTempFile::new()?;
    let mut writer = ASVPWriter::new(file);

    // Generate and add frames with random polystream data
    for _ in 0..frame_count {
        let channel_data = generate_random_polystream_data(64);
        let frame = FrameData {
            polystream: channel_data,
            bitmap: None,
            triangle_strip: None,
        };
        writer.add_frame(frame);
    }

    // write_all consumes self and returns the inner writer (the NamedTempFile)
    let file = writer.write_all()?;
    Ok(file)
}

/// Create a test ASVR file as a temporary file
///
/// Generates `frame_count` frames with random polystream data
/// and writes them using ASVRWriter with the given encryption parameters
pub fn create_test_asvr(
    scene_id: u32,
    version: &[u8],
    frame_count: u32,
) -> Result<NamedTempFile, FormatError> {
    let file = NamedTempFile::new()?;
    let file_path = file.path().to_path_buf();
    let base_url = file_path.file_name().unwrap().to_str().unwrap();
    let mut writer = ASVRWriter::new(file, scene_id, version, base_url.as_bytes())?;

    // Generate and add frames with random polystream data
    for _ in 0..frame_count {
        let channel_data = generate_random_polystream_data(64);
        let frame = FrameData {
            polystream: channel_data,
            bitmap: None,
            triangle_strip: None,
        };
        writer.add_frame(frame);
    }

    // write_all consumes self and returns the inner writer (the NamedTempFile)
    let file = writer.write_all()?;
    Ok(file)
}
