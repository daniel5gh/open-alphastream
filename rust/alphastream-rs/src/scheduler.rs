// Scheduler module
// This module implements a hybrid index-timebase scheduler for managing frame processing tasks.
// It supports prioritization, backpressure to prevent overload, and prefetching for smooth playback.

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
pub struct Scheduler {
    // Timebase constant: frames per second (60 FPS).
    timebase_fps: f64,
    // Queue for pending tasks, prioritized by priority then frame index.
    task_queue: VecDeque<Task>,
    // Channel sender for communicating with the processing loop.
    task_sender: mpsc::UnboundedSender<Task>,
    // Channel receiver for the processing loop.
    task_receiver: mpsc::UnboundedReceiver<Task>,
    // Maximum number of concurrent tasks to prevent overload (backpressure).
    max_concurrent: usize,
    // Current number of active tasks.
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
        // Insert task in priority order (higher priority first, then lower frame index)
        let pos = self.task_queue.iter().position(|t| {
            t.priority < task.priority || (t.priority == task.priority && t.frame_index > task.frame_index)
        }).unwrap_or(self.task_queue.len());
        self.task_queue.insert(pos, task);
    }

    /// Get the next task to process, respecting backpressure.
    pub fn next_task(&mut self) -> Option<Task> {
        if self.active_tasks >= self.max_concurrent {
            return None; // Backpressure: don't start more tasks
        }
        self.task_queue.pop_front().map(|task| {
            self.active_tasks += 1;
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
            let task = Task::with_priority(frame_index, 1); // Low priority for prefetch
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
            assert_eq!(task.priority, 1);
            scheduler.complete_task();
        }
    }
}