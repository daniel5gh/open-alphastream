# ADR 0006: Public API Facade and C ABI (P/Invoke)

## Status
Accepted

## Context
Alphastream-rs must provide a high-level, ergonomic Rust API for core functionality, and a stable C ABI for interoperability with .NET and other languages. Buffer ownership, error mapping, and lifetime rules must be clear for both Rust and FFI consumers.

## Decision
- Expose a public Rust API facade with ergonomic methods for source setup, frame access, and error handling.
- Provide a stable C ABI (CV_*) for P/Invoke, with:
  - Cdecl calling convention
  - Explicit buffer ownership and lifetime rules
  - Error codes mapped to integer values and descriptive text
  - Handles and pointers validated for safety
- Document all API and ABI methods with examples and usage notes.

## Consequences
- Enables easy integration in Rust and .NET environments
- Ensures safety and clarity for FFI consumers
- Simplifies cross-language testing and support

## References
- [docs/tasks/12-public-api-facade.md](../tasks/12-public-api-facade.md)
- [docs/tasks/13-c-abi-pinvoke.md](../tasks/13-c-abi-pinvoke.md)
- [docs/RUST_IMPLEMENTATION.md](../RUST_IMPLEMENTATION.md)
- [docs/prd/prd-alphastream-rs.md](../prd/prd-alphastream-rs.md)
