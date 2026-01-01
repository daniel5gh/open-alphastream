# Task 17 — Builder Config

## Objective
- Builder to configure runtime, pools, transport caps/timeouts.

## Options
- **Runtime**: Thread pool size, async runtime type.
- **Pools**: Connection pool size, worker pool size.
- **Transport Caps/Timeouts**: Max connections, read/write timeouts, retry limits.

## Defaults and Overrides
- Runtime: Default thread pool size 8, override range 1-64.
- Pools: Default connection pool size 10, override range 1-100.
- Transport: Default timeout 30 seconds, override range 1-300 seconds.

## Example Configuration

```rust
pub struct BuilderConfig {
    pub runtime_threads: usize, // Default: 8, Range: 1-64
    pub connection_pool_size: usize, // Default: 10, Range: 1-100
    pub timeout_seconds: u64, // Default: 30, Range: 1-300
}
```

## Scope
- Defaults and overrides with sane ranges.

## Deliverables
- Builder API docs

## Dependencies
- [docs/tasks/06-async-runtime-concurrency.md](docs/tasks/06-async-runtime-concurrency.md)
- [docs/tasks/03-transport-http.md](docs/tasks/03-transport-http.md)
- [docs/tasks/04-transport-local.md](docs/tasks/04-transport-local.md)
- [docs/tasks/05-transport-in-memory.md](docs/tasks/05-transport-in-memory.md)

## Implementation Checklist
- Builder API docs
- Validation of inputs
- Sane ranges
- Metrics reflect overrides

## Acceptance Criteria
- Config applied in runtime and transport layers
- Overrides visible in metrics outputs

## References
- [docs/RUST_IMPLEMENTATION.md](docs/RUST_IMPLEMENTATION.md)
