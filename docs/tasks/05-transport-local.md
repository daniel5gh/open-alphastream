# Task 05 — Transport: Local

## Objective
- Implement local file transport for alphastream-rs, preferring memory-mapped I/O with buffered fallback for range reads.

## Scope
- File opening with memory-mapped I/O preference and 128 KiB buffered fallback.
- Range reads implementation using mmap slices or buffered seeks.
- Alignment and performance notes for mmap and copy overhead reduction.
- Error mapping from file system errors to TransportError variants.

## Implementation

### File Opening
Prefer memory-mapped I/O for direct memory access. Fallback to buffered reads with buffer size of 128 KiB.

Buffer size formula: `buffer_size = 128 * 1024` bytes.

### Range Reads
Implement `read_range` using mmap slice access or buffered seek and read operations.

### Alignment & Performance
For mmap, align ranges to system page size to minimize overhead. Reduces copy overhead by avoiding intermediate buffers for direct access.

### Error Mapping
Map `std::io::Error` to `TransportError`:
- `NotFound` → `Transport("File not found")`
- `PermissionDenied` → `Transport("Permission denied")`
- Other I/O errors → `Transport(description)`

## Implementation Checklist
- mmap open:
  ```rust
  use memmap2::Mmap;
  let file = std::fs::File::open(path)?;
  let mmap = unsafe { Mmap::map(&file)? };
  ```
- Buffered fallback:
  ```rust
  use std::io::BufReader;
  let file = std::fs::File::open(path)?;
  let mut reader = BufReader::with_capacity(128 * 1024, file);
  ```
- Range logic:
  ```rust
  // For mmap
  let start = offset as usize;
  let end = (offset + size as u64) as usize;
  let slice = &mmap[start..end];
  // For buffered
  reader.seek(std::io::SeekFrom::Start(offset))?;
  let mut buf = vec![0; size as usize];
  reader.read_exact(&mut buf)?;
  ```

## Acceptance Criteria
- Correctness verified on large files exceeding 1 GB.
- Memory footprint measured and optimized.
- Performance benchmarked against buffered I/O.

## References
- [docs/RUST_IMPLEMENTATION.md](docs/RUST_IMPLEMENTATION.md)
- [docs/tasks/03-transport-abstraction.md](docs/tasks/03-transport-abstraction.md)
