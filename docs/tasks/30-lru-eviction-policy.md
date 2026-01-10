# LRU Eviction Policy Implementation

## Objective
Implement and enforce an LRU (Least Recently Used) eviction policy in the cache to maintain a maximum of 512 frames for memory efficiency.

## Scope
- Implement LRU eviction logic in [rust/alphastream-rs/src/cache.rs](rust/alphastream-rs/src/cache.rs).
- Ensure cache never exceeds 512 frames.
- Add unit tests for LRU behavior.

## Deliverables
- Updated cache implementation with LRU eviction.
- Unit tests verifying correct eviction and performance.

## Checklist
- Implement LRU eviction in cache.
- Set cache capacity to 512 frames.
- Add unit tests for eviction logic.
- Validate performance is not degraded.

## Acceptance Criteria
- Cache evicts the least recently used frame when exceeding 512 frames.
- All tests pass and performance is maintained.

## References
- [rust/alphastream-rs/src/cache.rs](rust/alphastream-rs/src/cache.rs)
