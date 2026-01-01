# Transport Abstraction (Unified Async Range Reader)

## Objective
Define a unified async transport trait exposing ranged reads for ASVR/ASVP across HTTP, local files, and in-memory sources.

## Trait Definition
The `Transport` trait provides asynchronous ranged reads with support for cancellation and backpressure hooks.

Associated types:
- `Error`: Associated error type implementing `std::error::Error`.

Methods:
- `async fn open(...) -> Result<Self, Self::Error>`: Opens the transport resource.
- `async fn len(&self) -> Result<u64, Self::Error>`: Returns the total length of the data.
- `async fn read_range(&self, offset: u64, size: u32) -> Result<Bytes, Self::Error>`: Reads a range of bytes starting at offset with the given size.
- Cancellation hook: `fn cancel(&self)`: Cancels ongoing operations.
- Backpressure hook: `fn backpressure(&self) -> bool`: Indicates if backpressure is applied.

## Method Signatures
```rust
pub trait Transport {
    type Error: std::error::Error;

    async fn open(uri: &str) -> Result<Self, Self::Error>
    where
        Self: Sized;

    async fn len(&self) -> Result<u64, Self::Error>;

    async fn read_range(&self, offset: u64, size: u32) -> Result<Bytes, Self::Error>;

    fn cancel(&self);

    fn backpressure(&self) -> bool;
}
```

## Error Taxonomy
Errors map to per-instance last error categories: Transport and Timeout.

Define `TransportError` enum:
```rust
#[derive(Debug, thiserror::Error)]
pub enum TransportError {
    #[error("Transport error: {0}")]
    Transport(String),
    #[error("Timeout error")]
    Timeout,
    // Other variants as needed
}
```

## Range Alignment & Partial Reads
- Ranges must be aligned to chunk_size (default 1 MiB).
- Partial reads are allowed; if size exceeds available data, return available bytes.
- Chunking: Divide large ranges into aligned chunks for efficient reading.

## Builder Integration
Caps and timeouts are configured via the builder:
- `max_concurrent_ranges`: Default 4, limits concurrent read operations.
- `chunk_size`: Default 1 MiB, used for alignment and chunking.
- Timeouts: Applied to read operations; configurable per transport instance.

Integration with scheduler prefetch: Builder sets caps that the scheduler respects for backpressure.

## Mock Implementation
A minimal mock transport for testing uses an in-memory buffer. Configurable delays and errors:
- Stores data in `Vec<u8>`.
- `read_range` simulates delays with `tokio::time::sleep`.
- Injects errors based on configuration (e.g., timeout after delay).

## Mapping Table
| Implementation | Task |
|----------------|------|
| HTTP Transport | [docs/tasks/04-transport-http.md](docs/tasks/04-transport-http.md) |
| Local File Transport | [docs/tasks/05-transport-local.md](docs/tasks/05-transport-local.md) |
| In-Memory Transport | [docs/tasks/06-transport-in-memory.md](docs/tasks/06-transport-in-memory.md) |

## Acceptance Criteria
- Trait compiles; mock passes unit tests; backpressure hooks invoked in scheduler tests.

## References
- [docs/tasks/01-architecture-overview.md](docs/tasks/01-architecture-overview.md)
- [docs/RUST_IMPLEMENTATION.md](docs/RUST_IMPLEMENTATION.md)
- [docs/tasks/04-transport-http.md](docs/tasks/04-transport-http.md)
- [docs/tasks/05-transport-local.md](docs/tasks/05-transport-local.md)
- [docs/tasks/06-transport-in-memory.md](docs/tasks/06-transport-in-memory.md)
