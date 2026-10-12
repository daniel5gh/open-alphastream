// Scheduler module
// This module helps manage when to process video frames.
// It acts like a smart task manager: decides which frames to work on first (priority queue),
// prevents too many tasks running at once (backpressure), and loads future frames early (prefetching).

use std::collections::{VecDeque, HashSet};
use std::sync::Arc;
use crate::cache::FrameCache;
use tokio::sync::mpsc;

/// Represents a scheduled task with a frame index and priority.
#[derive(Debug, Clone)]
pub struct Task {
    // The frame index for this task.
    pub frame_index: usize,
    // Priority level (higher numbers = higher priority).
    pub priority: u8,
}

impl Task {
    /// Create a new task with the given frame index and default priority.
    pub fn new(frame_index: usize) -> Self {
        Self {
            frame_index,
            priority: 0,
        }
    }

    /// Create a new task with custom priority.
    pub fn with_priority(frame_index: usize, priority: u8) -> Self {
        Self {
            frame_index,
            priority,
        }
    }
}

/// The main Scheduler struct for managing frame processing tasks.
/// This is the "brain" that coordinates frame processing:
// - Keeps a prioritized to-do list of frames to process
// - Ensures not too many workers are busy at once (prevents system overload)
// - Automatically adds future frames to the list (prefetching for smooth playback)
// - Tracks time so frames are processed in the right order
pub struct Scheduler {
    // Timebase constant: frames per second (60 FPS). Used to convert frame numbers to time.
    timebase_fps: f64,
    // Queue for pending tasks, prioritized by priority then frame index.
    // Higher priority tasks get processed first.
    task_queue: VecDeque<Task>,
    // HashSet for O(1) duplicate detection - tracks frame indices in queue
    queued_frames: HashSet<usize>,
    // Channel sender for communicating with the processing loop.
    // Like a message queue - workers can send tasks to the scheduler.
    task_sender: mpsc::UnboundedSender<Task>,
    // Channel receiver for the processing loop.
    // The "inbox" where the scheduler receives new tasks.
    task_receiver: mpsc::UnboundedReceiver<Task>,
    // Maximum number of concurrent tasks to prevent overload (backpressure).
    max_concurrent: usize,
    // Current number of active tasks. Tracks how many workers are busy.
    active_tasks: usize,
    // Number of frames to prefetch ahead of current playback.
    prefetch_count: usize,
    // Reference to the cache for adaptive prefetching and backpressure
    cache: Option<Arc<FrameCache>>,
}

impl Scheduler {
    /// Create a new Scheduler with default settings.
    pub fn new() -> Self {
        let (tx, rx) = mpsc::unbounded_channel();
        Self {
            timebase_fps: 60.0,
            task_queue: VecDeque::new(),
            queued_frames: HashSet::new(),
            task_sender: tx,
            task_receiver: rx,
            max_concurrent: 16, // Default max concurrent tasks
            active_tasks: 0,
            prefetch_count: 64, // Prefetch frames ahead
            cache: None,
        }
    }

    pub fn get_number_of_queued_tasks(&self) -> usize {
        self.task_queue.len()
    }

    pub fn get_number_of_active_tasks(&self) -> usize {
        self.active_tasks
    }

    pub fn get_number_of_max_concurrent_tasks(&self) -> usize {
        self.max_concurrent
    }

    /// Set the maximum number of concurrent tasks (for builder integration)
    pub fn set_max_concurrent(&mut self, max: usize) {
        self.max_concurrent = max;
    }

    /// Set the prefetch window size (for builder integration)
    pub fn set_prefetch_count(&mut self, count: usize) {
        self.prefetch_count = count;
    }

    /// Set the cache reference for coordinated rate control and prefetching
    pub fn set_cache(&mut self, cache: Arc<FrameCache>) {
        self.cache = Some(cache);
    }

    /// Calculate the time in seconds for a given frame index using the timebase.
    /// Formula: t_n = n / 60 (for 60 FPS).
    pub fn time_for_frame(&self, frame_index: usize) -> f64 {
        frame_index as f64 / self.timebase_fps
    }

    /// Schedule a new task for processing.
    /// Tasks are added to the queue and prioritized.
    /// Uses HashSet for O(1) duplicate detection.
    pub fn schedule_task(&mut self, task: Task) {
        let frame_index = task.frame_index;
        
        // O(1) duplicate check using HashSet
        if self.queued_frames.contains(&frame_index) {
            // Task already queued - check if we need to update priority
            if let Some(existing) = self.task_queue.iter_mut().find(|t| t.frame_index == frame_index) {
                if task.priority > existing.priority {
                    existing.priority = task.priority;
                    // Move to front of queue for high priority
                    self.queued_frames.remove(&frame_index);
                    self.task_queue.retain(|t| t.frame_index != frame_index);
                    self.task_queue.push_front(task);
                    self.queued_frames.insert(frame_index);
                }
            }
            return;
        }
        
        // Insert task in priority order (higher priority first, then lower frame index)
        let pos = self.task_queue.iter().position(|t| {
            t.priority < task.priority || (t.priority == task.priority && t.frame_index > task.frame_index)
        }).unwrap_or(self.task_queue.len());
        self.task_queue.insert(pos, task);
        self.queued_frames.insert(frame_index);
    }

    /// Get the next task to process, respecting backpressure and ring buffer capacity.
    /// The scheduler MUST NOT exceed decoding frames beyond the cache's capacity.
    /// Uses O(1) occupied_count() for efficient capacity checking.
    pub fn next_task(&mut self) -> Option<Task> {
        if self.active_tasks >= self.max_concurrent {
            return None; // Backpressure: don't start more tasks
        }
        
        if let Some(ref cache) = self.cache {
            // O(1) capacity check using atomic counters
            if cache.occupied_count() >= cache.capacity() {
                return None; // All slots occupied, pause processing
            }
        }
        
        // Find the first task that's in the valid range
        while let Some(task) = self.task_queue.pop_front() {
            // Remove from queued_frames HashSet
            self.queued_frames.remove(&task.frame_index);
            
            if let Some(ref cache) = self.cache {
                // Only process if frame is in the valid buffer window
                if cache.is_in_range(task.frame_index) {
                    // Mark slot as in-progress before returning task
                    cache.mark_in_progress(task.frame_index);
                    self.active_tasks += 1;
                    return Some(task);
                }
                // Frame is out of range (stale task from before a seek), skip it
                continue;
            } else {
                // No cache set, just process the task
                self.active_tasks += 1;
                return Some(task);
            }
        }
        None
    }

    /// Mark a task as completed, freeing up a slot for backpressure.
    pub fn complete_task(&mut self) {
        if self.active_tasks > 0 {
            self.active_tasks -= 1;
        }
    }

    /// Generate prefetch tasks for frames ahead of the current frame.
    /// Only prefetches within the valid buffer window [start_index, start_index + capacity).
    /// Uses O(1) HashSet lookup for duplicate detection.
    pub fn prefetch(&mut self, current_frame: usize) {
        let mut frames_to_prefetch = vec![];
        
        if let Some(ref cache) = self.cache {
            let cap = cache.capacity();
            let start = cache.get_start_index();
            let end = start + cap;
            let prefetch_limit = self.prefetch_count;

            for i in 1..=prefetch_limit {
                let frame_index = current_frame + i;
                
                // Only prefetch within the valid buffer window
                if frame_index >= end {
                    break; // Beyond buffer range
                }
                
                // O(1) check if already queued
                if self.queued_frames.contains(&frame_index) {
                    continue;
                }
                
                // Check if frame is already in cache or being processed
                if let Some(slot) = cache.get_slot_state(frame_index) {
                    if slot.is_empty() {
                        frames_to_prefetch.push(Task::new(frame_index));
                    }
                    // Skip InProgress and Ready slots
                }
            }
        } else {
            // No cache, use simple prefetch (fallback)
            for i in 1..=self.prefetch_count {
                let frame_index = current_frame + i;
                // O(1) duplicate check
                if !self.queued_frames.contains(&frame_index) {
                    frames_to_prefetch.push(Task::new(frame_index));
                }
            }
        }
        
        for task in frames_to_prefetch {
            self.schedule_task(task);
        }
    }

    /// Get the sender for external task submission.
    pub fn sender(&self) -> mpsc::UnboundedSender<Task> {
        self.task_sender.clone()
    }

    /// Get the receiver for processing tasks.
    pub fn receiver(&mut self) -> &mut mpsc::UnboundedReceiver<Task> {
        &mut self.task_receiver
    }
}

#[cfg(test)]
mod tests {
    use crate::cache::FrameCache;
    use std::sync::Arc;

    #[test]
    fn test_scheduler_cache_backpressure_prefetch() {
        let cache = Arc::new(FrameCache::new(4));
        let mut scheduler = Scheduler::new();
        scheduler.set_cache(Arc::clone(&cache));
        scheduler.set_prefetch_count(4);

        // Fill cache with Ready frames
        for i in 0..4 {
            cache.insert(i, crate::FrameData {
                polystream: vec![i as u8],
                bitmap: None,
                triangle_strip: None,
            });
        }
        
        // Prefetch should not schedule new tasks if cache is full with Ready frames
        scheduler.prefetch(0);
        // All slots are Ready, so next_task should return None (all occupied)
        assert!(scheduler.next_task().is_none());

        // Simulate play head advancement by updating play_head and advancing start
        // This makes room for new frames
        cache.advance_start(2);
        
        // Now prefetch can schedule frames in the new range [2, 6)
        scheduler.prefetch(2);
        let task = scheduler.next_task();
        // Should get a task for frame 3 or later (frame 2 is current)
        assert!(task.is_some());
        let frame = task.unwrap().frame_index;
        assert!(frame >= 3 && frame < 6, "Expected frame in [3,6), got {}", frame);
    }

    use super::*;

    #[test]
    fn test_time_for_frame() {
        let scheduler = Scheduler::new();
        assert_eq!(scheduler.time_for_frame(0), 0.0);
        assert_eq!(scheduler.time_for_frame(60), 1.0);
        assert_eq!(scheduler.time_for_frame(30), 0.5);
    }

    #[test]
    fn test_schedule_and_next_task() {
        let mut scheduler = Scheduler::new();
        let task1 = Task::new(1);
        let task2 = Task::with_priority(2, 5);

        scheduler.schedule_task(task1);
        scheduler.schedule_task(task2);

        // Higher priority task should come first
        let next = scheduler.next_task().unwrap();
        assert_eq!(next.frame_index, 2);
        assert_eq!(next.priority, 5);

        scheduler.complete_task();

        let next = scheduler.next_task().unwrap();
        assert_eq!(next.frame_index, 1);
        assert_eq!(next.priority, 0);
    }

    #[test]
    fn test_backpressure() {
        let mut scheduler = Scheduler::new();
        scheduler.max_concurrent = 1;

        let task = Task::new(1);
        scheduler.schedule_task(task);

        let next = scheduler.next_task().unwrap();
        assert_eq!(next.frame_index, 1);

        // Should not get another task due to backpressure
        let none = scheduler.next_task();
        assert!(none.is_none());
    }

    #[test]
    fn test_prefetch() {
        let mut scheduler = Scheduler::new();
        scheduler.prefetch(5);

        // Should have 10 prefetch tasks
        for i in 1..=10 {
            let task = scheduler.next_task().unwrap();
            assert_eq!(task.frame_index, 5 + i);
            // Prefetch tasks are scheduled with default priority 0 in implementation
            assert_eq!(task.priority, 0);
            scheduler.complete_task();
        }
    }
}