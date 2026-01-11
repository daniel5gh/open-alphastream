// Cache module
// Implements a thread-safe ring buffer cache for decoded frames.
// - Fixed capacity (default 512): Sequential access optimized, oldest frames automatically overwritten.
// - Thread-safe: Multiple threads can access safely via Arc<RwLock<...>> and atomics.
// - Read-only get(): No write lock needed for reading, improving performance.
// - Designed for strictly sequential workloads with seek detection.
// For novices: Like a circular conveyor belt that holds frames in order, where new frames
// push out the oldest ones, and many workers can read at once without blocking each other.

// Re-export FrameData from formats module
pub use crate::formats::FrameData;

use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::RwLock;

/// Represents the state of a slot in the ring buffer.
/// - Empty: No frame data, slot is available
/// - InProgress: Frame is being fetched/decoded
/// - Ready: Frame data is available for use
#[derive(Clone, Debug)]
pub enum FrameSlot {
    /// Slot contains no frame data
    Empty,
    /// Frame is currently being fetched or decoded
    InProgress,
    /// Frame data is ready for use
    Ready(FrameData),
}

impl FrameSlot {
    /// Check if the slot contains ready frame data
    pub fn is_ready(&self) -> bool {
        matches!(self, FrameSlot::Ready(_))
    }

    /// Check if the slot is empty
    pub fn is_empty(&self) -> bool {
        matches!(self, FrameSlot::Empty)
    }

    /// Check if the slot is in progress
    pub fn is_in_progress(&self) -> bool {
        matches!(self, FrameSlot::InProgress)
    }

    /// Get the frame data if ready, None otherwise
    pub fn get_data(&self) -> Option<&FrameData> {
        match self {
            FrameSlot::Ready(data) => Some(data),
            _ => None,
        }
    }
}

/// Thread-safe ring buffer cache for frame data.
/// Optimized for sequential access patterns with efficient read operations.
/// 
/// # Design
/// - Uses a fixed-size ring buffer where frame indices map to buffer positions
/// - `get()` only requires a read lock (no write lock for recency tracking)
/// - Seek detection: backward moves or large forward jumps invalidate the cache
/// - Automatic overwriting of oldest frames (no explicit eviction needed)
/// 
/// # Thread Safety
/// - `RwLock` on buffer allows multiple concurrent readers
/// - `AtomicUsize` for play_head and start_index for lock-free position updates
/// - `Arc` wrapper enables sharing across threads
pub struct RingBufferCache {
    /// The ring buffer storage, each slot can be Empty, InProgress, or Ready
    buffer: RwLock<Vec<FrameSlot>>,
    /// Current play head (last requested frame index)
    play_head: AtomicUsize,
    /// Frame index corresponding to buffer[0]
    start_index: AtomicUsize,
    /// Fixed capacity of the ring buffer
    capacity: usize,
    /// Generation counter - incremented on cache invalidation (seek detection)
    /// Used to detect and discard stale decode task results
    generation: AtomicU64,
    /// Atomic counter for Ready slots - O(1) access instead of O(n) iteration
    ready_count: AtomicUsize,
    /// Atomic counter for InProgress slots - O(1) access instead of O(n) iteration
    in_progress_count: AtomicUsize,
}

impl RingBufferCache {
    /// Create a new RingBufferCache with the specified capacity.
    /// 
    /// # Arguments
    /// * `capacity` - Maximum number of frames to store. Must be greater than 0.
    /// 
    /// # Panics
    /// Panics if capacity is zero.
    pub fn new(capacity: usize) -> Self {
        assert!(capacity > 0, "RingBufferCache capacity must be > 0");
        
        // Initialize buffer with empty slots
        let buffer = (0..capacity).map(|_| FrameSlot::Empty).collect();
        
        Self {
            buffer: RwLock::new(buffer),
            play_head: AtomicUsize::new(0),
            start_index: AtomicUsize::new(0),
            capacity,
            generation: AtomicU64::new(0),
            ready_count: AtomicUsize::new(0),
            in_progress_count: AtomicUsize::new(0),
        }
    }

    /// Create a new RingBufferCache with default capacity of 512.
    /// This is the recommended default for most use cases as per PRD specifications.
    pub fn default() -> Self {
        Self::new(512)
    }

    /// Map a frame index to a buffer slot position.
    /// 
    /// # Arguments
    /// * `frame_index` - The frame number to map
    /// * `start` - The current start_index value
    /// 
    /// # Returns
    /// `Some(slot_index)` if frame is in valid range, `None` otherwise
    fn frame_to_slot(&self, frame_index: usize, start: usize) -> Option<usize> {
        if frame_index < start || frame_index >= start + self.capacity {
            None // Out of range
        } else {
            Some((frame_index - start) % self.capacity)
        }
    }

    /// Get a frame from the cache. **Read-only operation** - no write lock needed.
    /// 
    /// # Arguments
    /// * `frame_index` - The frame number to look up
    /// 
    /// # Returns
    /// `Some(FrameData)` if frame is in range and ready, `None` otherwise
    pub fn get(&self, frame_index: usize) -> Option<FrameData> {
        let start = self.start_index.load(Ordering::Acquire);
        
        let slot_index = self.frame_to_slot(frame_index, start)?;
        
        // Read lock only - no write needed for read operations
        let buffer = self.buffer.read().unwrap();
        
        match &buffer[slot_index] {
            FrameSlot::Ready(data) => Some(data.clone()),
            _ => None,
        }
    }

    /// Insert a completed frame into the appropriate slot.
    /// 
    /// # Arguments
    /// * `frame_index` - The frame number as cache key
    /// * `data` - The decoded frame data to store
    /// 
    /// # Returns
    /// `true` if insertion succeeded, `false` if frame is out of range
    pub fn insert(&self, frame_index: usize, data: FrameData) -> bool {
        let start = self.start_index.load(Ordering::Acquire);
        
        if let Some(slot_index) = self.frame_to_slot(frame_index, start) {
            let mut buffer = self.buffer.write().unwrap();
            // Track state transitions for atomic counters
            let old_state = &buffer[slot_index];
            let was_in_progress = old_state.is_in_progress();
            let was_ready = old_state.is_ready();
            
            buffer[slot_index] = FrameSlot::Ready(data);
            
            // Update counters based on state transition
            if was_in_progress {
                self.in_progress_count.fetch_sub(1, Ordering::Release);
            }
            if !was_ready {
                self.ready_count.fetch_add(1, Ordering::Release);
            }
            true
        } else {
            false // Out of range, don't insert
        }
    }

    /// Mark a slot as being loaded (in progress).
    /// Used by the scheduler to indicate a frame fetch is underway.
    /// 
    /// # Arguments
    /// * `frame_index` - The frame number being loaded
    /// 
    /// # Returns
    /// `true` if marking succeeded, `false` if frame is out of range
    pub fn mark_in_progress(&self, frame_index: usize) -> bool {
        let start = self.start_index.load(Ordering::Acquire);
        
        if let Some(slot_index) = self.frame_to_slot(frame_index, start) {
            let mut buffer = self.buffer.write().unwrap();
            // Only mark if currently empty (don't overwrite ready or in-progress)
            if buffer[slot_index].is_empty() {
                buffer[slot_index] = FrameSlot::InProgress;
                self.in_progress_count.fetch_add(1, Ordering::Release);
            }
            true
        } else {
            false // Out of range
        }
    }

    /// Update the play head position and detect seek events.
    ///
    /// # Seek Detection
    /// - **Backward seek**: new_frame < current_play_head → invalidate cache
    /// - **Large forward seek**: new_frame >= start_index + 2*capacity → invalidate cache (true seek)
    /// - **Normal forward beyond window**: Slide window forward to accommodate (sequential playback)
    /// - **Normal forward**: Just update play_head
    ///
    /// # Arguments
    /// * `frame_index` - The new play head position (last requested frame)
    ///
    /// # Returns
    /// `true` if a seek was detected (cache invalidated), `false` for normal forward play
    pub fn update_play_head(&self, frame_index: usize) -> bool {
        let current_play_head = self.play_head.load(Ordering::Acquire);
        let start = self.start_index.load(Ordering::Acquire);
        
        // Backward seek detection - invalidate cache
        if frame_index < current_play_head {
            self.invalidate_internal();
            self.start_index.store(frame_index, Ordering::Release);
            self.play_head.store(frame_index, Ordering::Release);
            return true;
        }
        
        // Very large forward seek (jump more than capacity ahead) - invalidate cache
        // This is a true seek, not sequential playback
        if frame_index >= start + 2 * self.capacity {
            self.invalidate_internal();
            self.start_index.store(frame_index, Ordering::Release);
            self.play_head.store(frame_index, Ordering::Release);
            return true;
        }
        
        // Forward movement that exceeds current window - slide window forward
        // This is sequential playback, preserve cached frames that are still ahead
        if frame_index >= start + self.capacity {
            // Advance start to make room: keep some buffer behind play head for re-requests
            // New start = frame_index - capacity/4 (keep 25% of capacity behind)
            let buffer_behind = self.capacity / 4;
            let new_start = if frame_index >= buffer_behind {
                frame_index - buffer_behind
            } else {
                0
            };
            self.advance_start(new_start);
        }
        
        // Normal forward movement - just update play_head
        self.play_head.store(frame_index, Ordering::Release);
        false
    }

    /// Check if a frame index is within the current buffer window.
    /// 
    /// # Arguments
    /// * `frame_index` - The frame number to check
    /// 
    /// # Returns
    /// `true` if the frame index is within [start_index, start_index + capacity)
    pub fn is_in_range(&self, frame_index: usize) -> bool {
        let start = self.start_index.load(Ordering::Acquire);
        frame_index >= start && frame_index < start + self.capacity
    }

    /// Check if a frame exists and is ready in the cache.
    /// Provided for backward compatibility with the previous LRU cache interface.
    /// 
    /// # Arguments
    /// * `frame_index` - The frame number to check
    /// 
    /// # Returns
    /// `true` if the frame is in range and has Ready data
    pub fn contains(&self, frame_index: &usize) -> bool {
        let start = self.start_index.load(Ordering::Acquire);
        
        if let Some(slot_index) = self.frame_to_slot(*frame_index, start) {
            let buffer = self.buffer.read().unwrap();
            buffer[slot_index].is_ready()
        } else {
            false
        }
    }

    /// Get the slot state for a specific frame (for scheduler use).
    /// 
    /// # Arguments
    /// * `frame_index` - The frame number to check
    /// 
    /// # Returns
    /// The FrameSlot state if in range, None if out of range
    pub fn get_slot_state(&self, frame_index: usize) -> Option<FrameSlot> {
        let start = self.start_index.load(Ordering::Acquire);
        
        if let Some(slot_index) = self.frame_to_slot(frame_index, start) {
            let buffer = self.buffer.read().unwrap();
            Some(buffer[slot_index].clone())
        } else {
            None
        }
    }

    /// Internal invalidation method - clears slots and increments generation.
    /// Called during seek events to invalidate stale data.
    fn invalidate_internal(&self) {
        let mut buffer = self.buffer.write().unwrap();
        for slot in buffer.iter_mut() {
            *slot = FrameSlot::Empty;
        }
        // Reset counters to 0
        self.ready_count.store(0, Ordering::Release);
        self.in_progress_count.store(0, Ordering::Release);
        // Increment generation so in-flight tasks will be rejected
        self.generation.fetch_add(1, Ordering::Release);
    }

    /// Clear all frames from the cache, resetting all slots to Empty.
    /// This also increments the generation counter to invalidate in-flight tasks.
    pub fn clear(&self) {
        self.invalidate_internal();
    }

    /// Get the current generation counter value.
    /// Used by tasks to check if their results are still valid.
    pub fn generation(&self) -> u64 {
        self.generation.load(Ordering::Acquire)
    }

    /// Get the capacity of the cache.
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// Get the current number of Ready frames in the cache.
    /// O(1) using atomic counter instead of O(n) iteration.
    pub fn len(&self) -> usize {
        self.ready_count.load(Ordering::Acquire)
    }
    
    /// Get the count of occupied slots (Ready + InProgress).
    /// O(1) for efficient backpressure checks.
    pub fn occupied_count(&self) -> usize {
        self.ready_count.load(Ordering::Acquire) + self.in_progress_count.load(Ordering::Acquire)
    }

    /// Check if the cache has no Ready frames.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Get the current play head position.
    pub fn get_play_head(&self) -> usize {
        self.play_head.load(Ordering::Acquire)
    }

    /// Get the current start index (frame index at buffer[0]).
    pub fn get_start_index(&self) -> usize {
        self.start_index.load(Ordering::Acquire)
    }

    /// Advance the start_index to allow caching newer frames.
    /// Used when the play head moves forward and we need to make room.
    /// 
    /// # Arguments
    /// * `new_start` - The new start_index value
    pub fn advance_start(&self, new_start: usize) {
        let current = self.start_index.load(Ordering::Acquire);
        if new_start > current {
            // Clear slots that will be reused
            let advance = new_start - current;
            let mut buffer = self.buffer.write().unwrap();
            
            for i in 0..advance.min(self.capacity) {
                let slot_index = (i) % self.capacity;
                // Track state transitions for atomic counters
                let old_state = &buffer[slot_index];
                if old_state.is_ready() {
                    self.ready_count.fetch_sub(1, Ordering::Release);
                } else if old_state.is_in_progress() {
                    self.in_progress_count.fetch_sub(1, Ordering::Release);
                }
                buffer[slot_index] = FrameSlot::Empty;
            }
            
            self.start_index.store(new_start, Ordering::Release);
        }
    }
}

impl Clone for RingBufferCache {
    /// Clone creates a new Arc reference to the same underlying data.
    /// Note: This creates a shallow clone that shares the same buffer.
    /// For a true deep clone, the internal Arc would need to be cloned.
    fn clone(&self) -> Self {
        // For the ring buffer, we need to actually clone all the data
        // since we don't use Arc internally for the buffer
        let buffer_data = self.buffer.read().unwrap().clone();
        
        Self {
            buffer: RwLock::new(buffer_data),
            play_head: AtomicUsize::new(self.play_head.load(Ordering::Acquire)),
            start_index: AtomicUsize::new(self.start_index.load(Ordering::Acquire)),
            capacity: self.capacity,
            generation: AtomicU64::new(self.generation.load(Ordering::Acquire)),
            ready_count: AtomicUsize::new(self.ready_count.load(Ordering::Acquire)),
            in_progress_count: AtomicUsize::new(self.in_progress_count.load(Ordering::Acquire)),
        }
    }
}

/// Type alias for backward compatibility with existing code.
/// New code should use RingBufferCache directly.
pub type FrameCache = RingBufferCache;

#[cfg(test)]
mod tests {
    use super::*;

    // Helper function to create test FrameData
    fn test_frame_data(id: u8) -> FrameData {
        FrameData {
            polystream: vec![id],
            bitmap: None,
            triangle_strip: None,
        }
    }

    #[test]
    fn test_new_cache() {
        let cache = RingBufferCache::new(10);
        assert_eq!(cache.capacity(), 10);
        assert!(cache.is_empty());
        assert_eq!(cache.len(), 0);
    }

    #[test]
    fn test_default_capacity() {
        let cache = RingBufferCache::default();
        assert_eq!(cache.capacity(), 512);
    }

    #[test]
    #[should_panic(expected = "capacity must be > 0")]
    fn test_zero_capacity_panics() {
        RingBufferCache::new(0);
    }

    #[test]
    fn test_insert_and_get() {
        let cache = RingBufferCache::new(10);
        let data = test_frame_data(42);
        
        assert!(cache.insert(0, data.clone()));
        
        let retrieved = cache.get(0).unwrap();
        assert_eq!(retrieved.polystream, vec![42]);
    }

    #[test]
    fn test_get_miss_empty_slot() {
        let cache = RingBufferCache::new(10);
        assert!(cache.get(0).is_none());
    }

    #[test]
    fn test_get_miss_out_of_range() {
        let cache = RingBufferCache::new(10);
        cache.insert(0, test_frame_data(1));
        
        // Out of range (beyond capacity)
        assert!(cache.get(15).is_none());
    }

    #[test]
    fn test_contains() {
        let cache = RingBufferCache::new(10);
        
        assert!(!cache.contains(&0));
        
        cache.insert(0, test_frame_data(1));
        assert!(cache.contains(&0));
        assert!(!cache.contains(&1));
    }

    #[test]
    fn test_is_in_range() {
        let cache = RingBufferCache::new(10);
        
        // Initial range is [0, 10)
        assert!(cache.is_in_range(0));
        assert!(cache.is_in_range(5));
        assert!(cache.is_in_range(9));
        assert!(!cache.is_in_range(10));
    }

    #[test]
    fn test_mark_in_progress() {
        let cache = RingBufferCache::new(10);
        
        assert!(cache.mark_in_progress(0));
        
        // Should still be in progress, not ready
        assert!(cache.get(0).is_none());
        assert!(!cache.contains(&0));
        
        // Check the slot state
        let state = cache.get_slot_state(0).unwrap();
        assert!(state.is_in_progress());
    }

    #[test]
    fn test_update_play_head_forward() {
        let cache = RingBufferCache::new(10);
        cache.insert(0, test_frame_data(1));
        
        // Normal forward movement
        let seek_detected = cache.update_play_head(5);
        assert!(!seek_detected);
        assert_eq!(cache.get_play_head(), 5);
        
        // Data should still be there
        assert!(cache.contains(&0));
    }

    #[test]
    fn test_update_play_head_backward_seek() {
        let cache = RingBufferCache::new(10);
        cache.insert(5, test_frame_data(1));
        cache.update_play_head(5);
        
        // Backward seek
        let seek_detected = cache.update_play_head(2);
        assert!(seek_detected);
        assert_eq!(cache.get_play_head(), 2);
        assert_eq!(cache.get_start_index(), 2);
        
        // Cache should be cleared
        assert!(cache.is_empty());
    }

    #[test]
    fn test_update_play_head_large_forward_seek() {
        let cache = RingBufferCache::new(10);
        cache.insert(0, test_frame_data(1));
        
        // Very large forward seek beyond 2*capacity - this is a true seek
        let seek_detected = cache.update_play_head(25);
        assert!(seek_detected);
        assert_eq!(cache.get_play_head(), 25);
        assert_eq!(cache.get_start_index(), 25);
        
        // Cache should be cleared
        assert!(cache.is_empty());
    }
    
    #[test]
    fn test_update_play_head_sequential_forward_slides_window() {
        let cache = RingBufferCache::new(10);
        
        // Fill cache with frames 0-9
        for i in 0..10 {
            cache.insert(i, test_frame_data(i as u8));
        }
        assert_eq!(cache.len(), 10);
        
        // Move play head to frame 10 (at boundary of window)
        // This should slide the window forward, NOT invalidate
        let seek_detected = cache.update_play_head(10);
        assert!(!seek_detected, "Sequential forward should not be detected as seek");
        assert_eq!(cache.get_play_head(), 10);
        
        // Window should have slid forward (start_index > 0)
        let new_start = cache.get_start_index();
        assert!(new_start > 0, "Window should have advanced");
        
        // Frame 10 should now be in range
        assert!(cache.is_in_range(10));
        
        // Some earlier frames should still be cached (window preserves some behind play head)
        // With capacity=10 and buffer_behind=2, new_start = 10 - 2 = 8
        // So frames 8, 9 should still be in the buffer (if they were ready)
    }

    #[test]
    fn test_clear() {
        let cache = RingBufferCache::new(10);
        
        for i in 0..5 {
            cache.insert(i, test_frame_data(i as u8));
        }
        assert_eq!(cache.len(), 5);
        
        cache.clear();
        assert!(cache.is_empty());
        assert_eq!(cache.len(), 0);
    }

    #[test]
    fn test_len_and_is_empty() {
        let cache = RingBufferCache::new(10);
        
        assert!(cache.is_empty());
        assert_eq!(cache.len(), 0);
        
        cache.insert(0, test_frame_data(1));
        assert!(!cache.is_empty());
        assert_eq!(cache.len(), 1);
        
        cache.insert(1, test_frame_data(2));
        assert_eq!(cache.len(), 2);
    }

    #[test]
    fn test_frame_to_slot_mapping() {
        let cache = RingBufferCache::new(10);
        
        // With start_index = 0, frames map directly
        assert!(cache.insert(0, test_frame_data(0)));
        assert!(cache.insert(5, test_frame_data(5)));
        assert!(cache.insert(9, test_frame_data(9)));
        
        assert!(cache.contains(&0));
        assert!(cache.contains(&5));
        assert!(cache.contains(&9));
        assert!(!cache.contains(&10)); // Out of range
    }

    #[test]
    fn test_sequential_access_pattern() {
        let cache = RingBufferCache::new(5);
        
        // Simulate sequential playback
        for i in 0..5 {
            cache.insert(i, test_frame_data(i as u8));
            cache.update_play_head(i);
        }
        
        // All frames should be available
        for i in 0..5 {
            assert!(cache.contains(&i));
            let data = cache.get(i).unwrap();
            assert_eq!(data.polystream[0], i as u8);
        }
    }

    #[test]
    fn test_advance_start() {
        let cache = RingBufferCache::new(5);
        
        // Fill cache
        for i in 0..5 {
            cache.insert(i, test_frame_data(i as u8));
        }
        
        // Advance start to make room for new frames
        cache.advance_start(2);
        
        assert_eq!(cache.get_start_index(), 2);
        
        // Old frames should be cleared
        assert!(!cache.is_in_range(0));
        assert!(!cache.is_in_range(1));
        
        // New range should work
        assert!(cache.is_in_range(2));
        assert!(cache.is_in_range(6)); // Now in range
    }

    #[test]
    fn test_clone_independence() {
        let cache1 = RingBufferCache::new(10);
        cache1.insert(0, test_frame_data(1));
        
        let cache2 = cache1.clone();
        
        // Both should have the data
        assert!(cache1.contains(&0));
        assert!(cache2.contains(&0));
        
        // Modifying one shouldn't affect the other (deep clone)
        cache2.insert(1, test_frame_data(2));
        assert!(!cache1.contains(&1));
        assert!(cache2.contains(&1));
    }

    #[test]
    fn test_thread_safety_concurrent_reads() {
        use std::sync::Arc;
        use std::thread;

        let cache = Arc::new(RingBufferCache::new(100));
        
        // Pre-populate cache
        for i in 0..50 {
            cache.insert(i, test_frame_data(i as u8));
        }
        
        let mut handles = vec![];
        
        // Spawn multiple reader threads
        for _ in 0..5 {
            let cache_clone = Arc::clone(&cache);
            let handle = thread::spawn(move || {
                for i in 0..50 {
                    let data = cache_clone.get(i);
                    assert!(data.is_some());
                    assert_eq!(data.unwrap().polystream[0], i as u8);
                }
            });
            handles.push(handle);
        }
        
        for handle in handles {
            handle.join().unwrap();
        }
    }

    #[test]
    fn test_thread_safety_concurrent_writes() {
        use std::sync::Arc;
        use std::thread;

        let cache = Arc::new(RingBufferCache::new(100));
        let mut handles = vec![];
        
        // Spawn multiple writer threads
        for t in 0..5 {
            let cache_clone = Arc::clone(&cache);
            let handle = thread::spawn(move || {
                for i in 0..20 {
                    let frame_idx = t * 20 + i;
                    cache_clone.insert(frame_idx, test_frame_data(frame_idx as u8));
                }
            });
            handles.push(handle);
        }
        
        for handle in handles {
            handle.join().unwrap();
        }
        
        // All frames should be present
        for i in 0..100 {
            assert!(cache.contains(&i));
        }
    }

    #[test]
    fn test_thread_safety_mixed_read_write() {
        use std::sync::Arc;
        use std::thread;

        let cache = Arc::new(RingBufferCache::new(50));
        
        // Pre-populate some frames
        for i in 0..25 {
            cache.insert(i, test_frame_data(i as u8));
        }
        
        let mut handles = vec![];
        
        // Reader thread
        let cache_reader = Arc::clone(&cache);
        handles.push(thread::spawn(move || {
            for _ in 0..100 {
                for i in 0..25 {
                    let _ = cache_reader.get(i);
                }
            }
        }));
        
        // Writer thread
        let cache_writer = Arc::clone(&cache);
        handles.push(thread::spawn(move || {
            for i in 25..50 {
                cache_writer.insert(i, test_frame_data(i as u8));
            }
        }));
        
        for handle in handles {
            handle.join().unwrap();
        }
        
        // Should have all frames
        assert_eq!(cache.len(), 50);
    }

    #[test]
    fn test_frame_slot_enum() {
        let empty = FrameSlot::Empty;
        let in_progress = FrameSlot::InProgress;
        let ready = FrameSlot::Ready(test_frame_data(1));
        
        assert!(empty.is_empty());
        assert!(!empty.is_in_progress());
        assert!(!empty.is_ready());
        assert!(empty.get_data().is_none());
        
        assert!(!in_progress.is_empty());
        assert!(in_progress.is_in_progress());
        assert!(!in_progress.is_ready());
        assert!(in_progress.get_data().is_none());
        
        assert!(!ready.is_empty());
        assert!(!ready.is_in_progress());
        assert!(ready.is_ready());
        assert!(ready.get_data().is_some());
    }

    #[test]
    fn test_insert_out_of_range_returns_false() {
        let cache = RingBufferCache::new(10);
        
        // Try to insert beyond capacity (should fail)
        assert!(!cache.insert(15, test_frame_data(1)));
        
        // Cache should still be empty
        assert!(cache.is_empty());
    }

    #[test]
    fn test_type_alias_backward_compatibility() {
        // Test that FrameCache type alias works
        let cache: FrameCache = FrameCache::new(10);
        cache.insert(0, test_frame_data(1));
        assert!(cache.contains(&0));
    }

    #[test]
    fn test_read_only_get_performance() {
        // This test verifies get() uses read lock (multiple concurrent gets should work)
        use std::sync::Arc;
        use std::thread;
        use std::time::Instant;

        let cache = Arc::new(RingBufferCache::new(100));
        
        // Pre-populate
        for i in 0..100 {
            cache.insert(i, test_frame_data(i as u8));
        }
        
        let start = Instant::now();
        let mut handles = vec![];
        
        // Many concurrent readers should not block each other
        for _ in 0..10 {
            let cache_clone = Arc::clone(&cache);
            handles.push(thread::spawn(move || {
                for _ in 0..1000 {
                    for i in 0..100 {
                        let _ = cache_clone.get(i);
                    }
                }
            }));
        }
        
        for handle in handles {
            handle.join().unwrap();
        }
        
        let elapsed = start.elapsed();
        // Should complete quickly since reads don't block each other
        // This is more of a sanity check than a strict performance test
        assert!(elapsed.as_secs() < 30, "Concurrent reads took too long: {:?}", elapsed);
    }
}
