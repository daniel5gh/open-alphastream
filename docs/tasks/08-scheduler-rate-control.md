# Task 08 — Scheduler & Rate Control

## Objective
Implement an async scheduler with forward anticipation strategy for alphastream-rs, ensuring efficient caching and prefetching assuming forward playback.

## Forward Anticipation Strategy
On frame request, fetch if not cached, evict distant frames, prefetch subsequent frames assuming forward playback.

## Prefetch & Cache
Cache size=512 frames with LRU eviction. Prefetch focuses on subsequent frames to anticipate forward playback.

## Priority Handling
The `get_frame` operation escalates priority with a 12ms timebox. If the operation exceeds 12ms, priority is increased to ensure timely completion.

## Implementation Checklist
- **Queues and prioritization**: Implement priority queues for frame requests.
  ```rust
  use std::collections::BinaryHeap;
  // Priority queue for frame requests
  let mut frame_queue: BinaryHeap<FrameRequest> = BinaryHeap::new();
  ```
- **Timebox logic and enforcement**: Enforce 12ms timebox on `get_frame`.
  ```rust
  async fn get_frame_with_timebox(&self, frame_id: u64) -> Result<Frame, Error> {
      tokio::time::timeout(Duration::from_millis(12), self.get_frame(frame_id)).await
  }
  ```
- **Coordination with cache and runtime**: Integrate with async runtime and cache policy, implementing eviction of distant frames and prefetch of subsequent ones.

## Acceptance Criteria
- Forward anticipation simulation passes: Verify that the scheduler efficiently handles frame requests with forward anticipation.
- Latency bounds respected: Ensure `get_frame` completes within 12ms for high-priority requests.

## References
- [docs/RUST_IMPLEMENTATION.md](docs/RUST_IMPLEMENTATION.md)
- [docs/tasks/09-frame-cache-policy.md](docs/tasks/09-frame-cache-policy.md)
