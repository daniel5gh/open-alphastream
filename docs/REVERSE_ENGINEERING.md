# Reverse Engineering Notes

Scope: Technical methods used to analyze and replicate the AlphaStream mask pipeline from [libalphastream.so](libalphastream.so). Includes decompilation workflow, runtime testing, and debugging stack with a `dlopen` harness and `qemu-aarch64` paired with `gdb-multiarch`.

Primary references:
- [alpha_stream_crypto.py](alpha_stream_crypto.py)
- [alpha_stream_draw.py](alpha_stream_draw.py)
- [FILE_FORMAT.md](FILE_FORMAT.md)
- [FILE_FORMAT_PLAINTEXT.md](FILE_FORMAT_PLAINTEXT.md)
- MCP dumps in RE/mcp-outputs: function prototypes and decompilations

## Objectives

- Identify frame parsing, crypto, and drawing pipeline in the binary.
- Derive a high-level Python implementation that reproduces the mask rendering from vector payloads.
- Document an indexable file format enabling HTTP Range downloads.

## Toolchain

- Static analysis: Ghidra for AArch64 ELF shared object.
- MCP server plugin for Ghidra to assist by LLM-aided decompilation.
- Dynamic loading: a C test harness using `dlopen`/`dlsym` to probe exported symbols and call entry points.
- Emulation + debug: `qemu-aarch64` user-mode with `gdb-multiarch` for remote debugging.
- Python: Decryption and rasterization mirrors.

## Key Findings

- Crypto
  - KDF: scrypt with parameters $N=16384$, $r=8$, $p=1$.
  - Salt: 4 bytes little-endian scene_id + ASCII version + ASCII base_url substring.
  - Passphrase: 32-byte constant embedded in the binary.
  - Cipher: ChaCha20 with 128-bit nonce; first 12 bytes zero, last 4 bytes is little-endian key_id.
- Container format
  - 16-byte header; bytes 12..15 store compressed sizes table length.
  - A zlib-compressed sizes table of 64-bit little-endian frame sizes.
  - Concatenated per-frame blocks; each block decrypts (or is plaintext), then holds `u32 expected_uncompressed_len` + zlib payload.
  - Decompressed per-frame payload: `u32 channel_count`, then `channel_count×u32` record sizes, followed by channel payloads.
- Vector payload encoding
  - Each channel: base absolute (x0,y0) as two `uint16_le` (4 bytes), then successive `(dx,dy)` as `int8` pairs.
  - Point count: $N = \frac{b}{2} - 1$ for payload byte size $b$.
- Rasterization
  - Scale points from the 2048×1024 source space to the mask size.
  - Draw connected segments via Bresenham line algorithm with clipping to mask rectangle.
  - Fill the edges of all channels together with one even-odd scanline pass (nested channels are holes). See [Rasterizer details and parity](#rasterizer-details-and-parity).

## Ghidra Workflow

1. Import [libalphastream.so](libalphastream.so) and analyze with default AArch64 settings.
2. Use strings and RTTI hints (see [RE/mcp-outputs/strings.txt](RE/mcp-outputs/strings.txt)) to locate candidate functions.
3. Decompile key routines (saved in [RE/mcp-outputs](RE/mcp-outputs)):
   - Processor orchestration (decryption, zlib, record build)
   - Segment drawing (Bresenham + clipping)
   - Finalization (range analysis and scanline update)
4. Identify core structures by data flow rather than names alone.
5. Use MCP server to iteratively refine decompilations with LLM assistance.

## Dynamic Loading Harness (test.c)

A minimalist loader probes the shared object and calls selected routines for smoke tests under emulation.

```c
#include <dlfcn.h>
#include <stdio.h>
#include <stdbool.h>

int main() {
    fprintf(stdout, "Starting dynamic library test...\n");
    void *handle = dlopen("libalphastream.so", RTLD_NOW);
    if (!handle) {
        fprintf(stderr, "Failed to load libalphastream.so: %s\n", dlerror());
        return 1;
    }

    typedef void* (*CV_create_t)();
    typedef const char* (*CV_get_name_t)(void*);
    typedef const char* (*CV_get_version_t)(void*);
    typedef bool (*CV_init_t)(
        void*, const char*, unsigned int, unsigned int, unsigned int,
        const char*, unsigned int, unsigned int, unsigned int, unsigned int, unsigned int, unsigned int);
    typedef const char* (*CV_get_last_error_text_t)(void*);
    typedef int (*CV_get_last_error_code_t)(void*);
    typedef void* (*CV_get_frame_t)(void*, unsigned long);
    typedef unsigned int (*CV_get_frame_size_t)(void*);
    typedef unsigned int (*CV_get_total_frames_t)(void*);
    typedef void (*CV_destroy_t)(void*);

    CV_create_t CV_create = (CV_create_t)dlsym(handle, "CV_create");
    CV_get_name_t CV_get_name = (CV_get_name_t)dlsym(handle, "CV_get_name");
    CV_get_version_t CV_get_version = (CV_get_version_t)dlsym(handle, "CV_get_version");
    CV_init_t CV_init = (CV_init_t)dlsym(handle, "CV_init");
    CV_get_last_error_text_t CV_get_last_error_text = (CV_get_last_error_text_t)dlsym(handle, "CV_get_last_error_text");
    CV_get_last_error_code_t CV_get_last_error_code = (CV_get_last_error_code_t)dlsym(handle, "CV_get_last_error_code");
    CV_get_frame_t CV_get_frame = (CV_get_frame_t)dlsym(handle, "CV_get_frame");
    CV_get_frame_size_t CV_get_frame_size = (CV_get_frame_size_t)dlsym(handle, "CV_get_frame_size");
    CV_get_total_frames_t CV_get_total_frames = (CV_get_total_frames_t)dlsym(handle, "CV_get_total_frames");
    CV_destroy_t CV_destroy = (CV_destroy_t)dlsym(handle, "CV_destroy");

    if (!CV_create || !CV_get_name || !CV_get_version || !CV_init || !CV_get_last_error_text ||
        !CV_get_last_error_code || !CV_get_frame || !CV_get_frame_size || !CV_get_total_frames || !CV_destroy) {
        fprintf(stderr, "Failed to load symbols: %s\n", dlerror());
        dlclose(handle);
        return 1;
    }

    void* alphaStream = CV_create();
    // url from deovr.com for scene 85342 as of 2025-12 - it ovbiously needs to be valid at runtime so update as needed
    bool ok = CV_init(
        alphaStream,
        "https://cdn-vr.deovr.com/passthrough/video/85342/pov_mask.asvr?Expires=1767302158&Signature=bWNpe%2BK5IZgDPKzcysXaUtCK%2B9I%3D&AWSAccessKeyId=ACF1E79A1D064B998FF2C62A58DF9C60",
        85342,              // sceneId
        6720,               // width
        3360,               // height
        "1.5.0",            // version
        0,                  // startFrame
        10,                 // l0BufferLength
        200,                // l1BufferLength
        50,                 // l1BufferInitLength
        4000,               // initTimeoutMs
        4000                // dataTimeoutMs
    );
    printf("CV_init returned: %s\n", ok ? "true" : "false");
    printf("Name: %s\n", CV_get_name(alphaStream));
    printf("Version: %s\n", CV_get_version(alphaStream));
    printf("Last error text: %s\n", CV_get_last_error_text(alphaStream));
    printf("Last error code: %d\n", CV_get_last_error_code(alphaStream));
    printf("Frame size: %u\n", CV_get_frame_size(alphaStream));
    printf("Total frames: %u\n", CV_get_total_frames(alphaStream));
    void* frame = CV_get_frame(alphaStream, 0);
    printf("Frame ptr: %p\n", frame);
    frame = CV_get_frame(alphaStream, 1000);
    printf("Last error text: %s\n", CV_get_last_error_text(alphaStream));
    printf("Last error code: %d\n", CV_get_last_error_code(alphaStream));
    int code = CV_get_last_error_code(alphaStream);
    
    while (code != 0)
    {
        printf("Waiting for valid frame...\n");
        frame = CV_get_frame(alphaStream, 1000);
        printf("Frame ptr: %p\n", frame);
        code = CV_get_last_error_code(alphaStream);
    }
    

    CV_destroy(alphaStream);

    dlclose(handle);
    return 0;
}
```

Notes:
- Exact prototypes may differ; the harness is for symbol resolution and breakpointing.
- Build for AArch64 to match the binary.

```bash
NDK=./android-ndk-r29

$NDK/toolchains/llvm/prebuilt/linux-x86_64/bin/aarch64-linux-android30-clang \
    -g \
    -o test_harness \
    test_harness.c \
    -L. \
    -lm \
    -ldl \
    -lz \
    -llog \
    -lssl \
    -lcrypto \
    -pthread
```
## Emulation + Remote Debugging

Run the harness under QEMU, exposing a GDB stub, then attach with `gdb-multiarch`:

```bash
export SYSROOT=$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/linux-x86_64/sysroot
# copy over dependencies to SYSROOT, we got them from a Pico headset and the application's APK.
# Start QEMU user-mode with GDB stub on port 1234
qemu-aarch64 -g 1234 -L $SYSROOT ./test_harness
```

```bash
# In another terminal: attach with gdb-multiarch
gdb-multiarch ./test_harness
```

```gdb
# Connect to QEMU stub
target remote :1234
set solib-absolute-prefix ./android-ndk-r29/toolchains/llvm/prebuilt/linux-x86_64/sysroot
# Set breakpoints at interesting functions
b main
# single-step until lib is loaded
info proc mappings
# find base address of libalphastream.so
set $libbase = 0xXXXXXX  # replace with actual base
# Set breakpoints in lib
b *($libbase + 0xYYYYYY)  # replace with function offset as found in Ghidra
```

Useful commands:
- Memory/bytes: `x/32xb $xN` for registers or `x/64xb addr` for memory regions.
- Inspect arguments/registers per AArch64 ABI (x0..x7, w0..w7).
- Verify zlib lengths and buffer regions by stepping through `uncompress` call sites.

## Python Cross-Verification

- Decrypt header and sizes table with key_id `0xFFFFFFFF`.
- Decrypt frames with key_id = frame index.
- Confirm zlib payload lengths.
- Rasterize vectors and compare behavior with binary.

```python
from pathlib import Path
from alpha_stream_crypto import AlphaStream
from alpha_stream_draw import save_frame_png

asvr = AlphaStream(Path("85342/pov_mask-trailer.asvr"), 85342, b"1.5.0", b"pov_mask.asvr")
save_frame_png(asvr, frame_index=1000, width=1024, height=1024, out_path="85342/mask.png", fill=True)
```

## Library Version Differences

Comparison of the shared objects in `test_data/lib-alphastream-versions/` (all AArch64, stripped). Derived from dynamic symbol tables (`llvm-nm -D`), `DT_NEEDED` entries, string diffs, and annotated `llvm-objdump` disassembly — not from decompilation, so code-level findings are marked where they are inferred.

### Identity

| File | `CV_get_version()` | NDK / clang | Android API |
|---|---|---|---|
| `libalphastream-14.3.2572-20250902.so` | `1.5.4` | r25c / 14.0.7 | 26 |
| `libalphastream-Hyper-43404-0.0.1.so` | `1.7.0` | r27c / 18.0.3 | 32 |
| `libalphastream-15.3.3563-20251229-.so` | `0.0.0` (dev build, verbose logging) | r27c / 18.0.3 | 32 |
| `libalphastream-15.6.3736-20260508.so` | `1.11.0` | r27c / 18.0.3 | 32 |

`CV_get_name()` returns `AlphaStream` in all versions. Hyper is architecturally the 14.3 code base rebuilt with the newer toolchain.

### Exported symbols

- All versions export the same 10 functions: `CV_create`, `CV_destroy`, `CV_init`, `CV_get_frame`, `CV_get_frame_size`, `CV_get_total_frames`, `CV_get_name`, `CV_get_version`, `CV_get_last_error_code`, `CV_get_last_error_text`.
- Hyper additionally leaks RTTI (`_ZTI/_ZTS/_ZTV`) for `shared_ptr<pair<int, string>>`; not an API addition.
- The exports are thin wrappers/thunks into internal implementations; in 15.x and Hyper they are pure tail calls.

### Imports and linked libraries

| | 14.3 | Hyper | 15.3 | 15.6 |
|---|---|---|---|---|
| crypto lib | `libcrypto_3.so` | `libcrypto_3.so` | `libcrypto_3.so` | **`libcrypto.so`** |
| `libssl_3.so` | yes | no | no | no |
| sync primitives | `condition_variable` (`system_clock` timed wait) | `condition_variable` (`pthread_cond_clockwait`) | atomic wait/notify, `sched_yield`, `sleep_for` | same as 15.3 |
| aligned `new`/`delete` | no | no | yes | yes |

Other import changes: 15.x drops libc++ `basic_string::find/rfind/append/assign` and adds `memchr`; 15.3 alone imports `to_string(unsigned)` (debug logging). `getauxval`, `__system_property_get` and the `"ro.arch"`/`"exynos9810"` strings in 14.3/Hyper are compiler-rt outline-atomics LSE detection (toolchain), not library logic.

### `CV_init` signature changes

Parameter order as used by the harness above: `handle, url, sceneId, width, height, version, startFrame, l0BufferLength, l1BufferLength, l1BufferInitLength, initTimeoutMs, dataTimeoutMs`.

- **14.3 / Hyper**: 12 arguments, all forwarded. Validation: `ap: l0/l1/l1i is zero`, `ap: l0 > l1i`, `ap: l1i > l1`, `ap: l1/l1i !/ l0` (divisibility), `ap: it/dt is zero`, plus url/sceneId/size checks.
- **15.3**: still 12 arguments, but the thunk discards `l0`, `l1`, `l1i` and passes the constants `l0 = 20`, `l1 = 600`; only the two timeouts are forwarded.
- **15.6**: `l1BufferInitLength` is removed — 11 arguments: `(…, startFrame, l0, l1, initTimeoutMs, dataTimeoutMs)`. Confirmed by Ghidra decompilation of the internal init (`FUN_00112fc8`, 11 params) and validator (`FUN_001136a8`). New checks: `ap: l0/l1 not even` (both must be even) and `ap: l1 !/ l0` (`l1 % l0 == 0`). Calling it with the 12-argument harness shifts arguments: `l1i` becomes `initTimeoutMs` and `initTimeoutMs` becomes `dataTimeoutMs`. Not yet exercised at runtime.
- **Buffer sizing in 14.3** (decompiled `FUN_0010f370`): L0 holds `l0` decoded frames (`l0 × frameBytes`); L1 holds `l1` compressed frames with `l1i` initial fill and a byte budget of `l1 << 11` (2 KiB per frame). Re-init with the same url, sceneId and version skips the header download and only resizes and restarts buffering.
- `ap: version is empty` check exists in Hyper and 15.x, not in 14.3.

### Download / buffering engine

- **14.3 / Hyper**: classes `S3Header`, `S3Body`/`S3BodyImpl`, `FramesBuffer`. Multi-range body requests with consistency checks (`s3b: different urls`, `different ranges`, `different intervals`, `invalid frame index`, `invalid range`, `wrong data size`) and `ap: frame is broken`. Producer/consumer via mutex + condition variables.
- **15.x**: classes `S3Header`, `Worker`. One worker thread streams byte ranges into L0/L1 buffers; 15.3's debug logs describe it (`Start downloading bytes`, `CurlToL1`, `restartDownloading`, `startDownloadingNext`, `Cancel outdated data`, `cacheInvalid=`, `lastAskedFrame=`, `curlEndFrameIndex=`). The worker reads atomic head/tail indices (lock-free ring buffer) and sleeps 5 ms (`0x4c4b40` ns) when the buffer is sufficiently full. 15.6 is the same design with the debug logging removed.

### Network

libcurl options are identical across versions (`URL`, `RANGE`, `TIMEOUT_MS`, `SSL_VERIFYPEER`, `SSL_VERIFYHOST`, `CAPATH`, `ERRORBUFFER`, `WRITEFUNCTION`/`WRITEDATA`); 15.x adds `TCP_NODELAY`.

### Unchanged across all versions

- scrypt KDF with `N=16384`, `r=8`, `p=1`.
- The 32-byte passphrase (`90379b41bbfd519d7fa68eebac34c97a12af6e3bcd23188a5a53648f72b47271`), byte-identical in all four and matching [alpha_stream_crypto.py](../python/alpha_stream_crypto.py).
- ChaCha20 via `EVP_DecryptInit_ex`; two `uncompress` call sites; identical `s3h:` header error strings.

The encrypted ASVR container and crypto are therefore expected to be compatible across these versions.

### Frame decode and rasterizer (14.3 vs 15.6)

Decompiled with REA + Ghidra 12.1.4 (14.3: decode `FUN_0010b580`, raster `FUN_0010705c` → `FUN_00107148` + `FUN_0010747c`; 15.6: decode `FUN_0010f494`, raster `FUN_0010a8b4` → `FUN_0010a9b0` + `FUN_0010ad38`). The logic is the same in both; only codegen differs (newer libc++ vector code, NEON-vectorized bounding box, introsort).

- Per frame: decrypt, then `u32 expected_len` + zlib (`p4 zlib:` / `p4: ud != exp`), then `u32 channel_count`, `channel_count × u32` record sizes, then per channel a `u16,u16` base point followed by `i8,i8` deltas. Point count is `(size >> 1) - 1`, matching [FILE_FORMAT.md](FILE_FORMAT.md).
- Points are scaled by `(outW / srcW, outH / srcH)` in `double`, truncated to integers.
- Per polygon (including the closing edge last → first): draw the outline with clipping, and build an edge list in 16.16 fixed point (`x << 16 | 0x8000`, slope `dx/dy`).
- Edges are sorted, then filled with an even-odd active-edge scanline fill.
- Output is a **1-bit-per-pixel, MSB-first packed** bitmap (row offset `y * width / 8`); the bit-mask table `ff7f3f1f0f070301 80c0e0f0f8fcfeff` appears exactly once in all four versions.

### Rasterizer details and parity

Details of the 14.3 functions (`FUN_0010705c` and callees), needed for a bit-exact port:

- **Source space is 2048×1024.** The decoder object's constructor (`0x10b420`) stores the constant pair `2048, 1024` (`.rodata`, file offset `0x4898` in 14.3; present once in every version) at `+0x34/+0x38`, and the decode computes `scale = out / src` in `double`. Real data reaches x = 2047. Each point becomes `(long)(scale * x)` (`FUN_0010c03c`).
- **One edge table for all channels.** `FUN_0010705c` calls the per-polygon edge builder (`FUN_00107148`) for every channel into the same edge vector, then fills once (`FUN_0010747c`). Even-odd parity therefore spans channels: a channel inside another is a hole. In 85342, 66 of 150 sampled frames contain such nested channels, and the hole is always fully contained and wound opposite to its outer channel.
- **Outline:** for each segment (starting with the closing edge last → first), `FUN_00107af0` draws a midpoint line. It always draws left to right (swapping endpoints when dx < 0), takes x as the major axis when `dx >= |dy|`, starts with `d = major - 2·minor`, and steps the minor axis only while `d < 0`. Polygons with one or two points still draw their outline (a dot or a line).
- **Line clipping** (`FUN_00107918`) is Cohen-Sutherland in a fixed order: outcodes `1=x<0, 2=x>=w, 4=y<0, 8=y>=h`. It rejects if the codes share a bit. Otherwise it moves each endpoint onto the top/bottom border, recomputes x-only codes, rejects again if they share a bit, then moves endpoints onto the left/right border. Each move is `a + (double)n1 * (double)n2 / (double)d`, truncated. The second endpoint uses the already-moved first one.
- **Edges** (32 bytes: `i32 ymin, i32 ymax, i64 x, i64 slope, next*`): when both endpoints are inside, `x = X << 16 | 0x8000` and `slope = (x1 - x0) / (y1 - y0)` (C division, truncating). When an endpoint is outside, the builder calls the clipper, **ignores its accept flag**, and, if the clipped y values differ, uses the clipped endpoints shifted `<< 16` *without* the 0x8000 bias, then extrapolates back to the original `ymin`. Horizontal edges are skipped.
- **Fill:** edges are sorted by `(ymin, x, slope)` with a sentinel appended. Rows run from the smallest `ymin` up to `min(max ymax, height)`. An edge is active for `ymin <= y < ymax`, and the active list is bubble-sorted by x after each step. Pairs fill pixels `x0 >> 16 ..= x1 >> 16` (clamped, skipped if entirely outside) using the start/end byte masks and `memset(0xff)` between them. Nothing is drawn for rows `< 0`, but edges still step through them.

**Parity check.** The native rasterizer was run under [Unicorn](https://www.unicorn-engine.org/) (AArch64). The method was:

- map the `.so` `PT_LOAD` segments, and set `TPIDR_EL0` so the stack canary works;
- replace the PLT stubs for `operator new/delete`, `memcpy`, `memset` and `__stack_chk_fail` with Python hooks;
- build the `vector<vector<{i64 x, i64 y}>>` input in emulated memory, and call `0x705c(buf, w, h, &polys)` with already-scaled points.

The Rust rasterizer (`rasterize_channels`) and the Python reference (`rasterize_channels`) are both bit-exact against it:

| Input | Output sizes | Differing pixels |
|---|---|---|
| 200 frames of 85342 | 512×256, 1000×500, 1920×1080, 2048×1024, 4096×2048 | 0 |
| 150 frames of a second scene | 1920×1080, 2048×1024 | 0 |
| 700 random frames, polygons crossing every border | 64×32 to 2048×1024 | 0 |

The previous Rust rasterizer (2024-wide source, per-channel fill ORed together, clamped lines) differed on about 22% of set pixels. It was also about 25–29× slower: 6.7 ms vs 0.23 ms per frame at 2048×1024, and 25 ms vs 0.96 ms at 4096×2048.

### Insights from the 15.3 dev build

15.3 (`CV_get_version()` = `0.0.0`) is 15.6's architecture with verbose `ANDROID_LOG_VERBOSE` tracing. The log sites name the worker's state machine. Decompiled functions (15.3 addresses):

| Function | Log | Behaviour |
|---|---|---|
| `FUN_0010f128` | — | Worker loop. Each tick runs the steps below in priority order and stops at the first that does work; sleeps 5 ms if none did. |
| `FUN_0010f228` | `curl: <err>` | On a finished transfer with an error: log, reset curl, clear buffer. |
| `FUN_0010f390` | `Cancel outdated data cacheInvalid= lastAskedFrame= curlEndFrameIndex=` | If `cacheInvalid` or `lastAsked >= curlEnd`: abort the transfer, flush L1, restart at `lastAsked`. |
| `FUN_0010f9c4` | `CurlToL1 N of M` | Carve complete frames out of the curl buffer (sizes from the header table) into the L1 ring; frames `< lastAsked` are dropped. |
| `FUN_0010fee8` | `Start downloading bytes N from S to E frames` | One HTTP Range request for frames `[S, min(S + freeL1Slots, total))`, byte range from the header's offset table. |
| `FUN_001105c8` | `restartDownloading` | Resume an incomplete range at `max(curlPos, lastAsked)`. |
| `FUN_001106e4` | `startDownloadingNext` | When idle, the previous range is complete and L1 is at most half full, continue from `max(curlEnd, lastAsked)`. |
| `FUN_0011085c` | `L1ToL0 N of M` | Decode L1 entries (decrypt + decode + rasterize) into the L0 ring while L0 has room; entries `< lastAsked` are dropped. |
| `FUN_0010ece8` | `getFrame N` | Called by `CV_get_frame`; see below. |
| `FUN_00109ec8` | `curl write N` | libcurl write callback; appends to the transfer buffer. |

`CV_get_frame` (`FUN_0010ece8`) semantics:

- The same frame as last time returns the held buffer (no work).
- If `n < lastAsked`, sets `cacheInvalid` (backward seek). Then sets `lastAsked = n`.
- Pops the L0 ring FIFO, **destroying** every entry before `n`, until it finds `n`. If L0 runs empty it returns `NULL` with error **1004 `ap: no data`** straight away; it does not block or wait for a timeout.
- `n >= totalFrames` gives error **1001 `ap: frame out of range`**.

What this adds to or corrects in our understanding:

- **Pipeline is two FIFO rings, not a random-access cache.** L1 holds compressed frames (capacity `l1`, 600 in 15.3), L0 holds decoded bitmaps (capacity `l0`, 20 in 15.3). Requesting a frame consumes everything before it; going back means a cancelled transfer and a re-download. [ADR-0011](adr/0011-ring-buffer-frame-cache.md)'s play-head and seek model matches this. The original treats a forward jump past the *in-flight download end* as a seek, where ours uses a buffer-size multiple.
- **No timebox.** The original `CV_get_frame` is non-blocking in both 14.3 and 15.x and and returns 1004 when the frame is not decoded yet. The 12 ms escalation in [RUST_IMPLEMENTATION.md](RUST_IMPLEMENTATION.md) is our own design, not a port.
- **Downloads are one large sequential Range request per batch** (up to the free L1 slots, i.e. hundreds of frames), streamed and split by frame as bytes arrive. They are not per-frame or fixed 1 MiB chunks. The next batch starts once L1 drains to half full.
- **Decoding happens on the worker thread**, ahead of playback, in L0-sized batches; `CV_get_frame` only hands out a pointer.
- **Error codes:** 1001 invalid argument / out of range, 1004 not ready (`ap: no data`), 1201 (`0x4b1`) frame decode failure (`p4:` errors), and 1 for `ap: frame is broken` (14.3).

### `version` and URL handling in `CV_init` (14.3)

- **`version` is not validated against a list.** `FUN_0010f370` passes `(sceneId, version, basename(url))` to the crypto object's vtable slot `+0x10`. That slot points straight at the KDF `FUN_0010c350`, which only concatenates `u32le(sceneId) || version || basename` as the scrypt salt. Any string is accepted; a wrong one just yields a wrong key. The only check is `ap: version is empty` (Hyper and 15.x).
- **Base name rule** (`FUN_001095f4`): substring after the last `/` up to the first `?` (or end of string). Query strings such as S3 signatures are therefore not part of the key.
- The single crypto class in every version, including the January 2024 build, is `CryptoV4` / `HeaderV4` (RTTI). There is no other format version to select.

### Decrypted header layout

Confirmed on all test files (1.0.0 and 1.5.0) after decrypting with key_id `0xFFFFFFFF`:

| Offset | Type | Value |
|---|---|---|
| 0 | u64 LE | `4` (matches `CryptoV4` / `HeaderV4`) |
| 8 | u32 LE | frame count (equals the decompressed sizes-table length / 8) |
| 12 | u32 LE | compressed sizes-table length |
| 16 | — | zlib stream (`78 9c`) |

The fixed 8-byte prefix plus the zlib magic is about 80 bits of known plaintext, so an 18-byte Range request is enough to test whether a `(sceneId, version, basename)` guess is correct.

### Recovering unknown key inputs

Because the header test needs only 18 bytes and one scrypt (~75 ms on one core, ~235 guesses/s on 32 cores with OpenSSL via Python), unknown salt parts can be searched:

- Scene IDs 1–100 000 for one `(version, basename)` pair: about 7 minutes on 32 cores. The cost scales linearly with the number of version strings tried.
- Verified: the 85342 file is found in a search of 85000–86000 in 8 s.
- Real find: older full-length files (`pov_mask.asvr` and `no_pov_mask.asvr` of a March 2024 scene) do not use the `1.0.0` from the API metadata. They decrypt with **`version = "0.2.3"`**, their usual scene ID, base name as usual. They were found by trying `0.a.b` strings for the known ID. So older content can carry version strings that the current API no longer reports.

The Rust crate implements this as `libalphastream::keysearch` (`find_key_inputs`, `try_key`, `base_name_from_url`) plus a CLI (~470 guesses/s on 32 cores in release builds). For a URL it fetches only the first 18 bytes:

```
cargo run --release --bin keysearch -- <asvr_path_or_url> [--ids 1-200000] [--versions 1.5.0,1.0.0,0.2.3] [--sweep-versions 1.9.9] [--base-name pov_mask.asvr] [--threads N]
```

### `CV_get_frame` in 14.3 does not block

`FUN_0011006c` → `FUN_0010d158`. L0 is a double-buffered window of `l0` decoded frames rather than a ring:

- Frame inside the front window: return the pointer without taking a lock.
- Frame inside the back window (when ready): swap the windows, ask the worker for the next window `[end, end + l0)`, notify, and return the frame.
- Otherwise: ask the worker for window `[n, n + l0)`, notify, and return `NULL` with **1004 `ap: no data`** immediately.

The condition-variable waits are on the worker side only. Per-frame failures are kept in a hash map (`FUN_0011026c`) and re-reported on later requests for that frame, with code 1 `ap: frame is broken`. Unlike 15.x, earlier frames inside the current window stay random-access.

## Pitfalls and Notes

- Naming in the binary may be misleading as we built those up over time in Ghidra; prefer usage-driven semantics over identifiers.
- Beware of dynamic buffer reallocation patterns around 0x20-byte structs; ensure prefix sums and capacity checks mirror the binary.
- Non-authenticated ChaCha20 means plaintext integrity relies on transport (e.g., HTTPS) or external checksums.
- vtables and RTTI can help identify class hierarchies but may not reflect logical data structures directly.
- Needed to add a `/etc/hosts` entry in the SYSROOT to resolve `cdn-vr.deovr.com` when running under QEMU.

## Artifacts

- Shared object: [libalphastream.so](libalphastream.so) - extracted from the target application.
- Decompilation outputs: [RE/mcp-outputs](RE/mcp-outputs)
- File format specs: [FILE_FORMAT.md](FILE_FORMAT.md), [FILE_FORMAT_PLAINTEXT.md](FILE_FORMAT_PLAINTEXT.md)
- Python implementations: [alpha_stream_crypto.py](alpha_stream_crypto.py), [alpha_stream_draw.py](alpha_stream_draw.py)
- Harness source: `test.c` (described above)

## Vibe Reverse Engineering

Used a MCP server inside Ghidra so that an LLM could assist with function decompilation and analysis allowing us to rapidly iterate
on understanding complex routines. This greatly accelerated understanding of complex routines.
