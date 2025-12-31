"""decrypt.py

Convert an encrypted ASVR file to plaintext ASVR-PLAIN per FILE_FORMAT_PLAINTEXT.md.

- Derives key from (scene_id, version, base_url)
- Decrypts header + sizes table using key_id = 0xFFFFFFFF
- Decrypts each frame block using key_id = frame_index
- Writes plaintext file with the same structure but without encryption:
  [ 16-byte header | zlib(sizes table) | plaintext frame blocks ]

Usage:
  python decrypt.py --scene-id 85342 --version 1.5.0 --base-url pov_mask.asvr \
                    --input 85342/pov_mask.asvr \
                    --output 85342/pov_mask.asvp

See: FILE_FORMAT_PLAINTEXT.md
"""
from __future__ import annotations

import argparse
import struct
import zlib
from pathlib import Path
from typing import List, Tuple

from alpha_stream_crypto import derive_key_from_salts, decrypt

HEADER_SIZE = 16
MAGIC = b"ASVP"  # ASVR Plain
VERSION = b"PLN1"  # plaintext v1


def _read_file(path: Path) -> bytes:
    with open(path, "rb") as f:
        return f.read()


def _write_file(path: Path, data: bytes) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    with open(path, "wb") as f:
        f.write(data)


def _parse_sizes_table_zlib(raw: bytes) -> List[int]:
    """
    Decompress sizes table and return list of u64 little-endian sizes.
    """
    decompressed = zlib.decompress(raw)
    if len(decompressed) % 8 != 0:
        raise ValueError("Sizes table decompressed length is not a multiple of 8")
    sizes: List[int] = []
    for i in range(0, len(decompressed), 8):
        sizes.append(int.from_bytes(decompressed[i:i+8], byteorder="little", signed=False))
    return sizes


def _decrypt_header_and_sizes(encrypted_file: bytes, key: bytes) -> Tuple[int, bytes]:
    """
    Decrypt entire file with key_id FFFFFFFF and extract:
    - compressed_data_size from bytes 12..15
    - compressed sizes table bytes [16 : 16+compressed_data_size]
    """
    # Decrypt all to avoid keystream alignment issues
    dec = decrypt(encrypted_file, key, 0xFFFFFFFF)
    if len(dec) < HEADER_SIZE:
        raise ValueError("File too small for header")
    compressed_data_size = int.from_bytes(dec[12:16], byteorder="little", signed=False)
    end = HEADER_SIZE + compressed_data_size
    if len(dec) < end:
        raise ValueError("Decrypted data shorter than header+sizes table")
    sizes_table_compressed = dec[HEADER_SIZE:end]
    return compressed_data_size, sizes_table_compressed


def _decrypt_frame_blocks(encrypted_file: bytes, key: bytes, sizes: List[int], body_base: int) -> List[bytes]:
    """
    For each frame i:
    - Slice encrypted block from input
    - Decrypt with key_id = i
    - Validate structure (zlib decompress length matches expected)
    - Return plaintext blocks to write out
    """
    out_blocks: List[bytes] = []
    offset = body_base
    for i, size in enumerate(sizes):
        start = offset
        end = start + size
        if end > len(encrypted_file):
            raise ValueError(f"Frame {i}: encrypted range exceeds file length")
        enc_block = encrypted_file[start:end]
        plain_block = decrypt(enc_block, key, i) # plaintext, but still compressed
        if len(plain_block) < 4:
            raise ValueError(f"Frame {i}: plaintext block too short")
        expected_uncompressed_len = int.from_bytes(plain_block[0:4], byteorder="little", signed=False)
        try:
            payload = zlib.decompress(plain_block[4:])
        except zlib.error as ex:
            raise ValueError(f"Frame {i}: zlib decompress failed: {ex}")
        if len(payload) != expected_uncompressed_len:
            raise ValueError(
                f"Frame {i}: decompressed length {len(payload)} != expected {expected_uncompressed_len}"
            )
        out_blocks.append(plain_block)
        offset = end
    return out_blocks


def _build_plain_header(compressed_data_size: int, num_sizes_entries: int) -> bytes:
    """
    Construct 16-byte plaintext header per FILE_FORMAT_PLAINTEXT.md.
    Layout:
      0..3  MAGIC (ASVP)
      4..7  VERSION (PLN1)
      8..11 number of Sizes Table entries (uint32 LE)
      12..15 compressed_data_size (u32 LE)
    """
    return MAGIC + VERSION + num_sizes_entries.to_bytes(4, "little", signed=False) + compressed_data_size.to_bytes(4, "little", signed=False)


def convert_to_plain(scene_id: int, version: str, base_url: str, input_path: Path, output_path: Path) -> None:
    encrypted_file = _read_file(input_path)
    key = derive_key_from_salts(scene_id, version.encode("utf-8"), base_url.encode("utf-8"))

    compressed_data_size, sizes_table_compressed = _decrypt_header_and_sizes(encrypted_file, key)
    sizes = _parse_sizes_table_zlib(sizes_table_compressed)

    body_base = HEADER_SIZE + compressed_data_size
    frame_blocks = _decrypt_frame_blocks(encrypted_file, key, sizes, body_base)

    # Assemble plaintext file
    header = _build_plain_header(compressed_data_size, len(sizes))
    out = bytearray()
    out += header
    out += sizes_table_compressed
    for blk in frame_blocks:
        out += blk

    _write_file(output_path, bytes(out))

    # Summary
    total_frames = len(sizes)
    total_body = sum(sizes)
    print(
        f"Wrote plaintext ASVR: {output_path} | frames={total_frames} | sizes_table={compressed_data_size} bytes | body={total_body} bytes"
    )


def _parse_args() -> argparse.Namespace:
    p = argparse.ArgumentParser(description="Decrypt ASVR to plaintext ASVR-PLAIN")
    p.add_argument("--scene-id", type=int, required=True, help="Scene ID (uint32)")
    p.add_argument("--version", type=str, required=True, help="Version string, e.g. 1.5.0")
    p.add_argument("--base-url", type=str, required=True, help="Base URL substring, e.g. pov_mask.asvr")
    p.add_argument("--input", type=str, required=True, help="Path to encrypted ASVR file")
    p.add_argument(
        "--output",
        type=str,
        required=False,
        help="Output plaintext file path (default: input basename + '.asvp')",
    )
    return p.parse_args()


def main() -> None:
    args = _parse_args()
    input_path = Path(args.input)
    if args.output:
        output_path = Path(args.output)
    else:
        output_path = input_path.with_name(input_path.stem + ".asvp")
    convert_to_plain(args.scene_id, args.version, args.base_url, input_path, output_path)


if __name__ == "__main__":
    main()
