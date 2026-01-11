# ADR 0010: Write Support for ASVR and ASVP Formats

## Status
Accepted

## Context
The current implementation ([`formats.rs`](../../rust/alphastream-rs/src/formats.rs)) only supports reading ASVR (encrypted) and ASVP (plaintext) formats through the [`ASFormat`](../../rust/alphastream-rs/src/formats.rs:54) trait. This read-only abstraction was sufficient for initial development and playback use cases, but two new requirements have emerged:

### Use Case 1: Test Suite Generation
Integration tests and benchmarks require reproducible test data. Currently, the test suite relies on hardcoded test vectors (see [`formats.rs:420-433`](../../rust/alphastream-rs/src/formats.rs:420)) for basic decryption/decompression verification. However, comprehensive testing requires:
- Full file roundtrip tests (read → write → read should produce identical data)
- Frame-level manipulation tests
- Error injection tests with known malformed inputs
- Performance benchmarking with realistic file sizes

Generating test files programmatically allows the test suite to create files of arbitrary complexity without committing large binary blobs to the repository.

### Use Case 2: ASVR to ASVP Conversion
A common operational requirement is to decrypt ASVR files into plaintext ASVP files for:
- Archival and backup of decrypted content
- Analysis and debugging of AlphaStream data
- Conversion pipelines for legacy systems
- Creating test fixtures from real-world data

This conversion requires the ability to:
1. read_file and decrypt an ASVR file
2. write_to_file the decoded frames to a new ASVP file

## Decision
Implement writer structs for ASVR and ASVP formats that collect all frames first, then write the complete file in a single operation. This two-phase approach is necessary because the file header contains the compressed sizes table, which requires knowing all frame sizes before writing.

### Writer Design

The ASWrite trait is replaced with concrete writer structs that follow a two-phase pattern:

```rust
/// Writer for plaintext ASVP format
/// Collects frames first, then writes the complete file
pub struct ASVPWriter<W: Write> {
    writer: W,
    frames: Vec<FrameData>,
}

impl<W: Write> ASVPWriter<W> {
    /// Create a new writer
    pub fn new(writer: W) -> Self {
        Self { writer, frames: Vec::new() }
    }
    
    /// Add a frame to be written
    pub fn add_frame(&mut self, frame: FrameData) {
        self.frames.push(frame);
    }
    
    /// Write all collected frames to the file
    /// This writes the header first (with sizes table), then all frames
    pub fn write_all(mut self) -> Result<(), FormatError> {
        // Pre-compress all frames to determine sizes
        let mut frame_sizes = Vec::with_capacity(self.frames.len());
        let mut compressed_frames = Vec::with_capacity(self.frames.len());
        
        for frame in &self.frames {
            let compressed = compress_zlib(&frame.polystream)?;
            frame_sizes.push(compressed.len() as u64);
            compressed_frames.push(compressed);
        }
        
        // Write header with sizes table
        let sizes_bytes: Vec<u8> = frame_sizes.iter()
            .flat_map(|s| s.to_le_bytes())
            .collect();
        let compressed_sizes = compress_zlib(&sizes_bytes)?;
        
        // Write 16-byte header
        let mut header = [0u8; 16];
        header[12..16].copy_from_slice(&(compressed_sizes.len() as u32).to_le_bytes());
        self.writer.write_all(&header)?;
        self.writer.write_all(&compressed_sizes)?;
        
        // Write each frame: 4-byte length + compressed data
        for compressed in &compressed_frames {
            let len_bytes = (compressed.len() as u32).to_le_bytes();
            self.writer.write_all(&len_bytes)?;
            self.writer.write_all(compressed)?;
        }
        
        Ok(())
    }
}

/// Writer for encrypted ASVR format
/// Similar to ASVPWriter but with encryption
pub struct ASVRWriter<W: Write> {
    writer: W,
    scene_id: u32,
    version: Vec<u8>,
    base_url: Vec<u8>,
    key: [u8; 32],
    frames: Vec<FrameData>,
}

impl<W: Write> ASVRWriter<W> {
    /// Create a new writer with encryption parameters
    pub fn new(writer: W, scene_id: u32, version: &[u8], base_url: &[u8]) 
        -> Result<Self, FormatError> {
        let key = derive_key(scene_id, version, base_url)?;
        Ok(Self {
            writer,
            scene_id,
            version: version.to_vec(),
            base_url: base_url.to_vec(),
            frames: Vec::new(),
            key,
        })
    }
    
    /// Add a frame to be written
    pub fn add_frame(&mut self, frame: FrameData) {
        self.frames.push(frame);
    }
    
    /// Write all collected frames to the encrypted file
    pub fn write_all(mut self) -> Result<(), FormatError> {
        // Pre-compress and encrypt all frames
        let mut frame_sizes = Vec::with_capacity(self.frames.len());
        let mut encrypted_frames = Vec::with_capacity(self.frames.len());
        
        for (i, frame) in self.frames.iter().enumerate() {
            let compressed = compress_zlib(&frame.polystream)?;
            let mut frame_with_len = Vec::new();
            frame_with_len.extend_from_slice(&(compressed.len() as u32).to_le_bytes());
            frame_with_len.extend_from_slice(&compressed);
            
            let encrypted = encrypt_frame_data(&frame_with_len, &self.key, i as u32)?;
            frame_sizes.push(encrypted.len() as u64);
            encrypted_frames.push(encrypted);
        }
        
        // Write header with encrypted sizes table
        let sizes_bytes: Vec<u8> = frame_sizes.iter()
            .flat_map(|s| s.to_le_bytes())
            .collect();
        let compressed_sizes = compress_zlib(&sizes_bytes)?;
        
        // Encrypt header + sizes with key_id = 0xFFFFFFFF
        let mut header = [0u8; 16];
        header[12..16].copy_from_slice(&(compressed_sizes.len() as u32).to_le_bytes());
        let mut to_encrypt = header.to_vec();
        to_encrypt.extend_from_slice(&compressed_sizes);
        let encrypted_sizes = encrypt_frame_data(&to_encrypt, &self.key, 0xFFFFFFFF)?;
        
        self.writer.write_all(&encrypted_sizes)?;
        
        // Write each encrypted frame
        for encrypted in &encrypted_frames {
            self.writer.write_all(encrypted)?;
        }
        
        Ok(())
    }
}
```

### Key Design Decisions

1. **Two-phase writing**: Frames are collected first via [`add_frame()`](docs/adr/0010-write-support-for-asvr-and-asvp.md), then written in a single [`write_all()`](docs/adr/0010-write-support-for-asvr-and-asvp.md) call. This allows computing all compressed sizes before writing the header.

2. **FrameData input**: The writer accepts [`FrameData`](rust/alphastream-rs/src/formats.rs:44) structs containing polystream data, matching the read interface.

3. **Separate writers per format**: `ASVPWriter` and `ASVRWriter` are distinct types, allowing format-specific optimizations and error handling.

4. **No streaming write**: Unlike reading, writing requires knowing all frame sizes upfront. This is acceptable because:
   - Test fixture generation creates files in memory first
   - Conversion use cases read entire files before writing
   - Memory overhead is acceptable for typical AlphaStream file sizes

### Implementation Strategy

#### ASVP Writing (Priority: High)
ASVP is the plaintext format and is straightforward to implement:
1. Collect all frames (FrameData with polystream)
2. Pre-compress each frame to determine sizes
3. Compress the sizes table using zlib
4. write_to_file 16-byte header (padding + compressed_sizes_size)
5. write_to_file compressed sizes table
6. For each frame: write frame length (4 bytes LE) + compressed data

#### ASVR Writing (Priority: Medium)
ASVR requires encryption, making it more complex:
1. Collect all frames (FrameData with polystream)
2. Derive encryption key from scene_id, version, and base_url
3. Pre-compress and encrypt all frames to determine sizes
4. Compress and encrypt the sizes table with key_id = 0xFFFFFFFF
5. write_to_file encrypted header + sizes table
6. For each frame: write encrypted frame data

### Integration with Existing Code

The write functionality will be exposed through:
1. **Utility functions** in [`formats.rs`](../../rust/alphastream-rs/src/formats.rs) for standalone file conversion
2. **Test utilities** in [`testlib.rs`](../../rust/alphastream-rs/src/testlib.rs) for generating test fixtures
3. **CLI tool** (extending [`demo.rs`](../../rust/alphastream-rs/src/bin/demo.rs)) for command-line conversion

### Error Handling

Write operations will use the existing [`FormatError`](../../rust/alphastream-rs/src/formats.rs:22) enum, with potential additions:
- `WriteError` variant for I/O failures during write operations
- `CompressionError` for zlib compression failures (via `#[from]` on `FormatError`)
- `EncryptionError` for encryption failures

## Consequences

### Positive
- Enables comprehensive integration testing with generated fixtures
- Supports ASVR to ASVP conversion for debugging and archival
- Maintains separation of concerns (read vs write operations)
- Reuses existing compression/encryption infrastructure
- Clear two-phase design avoids streaming complexity

### Negative
- Increases code complexity with additional writer types
- Requires careful testing to ensure write format matches read expectations
- ASVR writing requires key derivation, adding complexity
- Memory usage proportional to file size (acceptable for use cases)

### Neutral
- Write operations are separate from read operations, allowing independent optimization
- Can be implemented incrementally (ASVP first, then ASVR)

## References
- [formats.rs](../../rust/alphastream-rs/src/formats.rs) - Current read-only format implementation
- [testlib.rs](../../rust/alphastream-rs/src/testlib.rs) - Test utilities for extension
- [ADR 0001: Format Abstraction](0001-format-abstraction.md) - Original read-only abstraction decision
- [FILE_FORMAT.md](../../FILE_FORMAT.md) - ASVR/ASVP format specification
- [FILE_FORMAT_PLAINTEXT.md](../../FILE_FORMAT_PLAINTEXT.md) - ASVP format details
