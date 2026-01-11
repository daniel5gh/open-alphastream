# C ABI Refactor to Wrap AlphaStreamProcessor

## Objective
Refactor the C ABI (CV_* functions) to wrap and expose `AlphaStreamProcessor` directly, ensuring a single, unified abstraction for both Rust and FFI consumers.

## Scope
- Update all C ABI functions in [rust/alphastream-rs/src/lib.rs](rust/alphastream-rs/src/lib.rs) to use `AlphaStreamProcessor` as the underlying implementation.
- Remove any remaining logic or state in the C ABI that duplicates processor responsibilities.
- Ensure memory management, error handling, and resource cleanup are handled via `AlphaStreamProcessor`.

## Deliverables
- Refactored C ABI in [rust/alphastream-rs/src/lib.rs](rust/alphastream-rs/src/lib.rs) using `AlphaStreamProcessor`.
- Updated tests for C ABI usage.
- Documentation updates reflecting the new FFI boundary.

## Checklist
- All C ABI functions delegate to `AlphaStreamProcessor`.
- No duplicated state or logic between C ABI and processor.
- Memory and error handling are unified.
- Tests and documentation updated.

## Acceptance Criteria
- C ABI is a thin wrapper over `AlphaStreamProcessor`.
- No duplicated logic or state.
- All tests pass for C ABI usage.
- Documentation is current and accurate.

## References
- [rust/alphastream-rs/src/lib.rs](rust/alphastream-rs/src/lib.rs)
- [rust/alphastream-rs/src/api.rs](rust/alphastream-rs/src/api.rs)
- [docs/adr/0009-handle-vs-processor.md](../adr/0009-handle-vs-processor.md)
- [docs/prd/prd-alphastream-rs.md](../prd/prd-alphastream-rs.md)
