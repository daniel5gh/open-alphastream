# Task 12 — Public API Facade

## Objective
Provide a high-level Rust API facade for alphastream-rs, exposing core functionality through ergonomic methods.

## Methods
The facade exposes the following methods:

```rust
fn load_source(&mut self, source: &str) -> Result<(), Error>
```

Loads a media source from the given URI.

```rust
fn set_rate(&mut self, rate: f64) -> Result<(), Error>
```

Sets the playback rate.

```rust
fn get_frame<'a>(&'a mut self) -> Result<Option<&'a Frame>, Error>
```

Retrieves the next frame, with lifetime tied to self.

```rust
fn last_error(&self) -> Option<&Error>
```

Returns the last error, if any.

## Ergonomics
- **Lifetimes**: Methods use appropriate lifetimes to ensure safe borrowing, e.g., `get_frame` returns a reference with lifetime `'a` bound to `self`.
- **Error Mapping**: Internal errors are mapped to a public `Error` enum, providing user-friendly error messages.

## Scope
- Ergonomic facade over subsystems.

## Deliverables
- Public module
- Docs and examples

## Dependencies
- [docs/tasks/02-format-abstraction.md](docs/tasks/02-format-abstraction.md)
- [docs/tasks/03-transport-http.md](docs/tasks/03-transport-http.md)
- [docs/tasks/04-transport-local.md](docs/tasks/04-transport-local.md)
- [docs/tasks/05-transport-in-memory.md](docs/tasks/05-transport-in-memory.md)
- [docs/tasks/06-async-runtime-concurrency.md](docs/tasks/06-async-runtime-concurrency.md)
- [docs/tasks/07-scheduler-rate-control.md](docs/tasks/07-scheduler-rate-control.md)
- [docs/tasks/08-frame-cache-policy.md](docs/tasks/08-frame-cache-policy.md)
- [docs/tasks/09-rasterization-resize.md](docs/tasks/09-rasterization-resize.md)

## Implementation Checklist
- Method signatures with proper lifetimes
- Error mapping from subsystem errors to public API errors
- Comprehensive documentation and doc-tests for each method

## Acceptance Criteria
- Example compiles
- Doc-tests pass

## References
- [docs/RUST_IMPLEMENTATION.md](docs/RUST_IMPLEMENTATION.md)
