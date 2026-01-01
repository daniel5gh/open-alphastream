# Task 01 — Architecture Overview

## Objective
Establish modules, boundaries, ownership, and data flow for alphastream-rs, reflecting decisions in [docs/RUST_IMPLEMENTATION.md](docs/RUST_IMPLEMENTATION.md).

## Architecture Module Map
- Formats: ASVR (encrypted, up to 1.5.0) and ASVP (plain) decoders; version-aware parse and per-frame decode to alpha masks.
- Transport (abstraction): unified async range-read over HTTP, local file, and in-memory buffers.
- Async Runtime: Tokio multi-thread executor owned by the library; builder-configurable.
- Scheduler: rate control, prioritization, prefetch window management, back-pressure.
- Frame Cache: LRU cache of 512 frames keyed by frame index; hit/miss instrumentation.
- Rasterizer: polystream rasterization to alpha mask; nearest-neighbor resize to requested output size.
- API Facade (Rust): synchronous API for source setup, rate control, and get_frame.
- C ABI (P/Invoke): stable C functions (CV_*) exposing facade; per-instance last error.
- Error Model: explicit codes/messages; per-instance last error; non-panicking failures.

## Boundaries & Traits
Primary trait boundaries and responsibilities:
- Transport (async): open, read_range(offset, len) -> bytes; consistent error semantics across HTTP/file/memory.
- ASFormat (decode): metadata(), decode_frame(index, bytes) -> AlphaMask; version/capability handling internal.
- Scheduler (async): set_rate(fps), request(frame_index), prefetch(window); coordinates transport/format tasks and throttling.
- Cache (sync): get(index) -> Option<AlphaMask>, put(index, mask), size(), evict(); LRU policy.
- Rasterizer (sync): rasterize(mask) -> AlphaMask, resize(mask, width, height) -> R8Bitmap; nearest-neighbor scaling.
- Facade (sync): get_frame(index, width, height) -> Option<R8Bitmap>; start/stop; last_error()/clear_error().

Sync/async crossing points:
- Transport: async boundary for range I/O.
- Format decode: CPU-bound; executed via spawn_blocking on dedicated worker threads.
- Scheduler: async orchestrator; dispatches transport+decode futures; signals back-pressure.
- Cache: synchronous map with internal mutex-free or fine-grained locking; accessed from async via lightweight boundaries.
- Rasterizer: synchronous compute; invoked post-cache or immediately on fresh decode.
- Facade: synchronous; bridges to internal async runtime; C ABI crosses FFI boundary.

Memory layout for get_frame (R8):
- Channel: R8 unorm single-channel alpha.
- Origin/order: top-left origin, row-major, tightly packed.
- Stride: $stride = width \times 1$ bytes per row.
- Semantics: 0 transparent, 255 opaque; non-premultiplied alpha.

## Ownership & Lifecycle
- Async runtime ownership: library-owned Tokio; initialized on first handle construction; shut down with last handle drop.
- Last error scope: per-instance; thread-safe set/get; not process-global.
- Resource lifetimes: handle owns transport, format, scheduler, cache; create via builder; destroy via drop; pending tasks are cancelled on drop.
- Buffer ownership rules:
  - Rust facade: returns an owned contiguous buffer (Vec<u8>) plus width/height/stride; no borrowing of internal cache buffers.
  - C ABI: returns a handle to a library-managed buffer with explicit release function; caller must call CV_Frame_Release() to free; data pointer remains valid until release.

## Crate/module layout proposal — [rust/alphastream-rs](rust/alphastream-rs)
- transport/ — abstraction + http/local/memory implementations
- formats/ — ASVR/ASVP parsers and decoders
- runtime/ — Tokio bootstrap and builder configuration
- scheduler/ — rate control and prioritization
- cache/ — LRU cache (512 frames)
- raster/ — polystream rasterizer and nearest-neighbor scaler
- api/ — Rust facade
- ffi/ — C ABI (CV_*)

## Data Flow Summary
Narrative: source → transport.read_range (async) → formats.decode (blocking via spawn_blocking) → scheduler (async coordination) → cache (sync) → rasterizer (sync) → api.get_frame (sync) → C ABI.

```text
[Source]
   |
   v
[Transport.read_range]  <-- async I/O boundary
   |
   v
[Formats.decode]        <-- blocking CPU; spawn_blocking
   |
   v
[Scheduler]             <-- async orchestration (rate, priorities, prefetch)
   |
   v
[Cache (LRU 512)]       <-- sync access
   |
   v
[Rasterizer]            <-- sync; polystreams + nearest-neighbor resize
   |
   v
[API Facade.get_frame]  <-- sync
   |
   v
[C ABI (CV_*)]          <-- FFI boundary
```

## Key Formulas
- Timebase: $t_n = \frac{n}{60}$
- Row stride: $stride = width \times 1$

## Acceptance Criteria
- Modules and traits documented with responsibilities and boundaries.
- Ownership/lifecycle and buffer rules stated.
- Crate/module layout proposal provided.
- Data flow text diagram included with async annotations.
- Cross-references to relevant tasks present.

## Cross-references
- Formats: [docs/tasks/02-format-abstraction.md](docs/tasks/02-format-abstraction.md)
- Transport abstraction: [docs/tasks/03-transport-abstraction.md](docs/tasks/03-transport-abstraction.md)
- Transport HTTP: [docs/tasks/04-transport-http.md](docs/tasks/04-transport-http.md)
- Transport local: [docs/tasks/05-transport-local.md](docs/tasks/05-transport-local.md)
- Transport in-memory: [docs/tasks/06-transport-in-memory.md](docs/tasks/06-transport-in-memory.md)
- Rasterization (polystreams): [docs/tasks/10-rasterization-polystreams.md](docs/tasks/10-rasterization-polystreams.md)
- Rasterization (resize): [docs/tasks/11-rasterization-resize.md](docs/tasks/11-rasterization-resize.md)
- Public API facade: [docs/tasks/12-public-api-facade.md](docs/tasks/12-public-api-facade.md)
- C ABI (P/Invoke): [docs/tasks/13-c-abi-pinvoke.md](docs/tasks/13-c-abi-pinvoke.md)
- Error model: [docs/tasks/14-error-model.md](docs/tasks/14-error-model.md)
- Implementation reference: [docs/RUST_IMPLEMENTATION.md](docs/RUST_IMPLEMENTATION.md)
