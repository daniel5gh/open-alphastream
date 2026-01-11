# Decryption for ASVR

## Objective
Implement cryptographic decryption for ASVR files using scrypt key derivation and ChaCha20 decryption.

## Scope
On initialization, derive key from scene_id, version, base_url using scrypt and decrypt header/sizes with fixed key_id. Decrypt frames on demand using frame index as key_id.

## Deliverables
Crypto module in Rust with key derivation and decryption functions.

## Dependencies
- None

## Checklist
- On init: Implement scrypt key derivation with parameters from Python, decrypt header/sizes with key_id 0xFFFFFFFF
- On demand: For each frame, decrypt with frame index as key_id using ChaCha20

## Acceptance Criteria
Decrypts ASVR files identically to Python implementation.

## References
- [python/alpha_stream_crypto.py](python/alpha_stream_crypto.py)