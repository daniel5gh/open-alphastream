//! Test utilities for alphastream-rs
// Provides helpers for creating test ASVP files and other test resources

/// Create a minimal ASVR file for testing (encrypted, 1 frame, valid structure)
// pub fn create_test_asvr() -> tempfile::NamedTempFile {
//     use tempfile::NamedTempFile;
//     use crate::formats::write_test_asvr_file;
//     let mut file = NamedTempFile::new().unwrap();
//     // Must match test values in builder test
//     let scene_id = 42u32;
//     let version = b"1.5.0";
//     let base_url = b"test.asvr";
//     write_test_asvr_file(&mut file, scene_id, version, base_url).unwrap();
//     file
// }

use tempfile::NamedTempFile;
use std::io::Write;

/// Create a minimal ASVP file for testing
pub fn create_test_asvp() -> NamedTempFile {
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
