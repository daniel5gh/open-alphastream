# AlphaStream ASVR File Format (Encrypted)

This document describes the technical specification of the encrypted AlphaStream Vector Resource (ASVR) file used for alpha-mask reconstruction. The format is designed for efficient random access and compatibility with HTTP Range requests so clients can download and decrypt only the parts needed for a given frame.

References:
- Binary analysis: [libalphastream.so](libalphastream.so)
- Python reference implementation: [alpha_stream_crypto.py](alpha_stream_crypto.py), [alpha_stream_draw.py](alpha_stream_draw.py)

## Goals

- Indexable: Frame sizes are stored up-front to compute per-frame offsets without reading the entire file.
- Stream-friendly: Each frame is stored as an independent encrypted+compressed block, enabling partial downloads.
- Deterministic decryption: The nonce derives from a key ID so frames can be decrypted in isolation.

## Terminology

- ASVR: Alpha Stream Vector Resource file containing encrypted vector data for alpha masks.
- Frame: A time-indexed unit of vector data (polyline sets) used to draw a mask.
- Channel/Record: A single polyline payload within a frame (multiple per frame).

## Cryptography

- KDF: scrypt
  - Parameters: $N=16384$, $r=8$, $p=1$, output length $=32$ bytes
  - Salt construction (bytes):
    - 4 bytes little-endian scene_id
    - Full version string (ASCII)
    - Base URL substring (ASCII), e.g. `pov_mask.asvr`
  - Passphrase: 32-byte constant embedded in the binary (extracted from lib)
- Cipher: ChaCha20 stream cipher
  - Nonce (128-bit): 12 zero bytes + 4-byte little-endian key_id
  - No authentication tag (not AEAD)
- Key derivation reference: [alpha_stream_crypto.py](alpha_stream_crypto.py)

Inline derivation equation:
$\text{key} = \operatorname{scrypt}(\text{passphrase}, \text{salt(scene\_id||version||base\_url)}, N=16384, r=8, p=1)$

## File Layout Overview

The file is logically split into:

1. Header (16 bytes)
2. Sizes Table (zlib-compressed buffer)
3. Frame Blocks (concatenation of encrypted per-frame payloads)

```
[ 16-byte header ] [ zlib(Sizes Table) ] [ Frame 0 ] [ Frame 1 ] ... [ Frame N-1 ]
```

### 1) Header (16 bytes)

- Bytes 12..15 (inclusive) are a 32-bit little-endian integer specifying the size (in bytes) of the zlib-compressed Sizes Table.
- Other header fields are reserved/unknown for now; do not depend on them.

Example (Python): `compressed_data_size = int.from_bytes(header[12:16], 'little')`

### 2) Sizes Table (zlib-compressed)

- Immediately follows the 16-byte header.
- Decryption: The header + sizes table region is encrypted with ChaCha20 using key_id $= \texttt{0xFFFFFFFF}$.
- After decryption, the sizes table is a zlib stream that decompresses to a flat array of little-endian 64-bit unsigned integers (one per frame), representing the encrypted size of each frame block.
- Frame count $M$ equals the number of 8-byte entries in the decompressed sizes table.

Let $S[i]$ be the size (in bytes) of frame $i$ (encrypted block size as stored in the file).

Total sizes table equation:
$$\text{sizes\_raw} = \operatorname{zlib\_decompress}(\text{ciphertext}[16:16+\text{compressed\_data\_size}])$$

### 3) Frame Blocks

- The frame blocks begin at offset:
  - $\text{body\_base} = 16 + \text{compressed\_data\_size}$
- The start offset of frame $i$ equals:
  - $\text{frame\_offset}[i] = \text{body\_base} + \sum\limits_{k=0}^{i-1} S[k]$
- The length of frame $i$ equals $S[i]$.
- Decryption: Each frame block is encrypted independently and uses ChaCha20 with key_id $= i$.
  - Nonce: `00 00 00 00 00 00 00 00 00 00 00 00 || <i (u32 LE)>`

#### Frame Block Payload Structure (post-decryption)

After decrypting frame $i$, the plaintext is:

- Bytes 0..3: `expected_uncompressed_len` (uint32 LE)
- Bytes 4..end: zlib-compressed payload

The binary verifies that the zlib-decompressed length equals `expected_uncompressed_len`.

$$\operatorname{len}(\operatorname{zlib\_decompress}(\text{plaintext}[4:])) = \text{expected\_uncompressed\_len}$$

#### Decompressed Frame Payload Structure

Let $P$ be the decompressed frame payload:

- Bytes 0..3: `channel_count` (uint32 LE)
- Next `channel_count * 4` bytes: per-channel payload sizes (uint32 LE) — these are the record sizes in bytes
- Remaining bytes: concatenation of `channel_count` channel payloads in order

Per-channel payload encoding (polyline):
- Bytes 0..3: absolute point $(x_0, y_0)$ as two uint16 LE
- Remaining bytes: pairs of signed int8 $(\Delta x, \Delta y)$ deltas, accumulated sequentially

Point count equation for a channel payload of size $b$ bytes:
$$N = \frac{b}{2} - 1$$
The sequence of absolute points is:
$$\bigl(x_0, y_0\bigr),\ \bigl(x_0 + \Delta x_1,\ y_0 + \Delta y_1\bigr),\ \ldots$$

Notes:
- All integers are little-endian.
- Channel payloads may be as small as 6 bytes (two points: base + one delta).

## Drawing Model (Informative)

The binary:
- Converts each channel payload into a list of absolute points.
- Draws line segments between successive points using Bresenham with clipping to the output mask rectangle.
- Optionally performs a scanline analysis/update pass to fill interior regions.

Python mirror: see [alpha_stream_draw.py](alpha_stream_draw.py).

## Random Access and HTTP Range Compatibility

Because the sizes table provides per-frame encrypted sizes, a client can compute exact byte ranges for any frame without reading earlier frames.

Procedure to fetch and render frame $i$ from a remote ASVR via HTTP Range:
1. Download and decrypt the header + sizes table region using key_id $= \texttt{0xFFFFFFFF}$.
2. Decompress the sizes table to obtain $S[0..M-1]$.
3. Compute $\text{frame\_offset}[i]$ and request the HTTP range `[frame_offset[i], frame_offset[i] + S[i] - 1]`.
4. Decrypt that frame with key_id $= i$, then zlib-decompress and parse.
5. Rasterize the polylines into a mask.

This design avoids downloading the full file up front and supports seek operations efficiently.

## Error Handling

The binary emits diagnostics for:
- Zlib error codes returned by `uncompress()`.
- Mismatch: `expected_uncompressed_len` vs actual decompressed length.
- Payload size accounting (sum of per-channel sizes must equal remaining bytes of the frame payload).

Implementations should:
- Verify lengths before allocation.
- Treat malformed frames as errors and skip rendering.

## Versioning and Salts

Key derivation salt includes the full version string and a base URL substring, making keys version/resource-specific. If the provider changes the version or resource name, the derived key changes accordingly.

Salt bytes layout:
```
[ scene_id (u32 LE) ] [ version (ASCII) ] [ base_url (ASCII) ]
```

## Endianness and Alignment

- All integral fields are little-endian.
- No padding/alignment requirements are imposed in the on-disk format beyond the defined byte layouts.

## Minimal Parser Outline (Informative)

High-level steps for a reader:

1. Derive key:
   - scrypt(passphrase, salt(scene_id||version||base_url), N=16384, r=8, p=1, out=32)
2. Decrypt header+sizes region with key_id $= \texttt{0xFFFFFFFF}$.
3. Read `compressed_data_size` from header bytes 12..15.
4. Decompress sizes table to get `frame_sizes[ ]` (u64 LE).
5. For target frame `i`:
   - Compute `frame_offset[i]` by summing prior sizes.
   - HTTP range read the encrypted block.
   - Decrypt with key_id $= i$.
   - Verify `expected_uncompressed_len` and zlib-decompress.
   - Parse channels and rasterize.

## Security Considerations

- ChaCha20 is used without authentication (no Poly1305 tag). Implementations should consider integrity protections at a higher layer (e.g., HTTPS, checksums) if needed.
- The passphrase is a static constant embedded in the binary; secrecy relies on salt variability and distribution controls.

## Implementation Notes

- Reference decryption/parsing code is available in [alpha_stream_crypto.py](alpha_stream_crypto.py).
- Rasterization logic mirroring the binary’s approach is available in [alpha_stream_draw.py](alpha_stream_draw.py).

