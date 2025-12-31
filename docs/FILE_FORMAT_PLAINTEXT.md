# AlphaStream ASVR-PLAIN File Format (Unencrypted)

This document specifies a plaintext (non-encrypted) variant of the AlphaStream Vector Resource (ASVR) format for alpha-mask reconstruction. It preserves the same indexing properties as the encrypted format so readers can fetch and decode individual frames with HTTP Range requests without downloading the entire file.

References (implementation-oriented):
- Parser/decryption reference: [alpha_stream_crypto.py](alpha_stream_crypto.py)
- Rasterizer reference: [alpha_stream_draw.py](alpha_stream_draw.py)

## Goals

- Indexable: Per-frame sizes table placed up-front enables computing exact byte ranges for any frame.
- Stream-friendly: Frames are independent compressed blocks; a client can range-request just the selected frame.
- Backwards-compatible structure: Mirrors the encrypted ASVR layout, omitting only the encryption layer.

## Conventions

- Endianness: Little-endian for all integers.
- Units: Byte offsets and lengths unless stated otherwise.
- Compression: zlib (deflate) streams where specified.

## High-Level Layout

A plaintext ASVR file (ASVR-PLAIN) consists of:

1) Fixed-size header (16 bytes)
2) Sizes Table (zlib-compressed)
3) Concatenated Frame Blocks (plaintext; each contains a zlib-compressed payload)

```
[ 16-byte header ] [ zlib(Sizes Table) ] [ Frame 0 ] [ Frame 1 ] ... [ Frame N-1 ]
```

This layout is intentionally identical to the encrypted format, except that the header+table and frame blocks are not encrypted. As a result, the same indexing math applies in both formats.

## 1) Header (16 bytes)

- Bytes 0..11: Reserved/implementation-defined (may include magic, version). For this plaintext variant, writers SHOULD set a magic and version to aid recognition:
  - Bytes 0..3: ASCII "ASVP" (ASVR Plain)
  - Bytes 4..7: ASCII "PLN1" (plaintext v1)
  - Bytes 8..11: Number of Sizes Table entries (uint32 LE)
- Bytes 12..15: `compressed_data_size` (uint32 LE)
  - Number of bytes of the Sizes Table after zlib compression.

Example (pseudo):
- `compressed_data_size = int.from_bytes(header[12:16], 'little')`

## 2) Sizes Table (zlib-compressed)

Immediately follows the header and is exactly `compressed_data_size` bytes long.

- Decompression yields a flat array of 64-bit little-endian unsigned integers: one per frame.
- Let `frame_sizes[i]` denote the compressed length of Frame `i` in bytes (i.e., length of the frame block described below).
- Frame count `M` equals the number of entries in the decompressed table.

Mathematically:
$$\text{sizes\_raw} = \operatorname{zlib\_decompress}(\text{file}[16:16+\text{compressed\_data\_size}])$$
where `sizes_raw` is `M` consecutive `uint64_le` values.

## 3) Frame Blocks (plaintext)

The first frame block begins at:
$$\text{body\_base} = 16 + \text{compressed\_data\_size}$$

The offset of frame `i` is computed by summing prior sizes:
$$\text{frame\_offset}[i] = \text{body\_base} + \sum_{k=0}^{i-1} \text{frame\_sizes}[k]$$

The block length for frame `i` is:
$$\text{frame\_length}[i] = \text{frame\_sizes}[i]$$

Each frame block is structured as:

- Bytes 0..3: `expected_uncompressed_len` (uint32 LE)
- Bytes 4..end: zlib-compressed frame payload

Readers MUST verify that:
$$\operatorname{len}(\operatorname{zlib\_decompress}(\text{block}[4:])) = \text{expected\_uncompressed\_len}$$

### Decompressed Frame Payload Structure

Let `payload = zlib_decompress(block[4:])`. The layout is:

1) Bytes 0..3: `channel_count` (uint32 LE)
2) Next `channel_count * 4` bytes: per-channel payload sizes (uint32 LE)
3) Remaining bytes: concatenation of `channel_count` channel payloads, in order

Let `sizes[i]` be the i-th per-channel size. The concatenation region length MUST equal `sum(sizes)`.

#### Channel Payload Encoding (Polyline)

Each channel encodes a polyline as:

- Bytes 0..1: `x0` (uint16 LE)
- Bytes 2..3: `y0` (uint16 LE)
- Bytes 4..end: pairs of signed 8-bit deltas `(dx, dy)`

Absolute point sequence reconstruction:
- `P0 = (x0, y0)`
- `Pj = (x0 + Σ dx_k, y0 + Σ dy_k)` for j-th point, accumulating deltas in order

Point count for a channel payload of size `b` bytes:
$$N = \frac{b}{2} - 1$$

Notes:
- Minimal payload is 6 bytes → 2 points (base + one delta).
- Coordinates are 16-bit; deltas are 8-bit signed and accumulate across the polyline.

## Random Access and HTTP Range

The upfront Sizes Table enables precise random access to any frame:

1) Read the 16-byte header and the `compressed_data_size`.
2) Range-download exactly the sizes table bytes and zlib-decompress it to obtain `frame_sizes`.
3) Compute `frame_offset[i]` using the prefix sum of `frame_sizes`.
4) Range-download `[frame_offset[i], frame_offset[i] + frame_sizes[i] - 1]`.
5) Parse the downloaded block: verify `expected_uncompressed_len`, then zlib-decompress and parse channels.

This workflow avoids downloading the entire file, supporting efficient seeking and playback.

## Error Handling and Validation

- If `sum(per-channel sizes) != len(payload) - 4 - channel_count*4`, treat the frame as malformed.
- If zlib decompression fails or the decompressed length mismatches `expected_uncompressed_len`, treat the frame as malformed.
- Bounds checking is required when reconstructing coordinates and during rasterization (clip to mask rectangle).

## Versioning

- Writers SHOULD populate header bytes 0..7 with an ASCII magic and version (e.g., `ASVP` + `PLN1`).
- Readers SHOULD accept any content in header bytes 0..11, relying only on bytes 12..15 for `compressed_data_size` and on successful table decompression for format detection.

## Security Considerations

- This plaintext variant carries no cryptographic protection. Integrity should come from transport (e.g., HTTPS) or external checksums.
- If integrity is critical, a higher-level manifest or per-frame checksum sidecar is recommended.

## Minimal Reader Pseudocode (Plaintext)

```
read 16 bytes -> header
compressed_data_size = le32(header[12:16])
range-read sizes_table_enc = file[16 : 16+compressed_data_size]
sizes_raw = zlib.decompress(sizes_table_enc)
frame_sizes = parse as array of le64

body_base = 16 + compressed_data_size
frame_offset[i] = body_base + sum(frame_sizes[:i])
range-read block = file[frame_offset[i] : frame_offset[i] + frame_sizes[i]]
expected_len = le32(block[0:4])
payload = zlib.decompress(block[4:])
assert len(payload) == expected_len

channel_count = le32(payload[0:4])
sizes = [le32(payload[4+4*j : 8+4*j]) for j in range(channel_count)]
ptr = 4 + 4*channel_count
records = []
for size in sizes:
    records.append(payload[ptr:ptr+size])
    ptr += size

# Each record -> points:
# x0 = le16(rec[0:2]); y0 = le16(rec[2:4])
# for k in range(4, len(rec), 2): x += i8(rec[k]); y += i8(rec[k+1]); points.append((x,y))
```

## Interoperability With Encrypted Variant

- Layout and indexing math are identical. The only difference is the absence of ChaCha20 encryption and keying.
- A converter can produce ASVR-PLAIN by decrypting header+table and each frame block from the encrypted format and writing out the same structures without re-encryption.

## Rasterization (Informative)

Polylines are typically rendered by:
- Drawing line segments between successive points using Bresenham with clipping.
- Optionally running a scanline fill pass for interior coverage.

A reference Python rasterizer mirroring the binary is provided in [alpha_stream_draw.py](alpha_stream_draw.py).
