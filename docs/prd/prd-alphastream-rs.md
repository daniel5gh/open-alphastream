# Alphastream-rs: High-Performance Rust Implementation PRD

**Participants:**
- Product Owner: [User] (Human)
- Technical Analyst: Roo AI Agent (AI, PRD structure, requirements synthesis)
- Engineering Lead: [To be assigned]
- Rust Developer(s): [To be assigned]
- Stakeholders: [To be assigned]

**Status:** Planning  
**Target Release:** [TBD]  
**Last Updated:** 2026-01-10

## Overview
A production-grade, high-performance Rust implementation of libalphastream for decoding and rasterizing alpha-stream frames. The implementation must deliver correctness, throughput, and predictable latency under I/O and compute pressure, supporting evolving formats and diverse transports.

## Team Goals and Business Objectives
- Deliver a Rust library that matches or exceeds the performance of the original libalphastream
- Support decoding and rasterization for both encrypted (ASVR) and plain (ASVP) formats
- Enable integration with modern graphics APIs (Vulkan/OpenGL)
- Provide robust async APIs for high-throughput streaming scenarios
- Ensure extensibility for future format and transport evolution

## Background and Strategic Fit
Alphastream is a core technology for high-performance alpha mask streaming. The Rust implementation targets production use, enabling integration in latency-sensitive and resource-constrained environments. This aligns with the project's goal to provide both a reference (Python) and a production (Rust) implementation, supporting open standards and future extensibility.

## Assumptions
### Technical
- Rust async ecosystem (Tokio) is mature enough for required concurrency
- SIMD and zero-copy optimizations are feasible for decoding/rasterization
- GPU upload targets (OpenGL/Vulkan) require R8 unorm textures

### Business
- Sufficient engineering resources are available for Rust development
- Performance is a primary differentiator for adoption

### User
- Users require both HTTP and local file support
- Users expect predictable error handling and diagnostics

**Note:** These assumptions should be validated during discovery or early implementation.

## User Stories

### Story 1: High-Performance Frame Decoding
As a **developer integrating alphastream-rs**, I want to **decode and rasterize alpha-stream frames at high throughput**, so that **my application can render real-time alpha masks with minimal latency**.

**Acceptance Criteria:**
- [ ] Library decodes ASVR and ASVP formats as per [docs/FILE_FORMAT.md](../FILE_FORMAT.md)
- [ ] Decoding throughput >= [TBD] frames/sec on reference hardware
- [ ] SIMD and zero-copy optimizations are used where possible
- [ ] Decoding is non-blocking and supports async usage

**Success Metrics:**
- Decoding throughput meets or exceeds [TBD] fps on reference hardware
- Latency per frame decode < [TBD] ms (95th percentile)

---

### Story 2: Flexible Transport Support
As a **developer**, I want to **load alpha-stream data from HTTP, local files, or in-memory buffers**, so that **I can integrate with diverse storage and streaming backends**.

**Acceptance Criteria:**
- [ ] Unified async trait for HTTP, file, and in-memory sources
- [ ] HTTP supports range requests, retries, and timeouts as specified
- [ ] Local files use mmap where possible, fallback to buffered reads
- [ ] In-memory transport supports slice-backed access

**Success Metrics:**
- All transports pass integration tests for correctness and performance
- HTTP and file sources achieve target throughput with no data loss

---

### Story 3: Precise Frame Control, Async Scheduling, and Rate Control
As a **developer integrating alphastream-rs**, I want to **request and render an exact frame by index at a specific time**, so that **I can synchronize alpha masks precisely with color channels and external media streams**.

**Acceptance Criteria:**
- [ ] API exposes get_frame(frame_index, width, height) for direct frame access
- [ ] Library ensures enough frames are pre-buffered to meet typical access patterns; pre-buffering rate is managed internally
- [ ] get_frame(frame_index, ...) always returns the frame for the requested index if available, or None with last_error set if not
- [ ] Frame delivery is deterministic: no frame reordering or skipping unless explicitly requested
- [ ] Scheduler supports prefetch, LRU cache, and priority escalation for foreground requests
- [ ] Backpressure and timeboxing (12ms) are enforced for get_frame

**Success Metrics:**
- Frame requests for a given index return the correct frame in 100% of cases (when available)
- Frame requests are fulfilled within 12ms timebox in 95% of cases
- Cache hit rate >= [TBD]% under typical workloads

---

### Story 4: Robust Error Handling
As a **developer**, I want to **receive explicit error codes and messages for failures**, so that **I can diagnose and handle issues predictably**.

**Acceptance Criteria:**
- [ ] Per-instance last_error() and clear_error() APIs
- [ ] Error codes: None, NotReady, Timeout, Decode, Transport
- [ ] All error paths are non-panicking and thread-safe

**Success Metrics:**
- 100% of error scenarios are covered by integration tests
- No panics in error paths under stress

---

### Story 5: GPU-Ready Output
As a **developer**, I want to **receive alpha masks in a format directly uploadable to GPU (OpenGL/Vulkan)**, so that **I can minimize conversion overhead in my rendering pipeline**.

**Acceptance Criteria:**
- [ ] Output format is R8 unorm, row-major, tightly packed, top-left origin
- [ ] Compatible with GL_R8 and VK_FORMAT_R8_UNORM
- [ ] Nearest-neighbor scaling is used for resizing

**Success Metrics:**
- Output passes conformance tests for GPU upload
- No additional conversion required in reference integrations

## User Interaction and Design
- API surface documented with examples for all transports and frame access patterns
- Diagnostics and error reporting are accessible via API
- Integration guides for OpenGL/Vulkan provided

## C ABI Specification

The following C ABI functions are provided for .NET P/Invoke and C interoperability. All functions use `extern "C"` and `#[no_mangle]`:

| Function | Signature | Description |
|----------|-----------|-------------|
| `CV_create` | `*mut AlphaStreamHandle CV_create(void)` | Allocates and returns a new handle. Must be freed with `CV_destroy`. |
| `CV_destroy` | `void CV_destroy(*mut AlphaStreamHandle)` | Frees the handle and associated memory. |
| `CV_get_name` | `const char* CV_get_name(*mut AlphaStreamHandle)` | Returns the plugin name as a static C string. |
| `CV_get_version` | `const char* CV_get_version(*mut AlphaStreamHandle)` | Returns the plugin version as a static C string. |
| `CV_get_last_error_code` | `int CV_get_last_error_code(*mut AlphaStreamHandle)` | Returns the last error code for the handle. -1 if handle is null. |
| `CV_get_last_error_text` | `const char* CV_get_last_error_text(*mut AlphaStreamHandle)` | Returns the last error message for the handle. |
| `CV_get_total_frames` | `unsigned int CV_get_total_frames(*mut AlphaStreamHandle)` | Returns the total number of frames. 0 if handle is null. |
| `CV_get_frame_size` | `unsigned int CV_get_frame_size(*mut AlphaStreamHandle)` | Returns the frame size (width * height). 0 if handle is null. |
| `CV_init` | `bool CV_init(*mut AlphaStreamHandle, const char* base_url, unsigned int scene_id, unsigned int width, unsigned int height, const char* version, unsigned int start_frame, unsigned int l0_buffer_length, unsigned int l1_buffer_length, unsigned int l1_buffer_init_length, unsigned int init_timeout_ms, unsigned int data_timeout_ms)` | Initializes the handle with connection and stream parameters. Returns true on success. |
| `CV_get_frame` | `const void* CV_get_frame(*mut AlphaStreamHandle, unsigned long long frame_index)` | Returns a pointer to the R8 mask buffer for the requested frame, or null on error. |
| `CV_get_triangle_strip_vertices` | `bool CV_get_triangle_strip_vertices(*mut AlphaStreamHandle, unsigned long long frame_index, float** out_vertices, size_t* out_count)` | Returns triangle strip vertex data for the frame. Returns true on success, false on error. |

- All functions require a valid handle unless otherwise noted.
- The `*mut AlphaStreamHandle` type is fully opaque to the API user and should be treated as a generic pointer (e.g., `void*` in C or `IntPtr` in C#). No assumptions should be made about its structure or contents.
- Error codes: 0 = success, negative = error, 3 = NotFound/out-of-range.
- All string parameters must be valid UTF-8 null-terminated C strings.
- All memory allocation and deallocation must be managed via `CV_create`/`CV_destroy`.
- Frame data returned by `CV_get_frame` is valid until the next call to `CV_get_frame` or `CV_destroy`.
- See [rust/alphastream-rs/src/lib.rs](../../rust/alphastream-rs/src/lib.rs) for implementation details.

## Additional Implementation Requirements (from Task Analysis)

### Thread-Safe Cache Operations
- Cache must use `Arc<RwLock<>>` or equivalent for safe concurrent access from multiple threads.
- All cache operations must be race-free and validated by concurrent access tests.

### Builder Configuration
- Expose builder pattern for configuring runtime, pool sizes, transport caps/timeouts, and processing types (bitmask, triangles, or both).
- All configuration options must have documented defaults and override ranges.

### Metadata & Timebase
- Metadata parsing and index→time mapping ($t_n = n/60$) must be implemented and exposed via API.
- Metadata validation rules and timebase drift correction must be documented.

### C ABI / PInvoke
- Public API must be available via a stable C ABI for .NET interop, with buffer ownership and error mapping rules clearly documented.
- Pointer lifetimes and error code mapping must be specified for FFI consumers.

### Integration Tests & Benchmarks
- End-to-end integration tests must cover all source/format pairs and error paths.
- CI benchmarks must be implemented with defined metrics (throughput, latency, cache hit rate, etc.).
- Performance regressions must be automatically flagged in CI.

### Error Model
- Per-instance, thread-safe error state with explicit mapping from subsystem errors.
- All error state updates must be atomic or mutex-protected.

### Transport Abstraction
- Unified async trait with chunked range reads, backpressure, and error taxonomy for HTTP, local file, and in-memory sources.
- All transports must support chunk alignment, partial reads, and error mapping as specified.

## Questions
| Question | Owner | Status | Resolution |
|----------|-------|--------|------------|
| Metadata schema and versioning exposure? | Product Owner | Open |  |
| Performance targets and benchmarking methodology? | Product Owner | Open |  |
| Bitmap pixel format for Vulkan/OpenGL upload? | Product Owner | Resolved | R8 unorm |
| Scaling policy (nearest vs bilinear)? | Product Owner | Resolved | Nearest-neighbor |
| Error handling: global vs per-instance? | Product Owner | Resolved | Per-instance |
| get_frame contract for “nothing” and last_error? | Product Owner | Resolved | Option return with last_error set |
| Cache capacity, eviction, prefetch window? | Product Owner | Resolved | 512 frames, LRU, 120 prefetch |
| Scheduler back-pressure and preemption? | Product Owner | Resolved | Adaptive, 12ms timebox |
| Timebase and drift handling? | Product Owner | Resolved | Monotonic clock, drift correction |
| Benchmark metrics and datasets? | Product Owner | Open |  |

## Out of Scope
### Not in This Release
- Video decoding or non-alpha formats
- Mobile-specific optimizations
- Custom GPU formats beyond R8 unorm
- UI or visualization tools

### Explicitly Decided Against
- Global error state (process-wide)
- Blocking APIs for frame access
- Bilinear or advanced scaling in v1

## Technical Specifications

### Technology Stack
- **Language:** Rust (2021 edition)
- **Async Runtime:** Tokio
- **HTTP Client:** reqwest
- **Memory Mapping:** memmap2
- **Target Graphics APIs:** OpenGL, Vulkan

### Architecture Patterns
- Layered: formats, transport, scheduler, rasterizer, cache, API facade
- Traits for extensibility and testability
- LRU cache for frames
- Builder pattern for configuration

### API Contracts
```rust
// Frame access
fn get_frame(&self, frame_index: u32, width: u32, height: u32) -> Option<&[u8]>;
fn last_error(&self) -> Option<(ErrorCode, String)>;
fn clear_error(&mut self);

// Source management
fn load_source(&mut self, source: SourceConfig) -> Result<(), Error>;
fn close(&mut self);

// Triangle strip access
fn get_triangle_strip_vertices(&self, frame_index: u32) -> Result<Vec<f32>, Error>;
```

### Data Models
```rust
// Error codes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ErrorCode {
    None,
    NotReady,
    Timeout,
    Decode,
    Transport,
}

// Source configuration
enum SourceConfig {
    Http { url: String },
    File { path: PathBuf },
    Memory { data: Arc<[u8]> },
}
```

### Performance Requirements
- **Decoding throughput:** >= [TBD] fps on reference hardware
- **Frame decode latency:** < [TBD] ms (95th percentile)
- **Cache hit rate:** >= [TBD]% under typical workloads
- **Memory usage:** Deterministic, capped by 512-frame LRU

### Security Requirements
- No panics on malformed or malicious input
- All I/O and decode errors are handled gracefully
- No unsafe code in public API surface

### Integration Points
- [docs/FILE_FORMAT.md](../FILE_FORMAT.md)
- [docs/FILE_FORMAT_PLAINTEXT.md](../FILE_FORMAT_PLAINTEXT.md)
- [docs/REVERSE_ENGINEERING.md](../REVERSE_ENGINEERING.md)
- [AGENTS.md](../../AGENTS.md)

## Implementation Guidance for AI Agents
- Follow architecture and API patterns as specified
- Use SIMD and zero-copy optimizations where possible
- Ensure all error paths are covered by tests
- Provide integration tests for all transports
- Document all public APIs and error codes
- Reference [docs/tasks/10-rasterization-polystreams.md](../tasks/10-rasterization-polystreams.md) for triangle strip details

## AI Implementation Metadata
```yaml
story_id: ALPHASTREAM-RS
priority: high
effort_estimate: [TBD]
complexity: high
risk_level: medium
tags:
  - rust
  - streaming
  - async
  - gpu
  - cache
dependencies:
  requires:
    - FILE_FORMAT.md
    - FILE_FORMAT_PLAINTEXT.md
    - REVERSE_ENGINEERING.md
  blocks:
    - Python reference implementation (for parity tests)
implementation_order: 1
tech_stack:
  - rust
  - tokio
  - reqwest
  - memmap2
  - opengl
  - vulkan
```
