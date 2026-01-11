# ADR 0006: Public API Facade and C ABI (P/Invoke)

## Status
Accepted

## Context
Alphastream-rs provides a high-level, ergonomic Rust API for core functionality, and a stable C ABI for interoperability with .NET and other languages. Buffer ownership, error mapping, and lifetime rules are clearly defined for both Rust and FFI consumers.

## Decision
- Expose a public Rust API facade (`AlphaStreamProcessor`) with async methods for source setup, frame access, and error handling.
- Provide a stable C ABI (CV_*) for P/Invoke, with:
  - Cdecl calling convention
  - Explicit buffer ownership and lifetime rules
  - Error codes mapped to integer values and descriptive text
  - Handles and pointers validated for safety
- All C ABI functions are thin wrappers around `AlphaStreamProcessor`, using an opaque `AlphaStreamCHandle` struct that owns the processor instance.
- Document all API and ABI methods with examples and usage notes.

## Consequences
- Enables easy integration in Rust and .NET environments
- Ensures safety and clarity for FFI consumers
- Simplifies cross-language testing and support

## API/ABI Overview
- **Rust API**: All operations are performed via `AlphaStreamProcessor`. Async methods provide frame access, metadata, and error handling. Builder pattern is used for configuration. Thread-safe and non-blocking.
- **C ABI**: All functions operate on an opaque `AlphaStreamCHandle` that owns an `AlphaStreamProcessor`. Memory management and error state are handled internally. Users must not free returned pointers directly.

## Example Usage
**Rust:**
```rust
let processor = AlphaStreamProcessor::new_asvp("file.asvp", 1920, 1080, ProcessingMode::Both)?;
let frame = processor.get_frame(0, 1920, 1080).await;
```

**C:**
```c
AlphaStreamCHandle* handle = CV_create();
bool ok = CV_init(handle, "file.asvp", 0, 1920, 1080, "1.0.0", 0, 1024, 512, 256, 5000, 30000);
const void* frame = CV_get_frame(handle, 0);
CV_destroy(handle);
```

## References
- [docs/tasks/12-public-api-facade.md](../tasks/12-public-api-facade.md)
- [docs/tasks/13-c-abi-pinvoke.md](../tasks/13-c-abi-pinvoke.md)
- [docs/RUST_IMPLEMENTATION.md](../RUST_IMPLEMENTATION.md)
- [docs/prd/prd-alphastream-rs.md](../prd/prd-alphastream-rs.md)
