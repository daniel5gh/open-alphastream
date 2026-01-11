# ADR 0004: Per-Instance, Thread-Safe Error Model

## Status
Accepted

## Context
Robust error handling is essential for diagnosing issues and ensuring predictable behavior in concurrent environments. Alphastream-rs must provide clear, non-panicking error reporting, with thread-safe access and explicit error codes.

## Decision
- Each API handle maintains its own last error state (per-instance), not global or thread-local.
- Error codes: None, NotReady, Timeout, Decode, Transport.
- Error state is updated using atomic or mutex-protected storage to ensure race-free access under concurrent load.
- Errors persist until explicitly cleared or overwritten by a subsequent error.
- All error paths are non-panicking and mapped to user-facing error codes/messages.

## Consequences
- Predictable error reporting for each consumer/handle
- Safe concurrent access to error state
- Simplified debugging and diagnostics
- No global error state or panics in error paths

## References
- [docs/tasks/14-error-model.md](../tasks/14-error-model.md)
- [docs/RUST_IMPLEMENTATION.md](../RUST_IMPLEMENTATION.md)
- [docs/prd/prd-alphastream-rs.md](../prd/prd-alphastream-rs.md)
