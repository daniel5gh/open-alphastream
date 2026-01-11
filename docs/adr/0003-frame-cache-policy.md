# ADR 0003: Frame Cache Policy (LRU, Prefetch, Thread Safety)

## Status
Accepted

## Context
Efficient frame caching is critical for performance and memory usage in alphastream-rs. The cache must support high concurrency, predictable memory footprint, and fast access, while integrating with the scheduler for prefetching and eviction.

## Decision
- Use a strict LRU (Least Recently Used) eviction policy with a default capacity of 512 frames.
- Cache stores alpha bit masks and optionally triangle strips, keyed by frame index.
- Prefetching is implemented using a forward anticipation strategy, preloading future frames based on sequential access patterns.
- Cache is wrapped in `Arc<RwLock<>>` to ensure thread-safe concurrent access.
- All cache operations are race-free and validated by concurrent access tests.

## Consequences
- Predictable memory usage and eviction behavior
- Improved performance for sequential and burst frame access
- Safe concurrent access from multiple threads
- Integration with scheduler for optimized data flow

## References
- [docs/tasks/09-frame-cache-policy.md](../tasks/09-frame-cache-policy.md)
- [docs/tasks/24-lru-cache.md](../tasks/24-lru-cache.md)
- [docs/tasks/25-prefetching.md](../tasks/25-prefetching.md)
- [docs/tasks/26-thread-safety-cache.md](../tasks/26-thread-safety-cache.md)
- [docs/tasks/27-cache-scheduler-integration.md](../tasks/27-cache-scheduler-integration.md)
- [docs/RUST_IMPLEMENTATION.md](../RUST_IMPLEMENTATION.md)
- [docs/prd/prd-alphastream-rs.md](../prd/prd-alphastream-rs.md)
