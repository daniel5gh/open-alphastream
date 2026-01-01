# Task 14 — Error Model

## Objective
Define a per-instance last error model for alphastream-rs.

## Error Codes
- None: No error.
- NotReady: Resource not ready.
- Timeout: Operation timed out.
- Decode: Decoding failure.
- Transport: Transport layer error.

## Set/Reset Semantics
Errors are set on failures during operations.

- `get_frame` sets Decode on decoding errors and Transport on transport failures.
- Source failures set Transport or Timeout as appropriate.
- Errors persist until explicitly cleared or overwritten by a subsequent error.

## Concurrency
Error state updates are thread-safe, using atomic operations or mutexes to ensure race-free access under concurrent load.

## Deliverables
- Error struct
- Error mapping

## Dependencies
- [docs/tasks/12-public-api-facade.md](docs/tasks/12-public-api-facade.md)

## Implementation Checklist
- Implement `handle.last_error()` to return the current error code.
  ```rust
  fn last_error(&self) -> ErrorCode;
  ```
- Implement `handle.clear_error()` to reset the error to None.
  ```rust
  fn clear_error(&mut self);
  ```
- Ensure thread-safe storage (atomic or mutex).
- Map subsystem errors to unified codes.

## Acceptance Criteria
- Race-free under load (stress tests).
- Unit tests pass for set/reset and concurrent access.

## References
- [docs/RUST_IMPLEMENTATION.md](docs/RUST_IMPLEMENTATION.md)
