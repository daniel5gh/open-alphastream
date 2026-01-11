# ADR 0001: Unified Format Abstraction for Alphastream (ASVR/ASVP)

## Status
Accepted

## Context
Alphastream must support multiple evolving file formats (encrypted ASVR, plaintext ASVP, and future versions). The implementation must allow for version-aware parsing, metadata access, and per-frame decode, while enabling extensibility and high performance.

## Decision
Define a unified trait abstraction (`ASFormat`) for all supported AlphaStream formats. This trait exposes:
- Metadata access (dimensions, version, frame count)
- Frame discovery and random access
- Per-frame decode to alpha mask
- Capability negotiation for version/features
- Extensible error taxonomy
- Zero-copy and SIMD-friendly decode paths

## Consequences
- Enables seamless support for new format versions and features
- Simplifies API surface for consumers
- Centralizes error handling and capability negotiation
- Ensures high performance via zero-copy and SIMD optimizations

## References
- [docs/tasks/02-format-abstraction.md](../tasks/02-format-abstraction.md)
- [docs/RUST_IMPLEMENTATION.md](../RUST_IMPLEMENTATION.md)
- [docs/prd/prd-alphastream-rs.md](../prd/prd-alphastream-rs.md)
