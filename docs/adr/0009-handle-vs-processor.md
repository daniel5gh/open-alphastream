# ADR 0009: AlphaStreamHandle vs AlphaStreamProcessor Consolidation

## Status
Proposed

## Context
The codebase currently contains both `AlphaStreamHandle` (used in the C ABI and FFI boundary) and `AlphaStreamProcessor` (used in the Rust API and internal logic). Both types encapsulate state and operations for accessing, decoding, and caching AlphaStream frames, but their responsibilities and boundaries are not clearly separated or consolidated.

## Decision
- The `AlphaStreamProcessor` is the main abstraction that should be used for all future development and integration, both in Rust and as the backing for the C ABI.
- `AlphaStreamHandle` was a temporary stand-in used during early C ABI prototyping and should be deprecated and removed as soon as possible.
- All responsibilities for frame access, caching, error state, and resource cleanup should be consolidated into `AlphaStreamProcessor`.
- The FFI (C ABI) should provide thin wrappers around `AlphaStreamProcessor` for safe cross-language usage.
- Documentation and new features should target only `AlphaStreamProcessor` as the core API.

## Consequences
- Reduces code duplication and confusion for maintainers and users
- Simplifies documentation and API surface
- Ensures consistent behavior across Rust and FFI boundaries
- May require migration and refactoring of existing code

## References
- [rust/alphastream-rs/src/lib.rs](../rust/alphastream-rs/src/lib.rs)
- [rust/alphastream-rs/src/api.rs](../rust/alphastream-rs/src/api.rs)
- [docs/prd/prd-alphastream-rs.md](../prd/prd-alphastream-rs.md)
