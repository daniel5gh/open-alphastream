// Async runtime module
// This module provides an abstraction over Tokio's async runtime for managing concurrent tasks.

use tokio::runtime::{Builder, Runtime as TokioRuntime};

/// Builder for creating a custom Runtime with configurable worker threads and pools.
pub struct RuntimeBuilder {
    // Number of worker threads for the runtime. Defaults to the number of CPU cores.
    worker_threads: Option<usize>,
}

impl RuntimeBuilder {
    /// Create a new RuntimeBuilder with default settings.
    pub fn new() -> Self {
        Self {
            worker_threads: None,
        }
    }

    /// Set the number of worker threads for the runtime.
    pub fn worker_threads(mut self, threads: usize) -> Self {
        self.worker_threads = Some(threads);
        self
    }

    /// Build the Runtime with the configured settings.
    pub fn build(self) -> Result<Runtime, std::io::Error> {
        let mut builder = Builder::new_multi_thread();

        if let Some(threads) = self.worker_threads {
            builder.worker_threads(threads);
        }

        // Enable all features for full async support
        builder.enable_all();

        let runtime = builder.build()?;
        Ok(Runtime { runtime })
    }
}

/// The main Runtime struct that wraps Tokio's runtime.
/// This provides a high-level interface for running async tasks.
pub struct Runtime {
    // The underlying Tokio runtime instance.
    runtime: TokioRuntime,
}

impl Runtime {
    /// Create a new Runtime with default settings using the builder.
    pub fn new() -> Result<Self, std::io::Error> {
        RuntimeBuilder::new().build()
    }

    /// Create a Runtime with a custom number of worker threads.
    pub fn with_worker_threads(threads: usize) -> Result<Self, std::io::Error> {
        RuntimeBuilder::new().worker_threads(threads).build()
    }

    /// Run a future to completion on this runtime.
    /// This blocks the current thread until the future completes.
    pub fn block_on<F, T>(&self, future: F) -> T
    where
        F: std::future::Future<Output = T>,
    {
        self.runtime.block_on(future)
    }

    /// Spawn a task on this runtime and return a JoinHandle to await its result.
    pub fn spawn<F>(&self, future: F) -> tokio::task::JoinHandle<F::Output>
    where
        F: std::future::Future + Send + 'static,
        F::Output: Send + 'static,
    {
        self.runtime.spawn(future)
    }

    /// Spawn a blocking task on this runtime and return a JoinHandle to await its result.
    pub fn spawn_blocking<F, T>(&self, f: F) -> tokio::task::JoinHandle<T>
    where
        F: FnOnce() -> T + Send + 'static,
        T: Send + 'static,
    {
        self.runtime.spawn_blocking(f)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_runtime_creation() {
        let _runtime = Runtime::new().expect("Failed to create runtime");
        // Runtime created successfully
    }

    #[test]
    fn test_runtime_with_threads() {
        let _runtime = Runtime::with_worker_threads(2).expect("Failed to create runtime with threads");
        // Runtime with custom threads created successfully
    }

    #[test]
    fn test_block_on() {
        let runtime = Runtime::new().expect("Failed to create runtime");
        let result = runtime.block_on(async { 42 });
        assert_eq!(result, 42);
    }

    #[test]
    fn test_spawn() {
        let runtime = Runtime::new().expect("Failed to create runtime");
        let handle = runtime.spawn(async { 42 });
        let result = runtime.block_on(handle).expect("Task failed");
        assert_eq!(result, 42);
    }
}