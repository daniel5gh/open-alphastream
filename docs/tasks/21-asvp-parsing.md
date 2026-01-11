# ASVP File Parsing

## Objective
Implement parsing of plaintext ASVP files, including header parsing, frame extraction, and polystream data retrieval.

## Scope
On initialization, read header and decompress sizes table to extract frame offsets and sizes. Frame data is decompressed only when requested for streaming.

## Deliverables
ASVP parser module in Rust, functions for parsing headers, extracting frames, and retrieving polystream records.

## Dependencies
- None (plaintext format)

## Checklist
- On init: Parse 16-byte header (magic ASVP, version PLN1, num sizes, compressed sizes size), decompress sizes table to get frame sizes
- On demand: For requested frame: decompress, parse into header words and records, extract polystream data from records

## Acceptance Criteria
Correctly parses ASVP files matching Python implementation, outputs polystream data for rasterization.

## References
- [python/decrypt.py](python/decrypt.py)
- [docs/FILE_FORMAT_PLAINTEXT.md](docs/FILE_FORMAT_PLAINTEXT.md)