# Task 16 — Integration Tests

## Objective
- End-to-end integration tests for alphastream-rs, validating full pipeline from source to frame retrieval.

## Scope
- Test matrix covering sources and formats.
- Scenarios including load, schedule, get_frame operations at various sizes.
- Error path testing for robustness.

## Test Matrix
- Sources: HTTP, local file system, in-memory.
- Formats: ASVR (encrypted), ASVP (plain).
- Combinations: All source-format pairs (3 × 2 = 6 test configurations).

## Scenarios
- **Load Operations**: Initialize streams from each source-format combination.
- **Schedule Operations**: Schedule frame requests with varying rates and concurrency.
- **Get Frame Operations**: Retrieve frames at different sizes (e.g., 100×100, 1920×1080, 4K).
- **Error Paths**: Simulate network failures (HTTP), file access errors (local), memory limits (in-memory); invalid formats, corrupted data, timeouts.

Example test case:
```
#[test]
fn test_http_asvr_load_and_get_frame() {
    // Setup HTTP server with ASVR data
    // Load stream
    // Schedule and get frame
    // Assert frame data integrity
}
```

## Implementation Checklist
- Develop test code for each matrix combination.
- Ensure coverage of all scenarios and error paths.
- Implement flake resistance: timeouts (e.g., 30s per test), retries (up to 3), test isolation (no shared state).
- Integrate with Rust test framework; use async testing for concurrency.

## Acceptance Criteria
- 95% pass rate across 100+ runs in CI/CD.
- Flake-resistant: No more than 1% false failures due to timing or environment issues.

## Deliverables
- Integration test suite in `rust/alphastream-rs/tests/`.

## Dependencies
- [docs/tasks/02-format-abstraction.md](docs/tasks/02-format-abstraction.md)
- [docs/tasks/03-transport-http.md](docs/tasks/03-transport-http.md)
- [docs/tasks/04-transport-local.md](docs/tasks/04-transport-local.md)
- [docs/tasks/05-transport-in-memory.md](docs/tasks/05-transport-in-memory.md)
- [docs/tasks/06-async-runtime-concurrency.md](docs/tasks/06-async-runtime-concurrency.md)
- [docs/tasks/07-scheduler-rate-control.md](docs/tasks/07-scheduler-rate-control.md)
- [docs/tasks/08-frame-cache-policy.md](docs/tasks/08-frame-cache-policy.md)
- [docs/tasks/09-rasterization-resize.md](docs/tasks/09-rasterization-resize.md)
- [docs/tasks/10-public-api-facade.md](docs/tasks/10-public-api-facade.md)
- [docs/tasks/11-c-abi-pinvoke.md](docs/tasks/11-c-abi-pinvoke.md)

## References
- [docs/RUST_IMPLEMENTATION.md](docs/RUST_IMPLEMENTATION.md)
