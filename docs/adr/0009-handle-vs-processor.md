# ADR 0009: AlphaStreamHandle vs AlphaStreamProcessor Consolidation

## Status
Accepted

## Context
The codebase previously contained both `AlphaStreamHandle` (used in the C ABI and FFI boundary) and `AlphaStreamProcessor` (used in the Rust API and internal logic). Both types encapsulated state and operations for accessing, decoding, and caching AlphaStream frames, but their responsibilities and boundaries were not clearly separated or consolidated.

## Decision
- The `AlphaStreamProcessor` is now the sole abstraction for all AlphaStream operations, both in Rust and as the backing for the C ABI.
- `AlphaStreamHandle` has been fully deprecated and removed.
- All responsibilities for frame access, caching, error state, and resource cleanup are consolidated into `AlphaStreamProcessor`.
- The FFI (C ABI) provides thin wrappers around `AlphaStreamProcessor` for safe cross-language usage, using an opaque `AlphaStreamCHandle` struct that owns an `AlphaStreamProcessor` instance.
- Documentation and new features target only `AlphaStreamProcessor` as the core API.

## Consequences
- Reduces code duplication and confusion for maintainers and users
- Simplifies documentation and API surface
- Ensures consistent behavior across Rust and FFI boundaries
- Migration and refactoring of existing code completed

## Architecture Overview
The new architecture centers on `AlphaStreamProcessor`, which integrates:
- **FrameCache**: Thread-safe, LRU-evicting cache for decoded frames
- **Scheduler**: Manages frame processing tasks, rate control, and prefetching
- **Prefetcher**: Detects sequential access and triggers background prefetch
- **API/ABI**: Rust API and C ABI both wrap the same processor logic

See the [architecture diagram](../architecture/alphastream-architecture.svg) for component interactions.

## References
- [rust/alphastream-rs/src/lib.rs](../../rust/alphastream-rs/src/lib.rs)
- [rust/alphastream-rs/src/api.rs](../../rust/alphastream-rs/src/api.rs)
- [docs/prd/prd-alphastream-rs.md](../prd/prd-alphastream-rs.md)
