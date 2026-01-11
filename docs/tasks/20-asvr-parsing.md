# ASVR File Parsing

## Objective
Implement parsing of encrypted ASVR files, including header parsing, frame extraction, and polystream data retrieval.

## Scope
On initialization, read and decrypt header and sizes table to extract frame offsets and sizes. Frame data is read, decrypted, and decompressed only when requested for streaming.

## Deliverables
ASVR parser module in Rust, functions for parsing headers, extracting frames, and retrieving polystream records.

## Dependencies
- [docs/tasks/22-decryption.md](docs/tasks/22-decryption.md)

## Checklist
- On init: Parse 16-byte header (magic, version, compressed sizes size), decrypt and decompress sizes table to get frame sizes
- On demand: For requested frame: decrypt, decompress, parse into header words and records, extract polystream data from records

## Acceptance Criteria
Correctly parses ASVR files matching Python implementation, outputs polystream data for rasterization.

## References
- [python/alpha_stream_crypto.py](python/alpha_stream_crypto.py)
- [docs/FILE_FORMAT.md](docs/FILE_FORMAT.md)