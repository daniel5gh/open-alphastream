# Task 16 — Metadata & Timebase

## Objective
- Metadata parsing; timebase derivation.

## Scope
- Map index/time; confirm $t_n = \frac{n}{60}$ policy.

## Deliverables
- Parser docs
- Mapping docs

## Dependencies
- [docs/tasks/02-format-abstraction.md](docs/tasks/02-format-abstraction.md)
- [docs/tasks/07-scheduler-rate-control.md](docs/tasks/07-scheduler-rate-control.md)

## Implementation Checklist
- Enumerate metadata fields
- Validation rules for required/optional fields
- Derive index→time mapping and verify $t_n = \frac{n}{60}$

## Acceptance Criteria
- Consistent frame scheduling across sources and formats

## Metadata Fields

Parse header fields for format identification and validation.

### ASVP (Plaintext) Fields
- `magic`: Bytes 0..3, ASCII "ASVP". Required; validate exact match.
- `version`: Bytes 4..7, ASCII "PLN1". Required; validate exact match.
- `frame_count`: Bytes 8..11, uint32 LE. Required; must be > 0.
- `compressed_data_size`: Bytes 12..15, uint32 LE. Required; must be > 0.

### ASVR (Encrypted) Fields
- `compressed_data_size`: Bytes 12..15, uint32 LE. Required; must be > 0.
- Other header bytes: Reserved; no validation enforced.

Validation: Reject file if required fields invalid or sizes table decompression fails. Frame count derived from sizes table length.

## Timebase Mapping

Frame index $n$ maps to time $t_n = \frac{n}{60}$ seconds, assuming 60 fps target rate.

Confirm derivation: $t_n = \frac{n}{\text{fps}}$ with fps=60.

## Implementation Checklist
- [ ] Implement header parser for ASVP and ASVR fields.
- [ ] Add validation logic for required fields and sizes table integrity.
- [ ] Code timebase mapping function: `fn time_from_index(n: u32) -> f64 { n as f64 / 60.0 }`.
- [ ] Integrate metadata parsing into format abstraction trait.
- [ ] Verify mapping in scheduler rate control.

## Acceptance Criteria
- Frame scheduling uses consistent $t_n = \frac{n}{60}$ across all formats and transports.
- Metadata parsing succeeds for valid ASVP/ASVR files; rejects invalid headers.
- Timebase drift correction maintains cadence within 1ms tolerance.

## References
- [docs/FILE_FORMAT.md](docs/FILE_FORMAT.md)
- [docs/RUST_IMPLEMENTATION.md](docs/RUST_IMPLEMENTATION.md)
