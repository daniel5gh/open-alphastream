# Task 06 — Transport: In-Memory

## Objective
Implement an in-memory slice-backed transport for alphastream-rs, providing zero-copy reads where possible.

## Scope
- Slice Backing: Use a `Vec<u8>` or slice to back the data, enabling zero-copy reads with bounds checks.
- Range Reads: Implement `read_range` using slice subranges for direct memory access.
- Performance: Eliminate I/O overhead through direct memory access.
- Error Mapping: Map bounds errors to `TransportError::Transport` (e.g., "OutOfBounds").

## Deliverables
- Unified transport trait implementation for in-memory data sources.

## Dependencies
- [docs/tasks/01-architecture-overview.md](docs/tasks/01-architecture-overview.md)
- [docs/tasks/03-transport-abstraction.md](docs/tasks/03-transport-abstraction.md)

## Implementation Checklist
- Define `InMemoryTransport` struct holding `data: Vec<u8>`.
- Implement `Transport` trait with `Error = TransportError`.
- `open`: Parse URI or initialize with provided data; return `InMemoryTransport`.
- `len`: Return `data.len()` as `u64`.
- `read_range`: Perform bounds check; return `Bytes::from(&data[offset..offset + actual_size])` where `actual_size = min(size, data.len() - offset)`.
- Bounds check: If `offset > data.len()`, return `TransportError::Transport("Out of bounds".to_string())`.
- Code snippet for slice reader:
  ```rust
  pub struct InMemoryTransport {
      data: Vec<u8>,
  }

  impl Transport for InMemoryTransport {
      type Error = TransportError;

      async fn open(_uri: &str) -> Result<Self, Self::Error> {
          // For in-memory, URI may be ignored; data provided separately or via builder
          Ok(Self { data: Vec::new() }) // Placeholder
      }

      async fn len(&self) -> Result<u64, Self::Error> {
          Ok(self.data.len() as u64)
      }

      async fn read_range(&self, offset: u64, size: u32) -> Result<Bytes, Self::Error> {
          let start = offset as usize;
          let len = self.data.len();
          if start > len {
              return Err(TransportError::Transport("Out of bounds".to_string()));
          }
          let end = (start + size as usize).min(len);
          Ok(Bytes::from(&self.data[start..end]))
      }

      fn cancel(&self) { /* No-op for in-memory */ }
      fn backpressure(&self) -> bool { false }
  }
  ```
- Range logic: Use slice indexing for subranges, ensuring zero-copy.

## Acceptance Criteria
- Unit tests passing with full coverage of in-bounds and out-of-bounds cases.
- Performance baseline: Reads complete in constant time with no I/O overhead.

## References
- [docs/RUST_IMPLEMENTATION.md](docs/RUST_IMPLEMENTATION.md)
- [docs/tasks/03-transport-abstraction.md](docs/tasks/03-transport-abstraction.md)
