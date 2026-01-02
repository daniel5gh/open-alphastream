// Cache module
// This module implements an LRU (Least Recently Used) cache for frames with concurrency support.
// It uses a thread-safe LRU cache to store up to 512 frames, evicting the least recently used when full.

// Re-export FrameData from formats module
pub use crate::formats::FrameData;

use lru::LruCache;
use std::num::NonZeroUsize;
use std::sync::{Arc, RwLock};

/// Thread-safe LRU cache for frame data.
/// Uses Arc<RwLock<>> to allow concurrent access from multiple threads.
pub struct FrameCache {
    // The underlying LRU cache, protected by a read-write lock for thread safety.
    cache: Arc<RwLock<LruCache<usize, FrameData>>>,
}

impl FrameCache {
    /// Create a new FrameCache with the specified capacity.
    /// Defaults to 512 frames as per requirements.
    pub fn new(capacity: usize) -> Self {
        Self {
            cache: Arc::new(RwLock::new(LruCache::new(NonZeroUsize::new(capacity).unwrap()))),
        }
    }

    /// Create a new FrameCache with default capacity of 512.
    pub fn default() -> Self {
        Self::new(512)
    }

    /// Insert a frame into the cache.
    /// If the cache is full, the least recently used frame is evicted.
    pub fn insert(&self, frame_index: usize, frame_data: FrameData) {
        let mut cache = self.cache.write().unwrap();
        cache.put(frame_index, frame_data);
    }

    /// Get a frame from the cache.
    /// Returns Some(frame_data) if found, None if not in cache.
    /// Accessing a frame marks it as recently used.
    pub fn get(&self, frame_index: usize) -> Option<FrameData> {
        let mut cache = self.cache.write().unwrap();
        cache.get(&frame_index).map(|fd| fd.clone())
    }

    /// Check if a frame is in the cache without marking it as recently used.
    pub fn contains(&self, frame_index: &usize) -> bool {
        let cache = self.cache.read().unwrap();
        cache.contains(frame_index)
    }

    /// Remove a frame from the cache.
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
    /// Clone the FrameCache, sharing the same underlying cache.
    fn clone(&self) -> Self {
        Self {
            cache: Arc::clone(&self.cache),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}