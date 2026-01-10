// Cache module
// Implements a thread-safe LRU (Least Recently Used) cache for decoded frames.
// - Fixed capacity (default 512): When full, least-recently-used frames are evicted.
// - Thread-safe: Multiple threads can access safely via Arc<RwLock<...>>.
// - Used by the main processor and scheduler for frame reuse and deduplication.
// For novices: Like a smart box that keeps the most recently used frames, automatically removing old ones when full, and safe for many workers at once.

// Re-export FrameData from formats module
pub use crate::formats::FrameData;

use lru::LruCache;
use std::num::NonZeroUsize;
use std::sync::{Arc, RwLock};

/// Thread-safe LRU cache for frame data.
/// Arc allows sharing between threads, RwLock allows multiple readers or one writer.
/// For novices: This is like a shared storage locker where multiple workers can read simultaneously,
/// but only one can write at a time, and old items are automatically removed when full.
/// Thread-safe, fixed-capacity LRU cache for decoded frames.
pub struct FrameCache {
    /// The underlying LRU cache, protected by a read-write lock for thread safety.
    /// Arc = Atomic Reference Count (safe sharing), RwLock = Read-Write Lock (concurrent access)
    cache: Arc<RwLock<LruCache<usize, FrameData>>>,
}

impl FrameCache {
    /// Create a new FrameCache with the specified capacity.
    /// # Arguments
    /// * `capacity` - Maximum number of frames to store. When full, LRU eviction occurs.
    /// # Panics
    /// Panics if capacity is zero.
    pub fn new(capacity: usize) -> Self {
        assert!(capacity > 0, "FrameCache capacity must be > 0");
        Self {
            cache: Arc::new(RwLock::new(LruCache::new(NonZeroUsize::new(capacity).unwrap()))),
        }
    }

    /// Create a new FrameCache with default capacity of 512.
    /// This is the recommended default for most use cases.
    pub fn default() -> Self {
        Self::new(512)
    }

    /// Insert a frame into the cache.
    /// If the cache is full, the least recently used frame is evicted.
    /// # Arguments
    /// * `frame_index` - The frame number as cache key.
    /// * `frame_data` - The decoded frame data to store.
    pub fn insert(&self, frame_index: usize, frame_data: FrameData) {
        let mut cache = self.cache.write().unwrap();
        cache.put(frame_index, frame_data);
    }

    /// Get a frame from the cache.
    /// Returns Some(frame_data) if found, None if not in cache.
    /// Accessing a frame marks it as "recently used" (affects eviction order).
    /// # Arguments
    /// * `frame_index` - The frame number to look up.
    pub fn get(&self, frame_index: usize) -> Option<FrameData> {
        let mut cache = self.cache.write().unwrap(); // Write lock needed because get() updates LRU order
        cache.get(&frame_index).map(|fd| fd.clone()) // Clone because we return owned data
    }

    /// Check if a frame is in the cache without marking it as recently used.
    /// # Arguments
    /// * `frame_index` - The frame number to check.
    pub fn contains(&self, frame_index: &usize) -> bool {
        let cache = self.cache.read().unwrap();
        cache.contains(frame_index)
    }

    /// Remove a frame from the cache.
    /// # Arguments
    /// * `frame_index` - The frame number to remove.
    pub fn remove(&self, frame_index: &usize) -> Option<FrameData> {
        let mut cache = self.cache.write().unwrap();
        cache.pop(frame_index)
    }

    /// Get the current number of frames in the cache.
    pub fn len(&self) -> usize {
        let cache = self.cache.read().unwrap();
        cache.len()
    }

    /// Check if the cache is empty.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Clear all frames from the cache.
    pub fn clear(&self) {
        let mut cache = self.cache.write().unwrap();
        cache.clear();
    }

    /// Get the capacity of the cache.
    pub fn capacity(&self) -> usize {
        let cache = self.cache.read().unwrap();
        cache.cap().get()
    }
}

impl Clone for FrameCache {
    /// Clone the FrameCache, sharing the same underlying cache (Arc).
    fn clone(&self) -> Self {
        Self {
            cache: Arc::clone(&self.cache),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scheduler::Scheduler;
    use std::sync::Arc;

    #[test]
    fn test_cache_scheduler_adaptive_integration() {
        let cache = FrameCache::new(3);
        let mut scheduler = Scheduler::new();
        scheduler.set_cache(Arc::new(cache.clone()));

        // Fill cache to capacity
        for i in 0..3 {
            cache.insert(i, FrameData {
                polystream: vec![i as u8],
                bitmap: None,
                triangle_strip: None,
            });
        }
        // Scheduler should not dispatch new tasks if cache is full
        scheduler.prefetch(2);
        assert!(scheduler.next_task().is_none());

        // Remove one item, now scheduler can dispatch one task
        cache.remove(&0);
        scheduler.prefetch(2);
        let task = scheduler.next_task();
        assert!(task.is_some());
        assert_eq!(task.unwrap().frame_index, 3);
    }

    #[test]
    fn test_cache_insert_and_get() {
        let cache = FrameCache::default();
        let data = FrameData {
            polystream: vec![1, 2, 3, 4],
            bitmap: None,
            triangle_strip: None,
        };

        cache.insert(1, data.clone());
        let retrieved = cache.get(1).unwrap();
        assert_eq!(retrieved.polystream, data.polystream);
    }

    #[test]
    fn test_cache_miss() {
        let cache = FrameCache::default();
        assert!(cache.get(999).is_none());
    }

    #[test]
    fn test_cache_capacity() {
        let cache = FrameCache::new(2);
        let data1 = FrameData { polystream: vec![1], bitmap: None, triangle_strip: None };
        let data2 = FrameData { polystream: vec![2], bitmap: None, triangle_strip: None };
        let data3 = FrameData { polystream: vec![3], bitmap: None, triangle_strip: None };
        cache.insert(1, data1);
        cache.insert(2, data2);
        cache.insert(3, data3); // Should evict 1

        assert_eq!(cache.len(), 2);
        assert!(cache.get(1).is_none());
        assert!(cache.get(2).is_some());
        assert!(cache.get(3).is_some());
    }

    #[test]
    fn test_lru_behavior() {
        let cache = FrameCache::new(2);
        let data1 = FrameData { polystream: vec![1], bitmap: None, triangle_strip: None };
        let data2 = FrameData { polystream: vec![2], bitmap: None, triangle_strip: None };
        let data3 = FrameData { polystream: vec![3], bitmap: None, triangle_strip: None };
        cache.insert(1, data1);
        cache.insert(2, data2);
        cache.get(1); // Access 1, making it most recent
        cache.insert(3, data3); // Should evict 2

        assert!(cache.get(1).is_some());
        assert!(cache.get(2).is_none());
        assert!(cache.get(3).is_some());
    }

    #[test]
    fn test_cache_operations() {
        let cache = FrameCache::default();
        assert!(cache.is_empty());

        let data = FrameData { polystream: vec![1], bitmap: None, triangle_strip: None };
        cache.insert(1, data);
        assert!(!cache.is_empty());
        assert_eq!(cache.len(), 1);
        assert!(cache.contains(&1));

        cache.remove(&1);
        assert!(cache.is_empty());
        assert!(!cache.contains(&1));
    }

    #[test]
    fn test_clone() {
        let cache = FrameCache::default();
        let data = FrameData { polystream: vec![1], bitmap: None, triangle_strip: None };
        cache.insert(1, data);

        let cache2 = cache.clone();
        assert!(cache2.contains(&1));

        let data2 = FrameData { polystream: vec![2], bitmap: None, triangle_strip: None };
        cache2.insert(2, data2);
        assert!(cache.contains(&2)); // Shared state
    }

    #[test]
    fn test_error_scenarios() {
        let cache = FrameCache::new(2); // Small cache

        // Test inserting None/empty data
        let empty_data = FrameData { polystream: vec![], bitmap: None, triangle_strip: None };
        cache.insert(0, empty_data);
        assert!(cache.contains(&0));

        // Test removing non-existent
        assert!(cache.remove(&999).is_none());

        // Test capacity limits
        let data1 = FrameData { polystream: vec![1], bitmap: None, triangle_strip: None };
        let data2 = FrameData { polystream: vec![2], bitmap: None, triangle_strip: None };
        let data3 = FrameData { polystream: vec![3], bitmap: None, triangle_strip: None };
        cache.insert(1, data1);
        cache.insert(2, data2);
        cache.insert(3, data3); // Should evict 0 (least recently used)

        assert!(!cache.contains(&0));
        // LRU eviction: after inserting 3 items into a cache of size 2, only the two most recently used remain
        // The actual eviction order depends on access pattern; here, keys 2 and 3 should remain
        assert!(!cache.contains(&1));
        assert!(cache.contains(&2));
        assert!(cache.contains(&3));
        assert!(cache.contains(&2));
        assert!(cache.contains(&3));
    }

    #[test]
    fn test_thread_safety() {
        use std::thread;
        use std::sync::Arc;

        let cache = Arc::new(FrameCache::new(10));

        let mut handles = vec![];

        // Spawn multiple threads writing to cache
        for i in 0..5 {
            let cache_clone = Arc::clone(&cache);
            let handle = thread::spawn(move || {
                let data = FrameData {
                    polystream: vec![i as u8],
                    bitmap: Some(vec![i as u8; 100]),
                    triangle_strip: Some(vec![i as f32; 50]),
                };
                cache_clone.insert(i, data);
            });
            handles.push(handle);
        }

        // Wait for all threads
        for handle in handles {
            handle.join().unwrap();
        }

        // Verify all data was inserted
        for i in 0..5 {
            assert!(cache.contains(&i));
        }
    }
}