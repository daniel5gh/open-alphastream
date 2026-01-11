# Task 07 — Async Runtime & Concurrency

## Objective
- Specify the async runtime and concurrency model for alphastream-rs, including Tokio setup, thread pools, builder configurability, and performance notes.

## Runtime Ownership
- Tokio multi-thread runtime owned by the library.
- Runtime started on handle construction.

## Defaults
- $worker_threads = num_cpus$
- Pools: io = 4 async tasks, decode = $num_cpus$ blocking threads, raster = 2 async tasks.

## Blocking Decode
- Dedicated threads for decode operations to avoid starving async tasks.

## Builder Configurability
- Overrides for thread counts, pools.
- Optional core pinning (off by default).

## Performance Considerations
- Multi-thread runtime choice for parallelism.
- Separate blocking threads to prevent blocking operations from affecting async performance.
- Default pool sizing based on CPU cores for optimal resource utilization.

## Implementation Checklist
- Runtime bootstrap:
  ```rust
  // Bootstrap Tokio runtime
  let rt = tokio::runtime::Builder::new_multi_thread()
      .worker_threads(num_cpus::get())
      .build()
      .unwrap();
  ```
- Builder options:
  ```rust
  // Builder with configurability
  struct Builder {
      worker_threads: Option<usize>,
      io_pool: Option<usize>,
      decode_pool: Option<usize>,
      raster_pool: Option<usize>,
      core_pinning: bool,
  }
  ```

## Acceptance Criteria
- Stress test without starvation.
- Configurable sizing works.

## References
- [docs/RUST_IMPLEMENTATION.md](docs/RUST_IMPLEMENTATION.md)
