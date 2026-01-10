# ADR 0002: Unified Transport Abstraction (HTTP, Local, In-Memory)

## Status
Accepted

## Context
Alphastream must support reading data from diverse sources: HTTP, local files, and in-memory buffers. Each transport has different performance, error, and concurrency characteristics. A unified abstraction is required to enable seamless integration and consistent error handling.

## Decision
Implement a unified async `Transport` trait for all sources, exposing:
- Async open, read_range, and len methods
- Range-read capability with chunking and alignment
- Backpressure and cancellation hooks
- Consistent error taxonomy (Transport, Timeout, NotFound, etc.)
- Builder-configurable concurrency and timeout settings

## Consequences
- Enables pluggable sources for all supported formats
- Simplifies scheduler and cache integration
- Centralizes error handling and performance tuning
- Allows for mock transports in testing

## References
- [docs/tasks/03-transport-abstraction.md](../tasks/03-transport-abstraction.md)
- [docs/tasks/04-transport-http.md](../tasks/04-transport-http.md)
- [docs/tasks/05-transport-local.md](../tasks/05-transport-local.md)
- [docs/tasks/06-transport-in-memory.md](../tasks/06-transport-in-memory.md)
- [docs/RUST_IMPLEMENTATION.md](../RUST_IMPLEMENTATION.md)
- [docs/prd/prd-alphastream-rs.md](../prd/prd-alphastream-rs.md)
