// Scheduler module
// This module helps manage when to process video frames.
// It acts like a smart task manager: decides which frames to work on first (priority queue),
// prevents too many tasks running at once (backpressure), and loads future frames early (prefetching).

use std::collections::VecDeque;
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
}

impl Scheduler {
    /// Create a new Scheduler with default settings.
    pub fn new() -> Self {
        let (tx, rx) = mpsc::unbounded_channel();
        Self {
            timebase_fps: 60.0,
            task_queue: VecDeque::new(),
            task_sender: tx,
            task_receiver: rx,
            max_concurrent: 4, // Default max concurrent tasks
            active_tasks: 0,
            prefetch_count: 10, // Prefetch 10 frames ahead
        }
    }

    /// Calculate the time in seconds for a given frame index using the timebase.
    /// Formula: t_n = n / 60 (for 60 FPS).
    pub fn time_for_frame(&self, frame_index: usize) -> f64 {
        frame_index as f64 / self.timebase_fps
    }

    /// Schedule a new task for processing.
    /// Tasks are added to the queue and prioritized.
    pub fn schedule_task(&mut self, task: Task) {
        // Check if a task with the same frame_index exists
        if let Some(existing) = self.task_queue.iter_mut().find(|t| t.frame_index == task.frame_index) {
            // Update priority if the new task has higher priority
            if task.priority > existing.priority {
                existing.priority = task.priority;
            }
        } else {
            // Insert task in priority order (higher priority first, then lower frame index)
            let pos = self.task_queue.iter().position(|t| {
                t.priority < task.priority || (t.priority == task.priority && t.frame_index > task.frame_index)
            }).unwrap_or(self.task_queue.len());
            self.task_queue.insert(pos, task);
        }
    }

    /// Get the next task to process, respecting backpressure.
    pub fn next_task(&mut self) -> Option<Task> {
        if self.active_tasks >= self.max_concurrent {
            return None; // Backpressure: don't start more tasks
        }
        self.task_queue.pop_front().map(|task| {
            self.active_tasks += 1;
            // reconsider prefetching, and who is responsible for it and will moderate it?
            // Prefetch next frames if cache has space and the queue is smaller than cache size
            // if self.task_queue.len() < self.cache.capacity() {
            //     println!("Prefetching frame {}, cache size {}, queue {}", task.frame_index, self.cache.len(), self.task_queue.len());
            //     self.prefetch(task.frame_index);
            // }
            task
        })
    }

    /// Mark a task as completed, freeing up a slot for backpressure.
    pub fn complete_task(&mut self) {
        if self.active_tasks > 0 {
            self.active_tasks -= 1;
        }
    }

    /// Generate prefetch tasks for frames ahead of the current frame.
    pub fn prefetch(&mut self, current_frame: usize) {
        for i in 1..=self.prefetch_count {
            let frame_index = current_frame + i;
            if !self.task_queue.iter().any(|t| t.frame_index == frame_index) {
                let task = Task::new(frame_index);
                self.schedule_task(task);
            }
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