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
  - Draw connected segments via Bresenham line algorithm with clipping to mask rectangle.
  - Post-process scanlines to fill interior coverage.

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
