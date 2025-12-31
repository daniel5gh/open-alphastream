"""alpha_stream_crypto.py

This module implements the cryptographic core of the AlphaStream binary.
It reproduces the scrypt key derivation and ChaCha20 decryption used by
``libalphastream.so``.

The binary uses a 32‑byte constant passphrase (``DAT_001048ae``).  The
value was extracted from the binary and embedded here.

The public API:

* ``derive_key_from_salts(salt1: bytes, salt2: bytes) -> bytes`` –
  Derives a 32‑byte key from the two salts.
* ``decrypt_frame(data: bytes, key: bytes, iv: bytes) -> bytes`` –
  Decrypts a single frame using ChaCha20.

The module depends on the ``cryptography`` package.
"""

from __future__ import annotations
from pathlib import Path
import zlib

from cryptography.hazmat.primitives.kdf.scrypt import Scrypt
from cryptography.hazmat.primitives.ciphers import Cipher, algorithms
from cryptography.hazmat.backends import default_backend

# 32‑byte constant passphrase extracted from the binary
PASS_PHRASE = (
    b"\x90\x37\x9B\x41\xBB\xFD\x51\x9D"
    b"\x7F\xA6\x8E\xEB\xAC\x34\xC9\x7A"
    b"\x12\xAF\x6E\x3B\xCD\x23\x18\x8A"
    b"\x5A\x53\x64\x8F\x72\xB4\x72\x71"
)


PASS_LEN = 0x20

# Scrypt parameters used by the binary
SCRYPT_N = 0x4000  # 16384
SCRYPT_R = 8
SCRYPT_P = 1
KEY_LEN = 32  # 32‑byte key


def derive_key_from_salts(scene_id: int, version: bytes, base_url: bytes) -> bytes:
    """Derive a 32‑byte key from scene_id, version, and base_url substring.

    Parameters
    ----------
    scene_id: int
        Scene ID (uint32, as used in DeoVR streams).
    version: bytes
        Version string (full, as used in the stream).
    base_url: bytes
        Substring after last '/' or '?' in the version string (matches binary logic).

    Returns
    -------
    bytes
        The derived key.

    Notes
    -----
    The binary prepends the raw 4 bytes of scene_id (little-endian uint32) to the salt buffer,
    followed by version and base_url substring. This matches libalphastream.so's logic.
    """
    scene_id_bytes = scene_id.to_bytes(4, byteorder="little", signed=False)
    print(f"scene_id_bytes: {scene_id_bytes.hex()}")
    combined_salt = scene_id_bytes + version + base_url
    print(f"combined_salt: {combined_salt}")
    kdf = Scrypt(
        salt=combined_salt,
        length=KEY_LEN,
        n=SCRYPT_N,
        r=SCRYPT_R,
        p=SCRYPT_P,
        backend=default_backend(),
    )
    return kdf.derive(PASS_PHRASE)


def decrypt(data: bytes, key: bytes, key_id: int) -> bytes:
    """Decrypt a frame using ChaCha20-Poly1305.

    Parameters
    ----------
    data: bytes
        Ciphertext of the frame.
    key: bytes
        32‑byte key derived by :func:`derive_key_from_salts`.
    iv: bytes
        16‑byte IV (nonce) used by the binary.

    Returns
    -------
    bytes
        Plaintext frame data.
    """

    iv = b"\x00" * 12 + key_id.to_bytes(4, byteorder="little", signed=False)


    cipher = Cipher(
        algorithms.ChaCha20(key, iv),
        mode=None,
        backend=default_backend(),
    )
    decryptor = cipher.decryptor()
    return decryptor.update(data) + decryptor.finalize()

def is_zlib_compressed(data: bytes) -> bool:
    """Check if the given data is zlib-compressed.

    Returns True if the data starts with the standard zlib header (0x78 0x01/0x9C/0xDA).
    """
    return len(data) >= 2 and data[0] == 0x78 and data[1] in (0x01, 0x9C, 0xDA)

def parse_block(data: bytes) -> tuple[bytes, bytes, int]:
    """Parse a block of data

    The first 16 bytes are uncompress header, and the rest is zlib-compressed payload.

    Parameters
    ----------
    data: bytes
        The block of data to parse.

    Returns
    -------
    tuple[bytes, bytes]
        A tuple containing the header and payload.
    """
    header = data[:16]
    # header is 16 bytes, payload is rest (8 bytes for magic, 4 bytes for number of frames in this block, 4 bytes for next block offset)
    next_block = data[12:16]
    next_block_offset = int.from_bytes(next_block, byteorder='little', signed=False)
    payload = data[16:]
    # decompress payload
    if not is_zlib_compressed(payload):
        raise ValueError("Payload is not zlib-compressed")
    payload = zlib.decompress(payload)
    return header, payload, next_block_offset

class AlphaStream():
    HEADER_SIZE = 16
    DUMP_DEBUG = True

    header_raw: bytes
    sizes_raw: bytes
    compressed_data_size: int
    decrypted_data: bytes
    
    file_path: Path
    scene_id: int
    version: bytes
    base_url: bytes

    key: bytes

    frame_offsets: list[int]
    frame_sizes: list[int]

    def __init__(self, file_path: Path, scene_id: int, version: bytes, base_url: bytes):
        self.file_path = file_path
        self.scene_id = scene_id
        self.version = version
        self.base_url = base_url

        with open(file_path, "rb") as f:
            data = f.read()

        # decrypt the file
        self.key = derive_key_from_salts(scene_id, version, base_url)
        # iv appears hardcoded to 12 bytes of 0 + 4 bytes of 0xFFFFFFFF
        # self.iv = b"\x00" * 12 + (0xFFFFFFFF).to_bytes(4, byteorder="little", signed=False)
        self.decrypted_data = decrypt(data, self.key, 0xFFFFFFFF)
        print(f"Decrypted ASVR file size: {len(data)} bytes")

        self._parse(self.decrypted_data)

    def _parse(self, data: bytes):
        self.header_raw = data[:self.HEADER_SIZE]
        self.compressed_data_size = int.from_bytes(self.header_raw[12:16], byteorder='little', signed=False)

        if not is_zlib_compressed(data[self.HEADER_SIZE:]):
            raise ValueError("Payload is not zlib-compressed")
        import zlib
        self.sizes_raw = zlib.decompress(data[self.HEADER_SIZE:])

        if self.DUMP_DEBUG:
            with open(self.file_path.with_suffix('.sizes_dump.bin'), "wb") as f:
                f.write(self.sizes_raw)

        self.frame_offsets = []
        self.frame_sizes = []
        offset = 0
        for i in range(0, len(self.sizes_raw), 8):
            size = int.from_bytes(self.sizes_raw[i:i+8], byteorder='little', signed=False)
            self.frame_sizes.append(size)
            self.frame_offsets.append(offset)
            offset += size

    def get_total_body_size(self) -> int:
        return sum(self.frame_sizes)
    
    def get_total_file_size(self) -> int:
        return sum(self.frame_sizes) + self.compressed_data_size + self.HEADER_SIZE
    
    def get_frame_data(self, frame_index: int):
        """Get the raw data of a specific frame by index.

        """
        if frame_index < 0 or frame_index >= len(self.frame_sizes):
            raise IndexError("Frame index out of range")
        
        base = self.HEADER_SIZE + self.compressed_data_size
        frame_offset = base + self.frame_offsets[frame_index]
        frame_size = self.frame_sizes[frame_index]

        # frame_data = self.decrypted_data[frame_offset:frame_offset+frame_size]
        with open(self.file_path, "rb") as f:
            f.seek(frame_offset)
            frame_data = f.read(frame_size)

        key_id = frame_index
        test_decrypted = decrypt(frame_data, self.key, key_id)
        # first 4 bytes is an integer length of the uncompressed data
        first_4_bytes = test_decrypted[:4]
        zlib_data = test_decrypted[4:]
        uncompressed_length = int.from_bytes(first_4_bytes, byteorder='little', signed=False)
        print(f"Frame {frame_index} uncompressed length: {uncompressed_length}")
        # rest is zlib-compressed data
        try:
            raw_frame = zlib.decompress(zlib_data)
            if self.DUMP_DEBUG:
                with open(self.file_path.with_name(f"frame_{frame_index:04d}_encrypted.bin"), "wb") as f:
                    f.write(frame_data)
                with open(self.file_path.with_name(f"frame_{frame_index:04d}_decrypted.bin"), "wb") as f:
                    f.write(test_decrypted)
                with open(self.file_path.with_name(f"frame_{frame_index:04d}_uncompressed.bin"), "wb") as f:
                    f.write(raw_frame)
            return raw_frame
        except zlib.error as ex:
            print(f"Frame {frame_index} decompression failed after decryption", ex)
            return test_decrypted

    def parse_frame_data(self, data: bytes):
        """
        Format:
        - first 4 bytes: number of words in header (N)
        - next N * 4 bytes: header words (uint32 little-endian), each word is a size in bytes into the payload
        - remaining bytes: records of sizes specified in the header
        """
        # first word is number of words in header
        num_header_words = int.from_bytes(data[:4], byteorder='little', signed=False)
        header_size = num_header_words * 4  # bytes
        header = data[4:4+header_size]
        # parse header as list of uint32
        header_words = []
        for i in range(0, len(header), 4):
            header_bytes = header[i:i+4]
            word = int.from_bytes(header_bytes, byteorder='little', signed=False)
            header_words.append(word)
        payload = data[4+header_size:]
        print(sum(header_words) + 4 + header_size == len(data))
        # extract records from payload based on header words
        records = []
        offset = 0
        for size in header_words:
            record = payload[offset:offset+size]
            records.append(record)
            offset += size
        return header_words, records

# End of module

# example input for testing
if __name__ == "__main__":
    # Example salt values from scene at https://deovr.com/be9ngg
    # to get test data, any video with "ai passthrough" is ok.
    # on In Dec 2025 we used scene 85342:
    # go to https://deovr.com/be9ngg, open network tab, find api call to get metadata for scene 85342
    #   https://api.deovr.com/v2/videos/85342
    #   grab the "version" and "povBaseUrl" fields from the JSON response
    #   download the pov_mask.asvr file from the povBaseUrl link
    #   and use the filename as "base_url" (everything after last '/' and before '?' if any)
    scene_id = 85342
    version = b"1.5.0"
    base_url = b"pov_mask.asvr"

    data_folder = Path("test_data") / str(scene_id)
    asvr_file = data_folder / "pov_mask.asvr"

    asvr = AlphaStream(Path(asvr_file), scene_id, version, base_url)
    print(f"Total frames: {len(asvr.frame_sizes)}")
    print(f"Total size of all frames: {asvr.get_total_body_size()} bytes")
    print(f"Total ASVR file size: {asvr.get_total_file_size()} bytes")

    for inspect_frame_index in [1111]:
        frame_data = asvr.get_frame_data(inspect_frame_index)
        parsed_header, parsed_payload = asvr.parse_frame_data(frame_data)
        print(f"  Parsed header size: {len(parsed_header)} words, content: {parsed_header}...")
        print(f"  Parsed payload size: {len(parsed_payload)} records, content: {parsed_payload[:1]}...")