# Task 09 — Frame Cache Policy

## Objective
Specify the frame cache policy for alphastream-rs, including capacity limits, eviction strategy, prefetch interaction, concurrency handling, and storage options (bit masks, triangle strips, or both).

## Scope
- Cap: Count-based limit of 512 frames.
- Eviction: Strict LRU; no pinning.
- Prefetch Interaction: Bounded by remaining capacity.
- Concurrency: Atomic stats, low-contention.
- Storage: Bit masks, triangle strips, or both.

## Implementation Checklist
- LRU index implementation:
  ```rust
  struct LRUIndex {
      // Ordered map for LRU tracking
  }
  impl LRUIndex {
      fn new() -> Self {
          // Initialize LRU structure
      }
      fn access(&mut self, key: &str) {
          // Update access order
      }
  }
  ```
- Eviction logic:
  ```rust
  fn evict(&mut self) {
      if self.len() >= 512 {
          // Remove least recently used frame
      }
  }
  ```
- Atomic stats for hit/miss rates.

## Acceptance Criteria
- Hit/miss rates validated under load.
- Cap enforced at 512 without leaks.

## References
- [docs/RUST_IMPLEMENTATION.md](docs/RUST_IMPLEMENTATION.md)
- [docs/tasks/08-scheduler-rate-control.md](docs/tasks/08-scheduler-rate-control.md)
