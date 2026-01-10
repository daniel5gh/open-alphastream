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

        // Prefetch test: sequential access triggers prefetch
        processor.get_frame(1, 16, 16).await;
        processor.get_frame(2, 16, 16).await;
        // Prefetch should have been triggered for frame 2
        // (no assertion here, but coverage for sequential prefetch logic)
            });
        }
        
        #[test]
        fn test_lru_eviction_edge_cases() {
            use libalphastream::FrameCache;
            use libalphastream::formats::FrameData;
            // Small cache for eviction
            let cache = FrameCache::new(2);
            let d1 = FrameData { polystream: vec![1], bitmap: None, triangle_strip: None };
            let d2 = FrameData { polystream: vec![2], bitmap: None, triangle_strip: None };
            let d3 = FrameData { polystream: vec![3], bitmap: None, triangle_strip: None };
            cache.insert(1, d1);
            cache.insert(2, d2);
            assert!(cache.contains(&1));
            assert!(cache.contains(&2));
            cache.insert(3, d3); // Should evict 1
            assert!(!cache.contains(&1));
            assert!(cache.contains(&2));
            assert!(cache.contains(&3));
            // Access 2, insert 4, should evict 3
            cache.get(2);
            let d4 = FrameData { polystream: vec![4], bitmap: None, triangle_strip: None };
            cache.insert(4, d4);
            assert!(cache.contains(&2));
            assert!(!cache.contains(&3));
            assert!(cache.contains(&4));
        }
        
        #[test]
        fn test_scheduler_prefetch_and_backpressure_edge() {
            use libalphastream::FrameCache;
            use libalphastream::formats::FrameData;
            use libalphastream::scheduler::Scheduler;
            use std::sync::Arc;
            let cache = FrameCache::new(3);
            let mut scheduler = Scheduler::new();
            scheduler.set_cache(Arc::new(cache.clone()));
            // Fill cache
            for i in 0..3 {
                cache.insert(i, FrameData {
                    polystream: vec![i as u8],
                    bitmap: None,
                    triangle_strip: None,
                });
            }
            scheduler.prefetch(0);
            assert!(scheduler.next_task().is_none());
            // Remove one, prefetch should schedule one
            cache.remove(&0);
            scheduler.prefetch(0);
            let t = scheduler.next_task();
            assert!(t.is_some());
            // Backpressure: simulate max_concurrent
            // simulate backpressure by scheduling and consuming a task, then not calling complete_task()
            // (max_concurrent is private, so we can't set it directly)
            scheduler.schedule_task(libalphastream::scheduler::Task::new(10));
            let _ = scheduler.next_task();
            assert!(scheduler.next_task().is_none());
        }
        
        #[test]
        fn test_cache_thread_safety_contention() {
            use std::sync::Arc;
            use std::thread;
            use libalphastream::FrameCache;
            use libalphastream::formats::FrameData;
            let cache = Arc::new(FrameCache::new(8));
            let mut handles = vec![];
            for i in 0..8 {
                let cache_clone = Arc::clone(&cache);
                handles.push(thread::spawn(move || {
                    for j in 0..100 {
                        let idx = (i + j) % 8;
                        let data = FrameData {
                            polystream: vec![idx as u8],
                            bitmap: Some(vec![idx as u8; 8]),
                            triangle_strip: Some(vec![idx as f32; 8]),
                        };
                        cache_clone.insert(idx, data);
                        let _ = cache_clone.get(idx);
                    }
                }));
            }
            for h in handles { h.join().unwrap(); }
            // All keys should be present
            for i in 0..8 {
                assert!(cache.contains(&i));
            }
        }
        
        #[test]
        fn test_scheduler_cache_integration_concurrent() {
            use std::sync::Arc;
            use std::thread;
            use libalphastream::FrameCache;
            use libalphastream::formats::FrameData;
            use libalphastream::scheduler::{Scheduler, Task};
            let cache = Arc::new(FrameCache::new(5));
            let mut scheduler = Scheduler::new();
            scheduler.set_cache(Arc::clone(&cache));
            let mut handles = vec![];
            for i in 0..5 {
                let cache_clone = Arc::clone(&cache);
                handles.push(thread::spawn(move || {
                    let data = FrameData {
                        polystream: vec![i as u8],
                        bitmap: Some(vec![i as u8; 5]),
                        triangle_strip: Some(vec![i as f32; 5]),
                    };
                    cache_clone.insert(i, data);
                }));
            }
            for h in handles { h.join().unwrap(); }
            // Scheduler should see cache as full
            scheduler.prefetch(0);
            assert!(scheduler.next_task().is_none());
        }

#[test]
fn test_c_abi_integration() {
    use std::ffi::CString;

    // Create handle
    let handle = CV_create();
    assert!(!handle.is_null());

    // Initialize
    // Use a real test file for processor-backed FFI
    let test_file = create_test_asvp();
    let base_url = CString::new(test_file.path().to_str().unwrap()).unwrap();
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

    // Get frame (wait for processing)
    let mut frame_ptr = CV_get_frame(handle, 0);
    let mut tries = 0;
    while frame_ptr.is_null() && tries < 20 {
        std::thread::sleep(std::time::Duration::from_millis(50));
        frame_ptr = CV_get_frame(handle, 0);
        tries += 1;
    }
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
    scheduler.set_cache(std::sync::Arc::new(cache.clone()));

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

    // Test adaptive prefetching/backpressure: fill cache, then prefetch should not schedule more
    let mut scheduler2 = Scheduler::new();
    let cache2 = FrameCache::new(3);
    scheduler2.set_cache(std::sync::Arc::new(cache2.clone()));
    for i in 0..3 {
        cache2.insert(i, formats::FrameData {
            polystream: vec![i as u8],
            bitmap: Some(vec![255; 100]),
            triangle_strip: Some(vec![0.0; 10]),
        });
    }
    scheduler2.prefetch(2); // cache is full, should not schedule
    assert!(scheduler2.next_task().is_none());
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