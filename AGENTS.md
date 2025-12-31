# AGENTS

## Project goal
An open implementation of libalphastream, with:
- A reference implementation in Python for educational purposes
- A very high-performance Rust implementation suitable for production use

## Abstraction for Alphastream data
The data abstraction supports:
- Encrypted ASVR as in the original libalphastream for known versions (up to 1.5.0)
- Decoded plain ASVP format (our own)
- Future newer versions as original libalphastream evolves

## Repository layout
- ./docs — technical documentation
- ./python — Python reference implementation
- ./rust — Rust production implementation

## Rust implementation guidelines
Agents must always keep performance in mind for the Rust implementation, but readability is also important.
