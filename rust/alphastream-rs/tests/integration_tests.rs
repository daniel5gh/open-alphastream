use libalphastream::*;
use libalphastream::{CV_create, CV_destroy, CV_init, CV_get_frame, CV_get_triangle_strip_vertices};
use libalphastream::testlib::create_test_asvr;
// Centralized test utility import
use crate::testlib::create_test_asvp;
use std::time::{Duration, Instant};
use tokio::time::Instant as TokioInstant;

#[tokio::test]
async fn test_full_processor_lifecycle() {
    let test_file = create_test_asvp(1).unwrap();

    // Create processor
    let processor = AlphaStreamProcessor::new_asvp(
        test_file.path().to_str().unwrap(),
        16,
        16,
        ProcessingMode::Both,
    ).await.unwrap();

    // Get metadata
    let metadata = processor.metadata().await.unwrap();
    assert_eq!(metadata.frame_count, 1);

    // Request and get frame
    processor.request_frame(0).await.unwrap();

    let start = TokioInstant::now();
    let frame = loop {
        if let Some(f) = processor.get_frame(0, 16, 16).await {
            break f;
        }
        if start.elapsed() > tokio::time::Duration::from_millis(1000) {
            panic!("frame not ready in time");
        }
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
    };
    assert_eq!(frame.len(), 256); // 16x16

    let start = TokioInstant::now();
    let vertices = loop {
        if let Some(v) = processor.get_triangle_strip_vertices(0).await {
            break v;
        }
        if start.elapsed() > tokio::time::Duration::from_millis(1000) {
            panic!("vertices not ready in time");
        }
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
    };
    assert_eq!(vertices.len(), 174); // Empty for this test data

    // Prefetch test: sequential access triggers prefetch
    let _ = processor.get_frame(1, 16, 16).await; // may fail if no frame 1
    let _ = processor.get_frame(2, 16, 16).await; // may fail if no frame 2
    // Prefetch should have been triggered for frame 2
    // (no assertion here, but coverage for sequential prefetch logic)
        }
        
        #[test]
        fn test_ring_buffer_range_handling() {
            use libalphastream::FrameCache;
            use libalphastream::serializers::FrameData;
            // Test ring buffer range behavior (replaces LRU eviction test)
            let cache = FrameCache::new(3);
            let d0 = FrameData { polystream: vec![0], bitmap: None, triangle_strip: None };
            let d1 = FrameData { polystream: vec![1], bitmap: None, triangle_strip: None };
            let d2 = FrameData { polystream: vec![2], bitmap: None, triangle_strip: None };
            
            // Insert frames 0, 1, 2 (all within initial range [0, 3))
            cache.insert(0, d0);
            cache.insert(1, d1);
            cache.insert(2, d2);
            assert!(cache.contains(&0));
            assert!(cache.contains(&1));
            assert!(cache.contains(&2));
            
            // Frame 3 is out of range initially
            let d3 = FrameData { polystream: vec![3], bitmap: None, triangle_strip: None };
            assert!(!cache.insert(3, d3.clone())); // Should fail, out of range
            
            // Advance start to make room for frame 3
            cache.advance_start(1);
            // Now range is [1, 4), frame 0 is out of range
            assert!(!cache.is_in_range(0));
            assert!(cache.is_in_range(1));
            assert!(cache.is_in_range(3));
            
            // Now we can insert frame 3
            assert!(cache.insert(3, d3));
            assert!(cache.contains(&3));
        }
        
        #[test]
        fn test_scheduler_prefetch_and_backpressure_edge() {
            use libalphastream::FrameCache;
            use libalphastream::serializers::FrameData;
            use libalphastream::scheduler::Scheduler;
            use std::sync::Arc;
            let cache = Arc::new(FrameCache::new(4));
            let mut scheduler = Scheduler::new();
            scheduler.set_cache(Arc::clone(&cache));
            scheduler.set_prefetch_count(4);
            
            // Fill cache with frames 0, 1, 2, 3 (entire capacity)
            for i in 0..4 {
                cache.insert(i, FrameData {
                    polystream: vec![i as u8],
                    bitmap: None,
                    triangle_strip: None,
                });
            }
            
            // Prefetch at frame 0 should not schedule (cache full, all Ready)
            scheduler.prefetch(0);
            assert!(scheduler.next_task().is_none());
            
            // Advance start to make room for new frames
            // New range is [2, 6), frames 0 and 1 are now out of range
            cache.advance_start(2);
            
            // Now prefetch should be able to schedule frames 4, 5 (within new range)
            scheduler.prefetch(2);
            let t = scheduler.next_task();
            assert!(t.is_some());
            let frame_idx = t.unwrap().frame_index;
            assert!(frame_idx >= 3 && frame_idx < 6, "Expected frame in [3,6), got {}", frame_idx);
            
            // Backpressure: simulate max_concurrent by not calling complete_task()
            // Schedule another task and try to get it without completing the previous one
            scheduler.schedule_task(libalphastream::scheduler::Task::new(4));
            // With max_concurrent defaulting to 16, we need to exhaust it
            // For simplicity, just verify the mechanism works
            scheduler.set_max_concurrent(1);
            // Already have 1 active task from above, so next should return None
            assert!(scheduler.next_task().is_none());
        }
        
        #[test]
        fn test_cache_thread_safety_contention() {
            use std::sync::Arc;
            use std::thread;
            use libalphastream::FrameCache;
            use libalphastream::serializers::FrameData;
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
            use libalphastream::serializers::FrameData;
            use libalphastream::scheduler::{Scheduler};
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

    // Initialize with a real test file
    let version = CString::new("1.0.0").unwrap();
    let test_file = create_test_asvr(123, version.as_bytes(), 1).unwrap();
    let test_path = test_file.path().to_str().unwrap();
    let base_url = CString::new(test_path).unwrap();

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
    let start = Instant::now();
    let mut frame_ptr = CV_get_frame(handle, 0);
    while frame_ptr.is_null() && start.elapsed() < Duration::from_millis(500) {
        std::thread::sleep(Duration::from_millis(10));
        frame_ptr = CV_get_frame(handle, 0);
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
    let start = Instant::now();
    let mut success = CV_get_triangle_strip_vertices(handle, 0, &mut vertices, &mut count);
    while !success && start.elapsed() < Duration::from_millis(500) {
        std::thread::sleep(Duration::from_millis(10));
        success = CV_get_triangle_strip_vertices(handle, 0, &mut vertices, &mut count);
    }
    assert!(success);
    assert_eq!(count, 174);

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
        let frame_data = serializers::FrameData {
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
        cache2.insert(i, serializers::FrameData {
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
    let rt = tokio::runtime::Runtime::new().unwrap();
    let result = rt.block_on(async {
        AlphaStreamProcessor::new_asvp(
            "nonexistent_file.asvp",
            16,
            16,
            ProcessingMode::Bitmap,
        ).await
    });
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