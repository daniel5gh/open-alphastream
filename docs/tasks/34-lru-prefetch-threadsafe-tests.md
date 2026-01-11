# Expanded Unit/Integration Tests: LRU, Prefetching, Thread Safety, Scheduler Integration

## Objective
Expand the unit and integration test suite to comprehensively cover LRU eviction, prefetching logic, thread safety, and cache-scheduler integration.

## Scope
- Add/expand tests in [rust/alphastream-rs/tests/integration_tests.rs](rust/alphastream-rs/tests/integration_tests.rs) for:
  - LRU eviction policy
  - Forward anticipation prefetching
  - Thread-safe cache operations
  - Scheduler and cache integration

## Deliverables
- Comprehensive test coverage for all cache and scheduler behaviors.
- Tests for concurrent access, eviction correctness, prefetching, and integration scenarios.

## Checklist
- Add/expand unit tests for LRU eviction.
- Add/expand tests for prefetching logic.
- Add/expand tests for thread safety (concurrent access).
- Add/expand integration tests for scheduler-cache coordination.

## Acceptance Criteria
- All new and existing tests pass.
- Coverage includes LRU, prefetching, thread safety, and integration.
- No regressions or uncovered scenarios.

## References
- [rust/alphastream-rs/tests/integration_tests.rs](rust/alphastream-rs/tests/integration_tests.rs)
