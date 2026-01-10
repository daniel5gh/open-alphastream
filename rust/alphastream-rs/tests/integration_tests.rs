use libalphastream::*;
use libalphastream::{CV_create, CV_destroy, CV_init, CV_get_frame, CV_get_triangle_strip_vertices};
use std::io::Write;
use tempfile::NamedTempFile;

// Helper to create a test ASVP file
fn create_test_asvp() -> NamedTempFile {
    let mut file = NamedTempFile::new().unwrap();

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

#[test]
fn test_full_processor_lifecycle() {
    let test_file = create_test_asvp();

    // Create processor
    let processor = AlphaStreamProcessor::new_asvp(
        test_file.path().to_str().unwrap(),
        16,
        16,
        ProcessingMode::Both,
    ).unwrap();

    // Get metadata
    let metadata = tokio::runtime::Runtime::new().unwrap().block_on(async {
        processor.metadata().await.unwrap()
    });
    assert_eq!(metadata.frame_count, 1);

    // Request and get frame
    let runtime = tokio::runtime::Runtime::new().unwrap();
    runtime.block_on(async {
        processor.request_frame(0).await.unwrap();

        // Wait a bit for processing
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        let frame = processor.get_frame(0, 16, 16).await.unwrap();
        assert_eq!(frame.len(), 256); // 16x16

        let vertices = processor.get_triangle_strip_vertices(0).await.unwrap();
        assert_eq!(vertices.len(), 0); // Empty for this test data
    });
}

#[test]
fn test_c_abi_integration() {
    use std::ffi::CString;

    // Create handle
    let handle = CV_create();
    assert!(!handle.is_null());

    // Initialize
    let base_url = CString::new("https://example.com").unwrap();
    let version = CString::new("1.0.0").unwrap();

    let success = CV_init(
        handle,
        base_url.as_ptr(),
        123,
        32,
        32,
        version.as_ptr(),
        0,
        1024,
        512,
        256,
        5000,
        30000,
    );
    assert!(success);

    // Get frame
    let frame_ptr = CV_get_frame(handle, 0);
    assert!(!frame_ptr.is_null());

    // Verify frame data
    unsafe {
        let frame_data = std::slice::from_raw_parts(frame_ptr as *const u8, 1024); // 32x32
        assert_eq!(frame_data.len(), 1024);
    }

    // Get triangle strip
    let mut vertices: *const f32 = std::ptr::null();
    let mut count: usize = 0;
    let success = CV_get_triangle_strip_vertices(handle, 0, &mut vertices, &mut count);
    assert!(success);

    // Cleanup
    CV_destroy(handle);
}

#[test]
fn test_cache_scheduler_integration() {
    let cache = FrameCache::new(10);
    let mut scheduler = Scheduler::new();

    // Schedule some tasks
    for i in 0..5 {
        let task = scheduler::Task::new(i);
        scheduler.schedule_task(task);
    }

    // Process tasks and cache results
    while let Some(task) = scheduler.next_task() {
        let frame_data = formats::FrameData {
            polystream: vec![task.frame_index as u8],
            bitmap: Some(vec![255; 100]),
            triangle_strip: Some(vec![0.0; 10]),
        };
        cache.insert(task.frame_index, frame_data);
        scheduler.complete_task();
    }

    // Verify all frames cached
    for i in 0..5 {
        assert!(cache.contains(&i));
        let data = cache.get(i).unwrap();
        assert_eq!(data.polystream[0], i as u8);
    }
}

#[test]
fn test_error_propagation() {
    // Test API error handling
    let result = AlphaStreamProcessor::new_asvp(
        "nonexistent_file.asvp",
        16,
        16,
        ProcessingMode::Bitmap,
    );
    assert!(result.is_err());

    // Test C ABI error handling
    let null_frame = CV_get_frame(std::ptr::null_mut(), 0);
    assert!(null_frame.is_null());
}

#[test]
fn test_concurrent_access() {
    use std::thread;
    use std::sync::Arc;

    let cache = Arc::new(FrameCache::new(100));
    let mut handles = vec![];

    // Spawn threads that access cache concurrently
    for i in 0..10 {
        let cache_clone = Arc::clone(&cache);
        let handle = thread::spawn(move || {
            let data = FrameData {
                polystream: vec![i as u8],
                bitmap: Some(vec![i as u8; 64]),
                triangle_strip: Some(vec![i as f32; 32]),
            };
            cache_clone.insert(i, data);

            // Read it back
            let retrieved = cache_clone.get(i).unwrap();
            assert_eq!(retrieved.polystream[0], i as u8);
        });
        handles.push(handle);
    }

    // Wait for all threads
    for handle in handles {
        handle.join().unwrap();
    }

    // Verify final state
    assert_eq!(cache.len(), 10);
}