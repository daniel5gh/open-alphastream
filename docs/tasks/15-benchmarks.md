# Task 15 — Benchmarks

## Objective
- Define and implement a performance suite for alphastream-rs, aligned with [AGENTS.md](AGENTS.md).

## Metrics
- HTTP range throughput: Measure bytes per second for HTTP range requests.
- Decode latency: Time to decode a frame or segment.
- Raster cost: Computational cost for rasterization operations.
- Cache hit rate: Percentage of cache hits for frame data.

## Harness
- Use fixed seeds for reproducible benchmark runs.
- Ensure CI-friendly execution with bounded time and resource usage.

Example benchmark run:
```
cargo bench --bench alphastream_benchmarks
```

## Baselines
- Document performance targets for each metric.
- Track regressions across commits.

## Implementation Checklist
- Develop benchmark code using Rust benchmarking tools (e.g., Criterion).
- Integrate benchmarks into CI pipeline for automated runs.

## Acceptance Criteria
- All performance targets are met or exceeded.
- Regressions are automatically flagged in CI.

## References
- [docs/RUST_IMPLEMENTATION.md](docs/RUST_IMPLEMENTATION.md)
- [AGENTS.md](AGENTS.md)
