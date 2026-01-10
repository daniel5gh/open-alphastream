# Handle/Processor Consolidation and C ABI Refactor

## Objective
Consolidate `AlphaStreamHandle` into `AlphaStreamProcessor` and refactor the C ABI to wrap `AlphaStreamProcessor` directly, eliminating duplication and ensuring a single, consistent abstraction across Rust and FFI boundaries.

## Scope
- Remove `AlphaStreamHandle` as a separate type.
- Refactor all C ABI (CV_*) functions in [rust/alphastream-rs/src/lib.rs](rust/alphastream-rs/src/lib.rs) to operate on `AlphaStreamProcessor`.
- Ensure all state, error handling, and resource management are unified under `AlphaStreamProcessor`.
- Update documentation and references to reflect the new architecture.

## Deliverables
- Updated C ABI in [rust/alphastream-rs/src/lib.rs](rust/alphastream-rs/src/lib.rs) using `AlphaStreamProcessor`.
- Migration of all state and logic from `AlphaStreamHandle` to `AlphaStreamProcessor`.
- Updated tests and documentation.

## Checklist
- Remove `AlphaStreamHandle` from codebase.
- Refactor C ABI to use `AlphaStreamProcessor` for all operations.
- Ensure error handling and resource cleanup are preserved.
- Update documentation and ADRs to reflect consolidation.

## Acceptance Criteria
- All C ABI functions operate on `AlphaStreamProcessor`.
- No references to `AlphaStreamHandle` remain.
- Tests pass for both Rust and C ABI usage.
- Documentation and ADRs are updated for the new architecture.

## References
- [rust/alphastream-rs/src/lib.rs](rust/alphastream-rs/src/lib.rs)
- [rust/alphastream-rs/src/api.rs](rust/alphastream-rs/src/api.rs)
- [docs/adr/0009-handle-vs-processor.md](../adr/0009-handle-vs-processor.md)
- [docs/prd/prd-alphastream-rs.md](../prd/prd-alphastream-rs.md)
