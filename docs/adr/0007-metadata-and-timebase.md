# ADR 0007: Metadata Parsing and Timebase Mapping

## Status
Accepted

## Context
Accurate metadata extraction is essential for correct frame scheduling, synchronization, and diagnostics in alphastream-rs. While a timebase mapping ($t_n = n/60$) is documented, strict time-based access to frames is not yet implemented or decided. The current API requires users to translate any time-based indexing into frame indices themselves. This approach leaves flexibility for future changes and avoids premature commitment to a specific time/frame mapping policy.

Additionally, the encoding of stereo frames in the ASVR format is not yet fully understood. The working assumption is that stereo frames are stored in alternating order (even indices for left eye, odd for right eye), but this is unconfirmed. For now, the responsibility for stereo frame selection and interpretation is placed on the API user.

## Decision
- Parse and validate all required metadata fields for ASVP and ASVR formats (e.g., magic, version, frame count, compressed size).
- Expose metadata via API for diagnostics and integration.
- Implement index-to-time mapping using $t_n = n/60$ for all formats, with validation and drift correction.
- Document metadata schema and timebase mapping in user-facing documentation.

## Consequences
- Ensures consistent frame scheduling and synchronization across sources and formats
- Enables diagnostics and validation for input files
- Supports future extensibility for new metadata fields or timebase policies

## References
- [docs/tasks/18-metadata-timebase.md](../tasks/18-metadata-timebase.md)
- [docs/RUST_IMPLEMENTATION.md](../RUST_IMPLEMENTATION.md)
- [docs/prd/prd-alphastream-rs.md](../prd/prd-alphastream-rs.md)
