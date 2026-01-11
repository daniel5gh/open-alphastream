# ADR 0005: Tokio Runtime Ownership and Builder Configuration

## Status
Accepted

## Context
Alphastream-rs requires a high-performance, configurable async runtime to manage I/O, decoding, and rasterization tasks. The runtime must be library-owned, support multi-threading, and allow users to tune resource usage via a builder pattern.

## Decision
- The Tokio multi-thread runtime is owned and managed by the library, initialized on first handle construction and shut down on last handle drop.
- Runtime and worker pool sizes (I/O, decode, raster) are configurable via a builder API, with documented defaults and override ranges.
- Optional CPU core pinning is supported (off by default).
- Transport concurrency, chunk sizes, and timeouts are also builder-configurable.
- Builder validates all inputs and exposes metrics reflecting overrides.

## Consequences
- Predictable and tunable resource usage for diverse deployment scenarios
- No dependency on external runtime management
- Flexibility for performance tuning and benchmarking
- Simplified integration for consumers

## References
- [docs/tasks/07-async-runtime-concurrency.md](../tasks/07-async-runtime-concurrency.md)
- [docs/tasks/17-builder-config.md](../tasks/17-builder-config.md)
- [docs/RUST_IMPLEMENTATION.md](../RUST_IMPLEMENTATION.md)
- [docs/prd/prd-alphastream-rs.md](../prd/prd-alphastream-rs.md)
