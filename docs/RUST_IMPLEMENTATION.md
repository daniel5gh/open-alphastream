# Alphastream-rs Rust Implementation Specification (Initial Outline)

## Overview & Goals

- Target: high-performance, production-ready Rust implementation of libalphastream for decoding and rasterizing alpha-stream frames.
- Deliver both correctness and throughput with predictable latency under I/O and compute pressure.
- Abstractions allow format evolution and transport diversity.
- Reference documentation: [docs/FILE_FORMAT.md](docs/FILE_FORMAT.md) and [docs/FILE_FORMAT_PLAINTEXT.md](docs/FILE_FORMAT_PLAINTEXT.md), [docs/REVERSE_ENGINEERING.md](docs/REVERSE_ENGINEERING.md), [AGENTS.md](AGENTS.md)

### User-provided requirements (verbatim)

- High-performance Rust implementation of libalphastream.
- Async reading and decoding of ASVR and ASVP via an abstraction (format may evolve).
- Transport abstraction for ASVR/ASVP datastreams: HTTP, local files, or user-provided byte array.
- Async reading and rasterization at a user-provided rate (fps), producing a cache of alpha bit masks.
- Main API provides get_frame(frame_index, size) returning a bitmap suitable for Vulkan/OpenGL textures; prefer cache but may prioritize async worker to fetch; if frame unavailable, return nothing and set a global last error code and message.

TBD notes linked to requirements:
- Exact pixel format for returned bitmap (A8 vs RGBA8 vs R8) — TBD.
- Return contract for “nothing” (Option vs sentinel vs separate status query) — TBD.
- Global error scope (per-instance vs process-wide) and thread-safety — TBD.
- Priority policy when cache misses occur vs ongoing async work — TBD.
- FPS rate control interface naming and behavior (set_rate vs constructor) — TBD.

## Architecture Summary

- Formats layer: decoders for ASVR and ASVP with version awareness; single trait to unify read/parse/decode.
- Transport layer: pluggable sources (HTTP, file, in-memory) with streaming and range support.
- Scheduler: async orchestration to meet target fps; back-pressure and prioritization.
- Rasterizer: converts decoded alpha data to texture-ready bitmap at requested size.
- Cache: frame-indexed alpha bit mask cache with eviction and prefetch.
- API facade: simple entry points for source setup, rate control, frame access, error reporting.
- Error model: non-panicking, explicit error codes/messages; diagnosable failure paths.

References:
- Format details: [docs/FILE_FORMAT.md](docs/FILE_FORMAT.md) and [docs/FILE_FORMAT_PLAINTEXT.md](docs/FILE_FORMAT_PLAINTEXT.md)
- Reverse engineering notes: [docs/REVERSE_ENGINEERING.md](docs/REVERSE_ENGINEERING.md)
- Project guidelines: [AGENTS.md](AGENTS.md)

## Data Format Abstraction

- Unified trait to represent evolving formats (ASVR encrypted variants up to 1.5.0; ASVP plain).
- Responsibilities: frame discovery, metadata, per-frame decode to alpha mask.
- Versioning and capability negotiation handled internally; external API remains stable.
- Performance emphasis: zero-copy where possible, SIMD-friendly decoding paths.
- Validation against documented structures in [docs/FILE_FORMAT.md](docs/FILE_FORMAT.md) and [docs/FILE_FORMAT_PLAINTEXT.md](docs/FILE_FORMAT_PLAINTEXT.md); edge cases per [docs/REVERSE_ENGINEERING.md](docs/REVERSE_ENGINEERING.md).
- TBD: exact trait naming and method set; error taxonomy.

## Transport Abstraction

- Unified async trait across HTTP, local file, and in-memory sources; mandatory range-read capability; integrates scheduler backpressure.

### HTTP

- Client: reqwest async with TLS defaults.
- Range requests: uses the HTTP Range header; example form: "Range: bytes=START-END".
- Chunking and concurrency: chunk_size=$1\,\text{MiB}$; max_concurrent_ranges=4; ranges dispatched in parallel with ordered reassembly.
- Reliability: retries=3 with exponential backoff starting at $250\,\text{ms}$; per-request timeout $10\,\text{s}$; handles 206 Partial Content.
- Backpressure: concurrency dynamically throttled by scheduler signals to avoid queue buildup.

### Local Files

- Prefer memory-mapped I/O (mmap) for zero-copy access and OS page-cache leverage.
- Fallback: buffered reads with $128\,\text{KiB}$ when mmap is unavailable or unsuitable; range reads map directly to file offsets.

### In-memory

- Slice-backed reader providing range access via slice indexing; lifetime tied to caller-supplied buffer.

- All transports expose the same async trait surface: open, read, seek, range-read; consistent error semantics.

## Async Scheduling & FPS

- User sets target fps; scheduler orchestrates decode+rasterize cadence.
- Back-pressure: throttle ingest/compute; avoid unbounded queues.
- Priorities: foreground get_frame requests can preempt prefetch when needed.
- Timebase: monotonic clock for scheduling; drift detection and correction.
- Graceful degradation: lower resolution or skip frames under pressure — TBD.
- Runtime: Tokio multi-thread, owned by the library; see "Async Runtime & Concurrency Model".

## Async Runtime & Concurrency Model

- Runtime: Tokio multi-thread executor owned by the library; started on handle construction if not already running internally.
- Defaults: $worker_\threads = \text{num\_cpus}$; pools: io=4 async tasks; decode=$\text{num\_cpus}$ blocking threads; raster=2 async tasks.
- Blocking decode: performed on dedicated blocking threads to avoid starving async tasks.
- Affinity: CPU core pinning is available but off by default.

### Configuration (Builder)

- Override $worker_\threads$ and per-pool sizing (io/decode/raster) via builder options.
- Toggle to enable core pinning (off by default).
- Enable/disable processing types (triangles, bitmask, or both) via builder options.
- HTTP range concurrency and timeouts are configurable here and align with the Transport Abstraction.

## Async Scheduler Timebase & Rate Control

- Timebase: index-driven at target_fps=60 with mapping $t_n = \frac{n}{60}$.
- Rate control: adaptive backpressure; when behind, duplicate the last decoded frame; when oversupplied, skip frames to maintain cadence; no interpolation.
- Prefetch and cache: prefetch_window=120 frames; cache_size=512 frames with LRU eviction.
- Prefetch is constrained by the cache cap: $prefetch = \min(\text{prefetch\_window}, \text{remaining capacity})$.
- Priority handling: get_frame escalates the requested frame for immediate decode with a 12ms timebox; if not ready within the timebox, return nothing and set last error=Timeout on the handle.

## Frame Cache & Rasterization

- Cache holds alpha bit masks or triangle strips (or both) keyed by frame index; LRU with size/fps-aware prefetch. See [docs/tasks/10-rasterization-polystreams.md](docs/tasks/10-rasterization-polystreams.md) for triangle strip details.
- Rasterizer transforms alpha masks to requested bitmap size or generates triangle strips; scaling policy (nearest/bilinear) — TBD.
- Bitmap layout optimized for GPU upload (tightly packed, row-aligned); exact format — TBD.
- Metadata-driven pipeline (dimensions, stride, version flags).
- Instrumentation: hit/miss counters, stall metrics.

### Frame Cache Policy

- Cap: 512 frames (count-based), default.
- Cost model: R8 masks occupy $width \times height$ bytes per frame; cap is by count, not bytes.
- Eviction: strict LRU; no pinning of recent frames.
- Prefetch interaction: scheduler prefetch_window = 120 frames but bounded so total cached frames never exceeds 512; at cap, new inserts evict LRU.

## Frame Pixel Format & GPU Upload

- Channel layout: R8 unorm, single-channel alpha mask.
- Memory layout: top-left origin; row-major, tightly packed. Row stride formula: $stride = width \times 1$ bytes.
- Endianness and alpha semantics: little-endian byte values in [0,255]; 0 fully transparent, 255 fully opaque; non-premultiplied alpha.
- Compatibility (GPU upload targets):
  - OpenGL: GL_R8
  - Vulkan: VK_FORMAT_R8_UNORM

## Resize Policy

- Scaling: nearest-neighbor from cached alpha bit masks to requested output size.
- Output format: R8 unorm per the defined layout above.
- Sampling mapping:

$$
x' = \left\lfloor \frac{x \cdot W_{src}}{W_{dst}} \right\rfloor,\quad y' = \left\lfloor \frac{y \cdot H_{src}}{H_{dst}} \right\rfloor
$$

- Coordinates: $x\in[0, W_{dst}-1], y\in[0, H_{dst}-1]$ select source indices $(x', y')$.

## Public APIs (high-level)

- Source management: load_source(source), close(); sources can be constructed/selected as HTTP, local file, or in-memory slice via transport constructors.
- Rate control: set_rate(fps), get_rate().
- Frame access: get_frame(frame_index, width, height) returns an R8 bitmap per the defined layout; when resizing, nearest-neighbor scaling is applied; cache-first with possible immediate decode escalation within a 12ms timebox; on failure returns None and sets the per-instance last error; retrieve via last_error().
- Triangle access: get_triangle_strip_vertices(frame_index) -> Result<Vec<f32>, Error>, returning a Vec<f32> containing x,y positions in triangle strip order suitable for graphics APIs like wgpu with TriangleStrip topology. Triangle strip format: vertices are arranged sequentially where each set of three consecutive vertices forms a triangle, sharing edges for efficient rendering.
- Diagnostics: last_error() returns {code, message} for this handle; clear_error() resets the handle's last error to None; get_metadata() exposes format/source metadata.
- Lifecycle: start(), stop(); optional auto-start on source load — TBD.

## Error Handling Strategy

- Per-instance last error code/message maintained on each handle; thread-safe updates via internal synchronization.
- Errors categorized (I/O, format, decode, rasterize, scheduling, resource).
- Non-panicking APIs; failures are reported via status + last_error.
- Logging levels and integration points — TBD.
- Unavailable frame or timebox expiration: get_frame returns nothing; handle last error is set accordingly (NotReady or Timeout).

### Error Model: Per-Instance Last Error

- Scope: last error stored per loader/cache handle; not process-global or thread-local.
- Codes: None, NotReady, Timeout, Decode, Transport.
- Set conditions:
  - get_frame returns None (e.g., frame unavailable or 12ms timebox expired).
  - Transport read failures.
  - Decode failures.
- Retrieval/clear: handle.last_error() returns {code, message}; handle.clear_error() resets to None.
- Concurrency note: updates are confined to the instance; internal synchronization avoids cross-handle races.

## Performance Considerations

- Multi-thread runtime chosen to reduce contention; decode runs on separate blocking threads.
- Default pool sizing: io=4, decode=$\text{num\_cpus}$, raster=2 for balanced throughput.
- Optional core pinning available for deterministic performance at the cost of flexibility.
- Minimize allocations via buffer pools; reuse and pin memory for hot paths.
- Favor lock-free or fine-grained locking in concurrent sections.
- HTTP range prefetching: 4-way concurrency with $1\,\text{MiB}$ chunks; ordered merge; scheduler-driven throttling.
- Exploit range requests and chunked reads; tune block sizes.
- mmap usage to reduce copy overhead; buffered fallback with $128\,\text{KiB}$ when mmap not viable.
- SIMD-friendly decoding; avoid branches on critical paths.
- Batch rasterization when feasible; avoid redundant scaling.
- Nearest-neighbor scaling chosen for low CPU overhead during resize.
- Benchmark suite and profiling hooks aligned with [AGENTS.md](AGENTS.md).
- LRU-only eviction minimizes contention and bookkeeping cost.
- Count-based cap chosen for deterministic memory footprint across varying frame sizes in R8.
- Triangle strips: optional storage increases memory usage but enables efficient caller-side rasterization; configurable processing types balance performance trade-offs; see [docs/tasks/10-rasterization-polystreams.md](docs/tasks/10-rasterization-polystreams.md).

## Glossary

- ASVR: encrypted Alphastream versioned record format as per original libalphastream.
- ASVP: decoded plain Alphastream format used in this repo.
- Frame: a time-indexed unit of alpha stream data.
- Alpha bit mask: per-pixel alpha coverage mask used to derive final textures/bitmaps.

## Open Questions for Interview (TBD)

- HTTP client, TLS, retries, and timeout policy selection. -> reqwest with TLS defaults, 3 retries, 10s timeout.
- Exact bitmap pixel format and alignment for Vulkan/OpenGL upload. -> R8 unorm single-channel.
- Scaling policy (nearest vs bilinear) and quality/perf trade-offs. -> Nearest-neighbor.
- Global vs per-instance error handling semantics. -> per-instance last error.
- get_frame contract for “nothing” and interaction with last_error. -> Option return with last_error set.
- Cache capacity, eviction strategy, and prefetch window sizing. -> 512 frames cap, LRU eviction, 120 frame prefetch window.
- Scheduler back-pressure model and preemption rules. -> adaptive backpressure, get_frame preemption with 12ms timebox.
- Timebase selection and drift handling strategy. -> monotonic clock, drift detection/correction.
- Metadata surface: what is exposed and how versioning is represented. -> TBD.
- Benchmark metrics and target thresholds; representative datasets. -> TBD.

### Interview Checklist

- [ ] Agree on metadata schema and versioning exposure.
- [ ] Set performance targets and benchmarking methodology.