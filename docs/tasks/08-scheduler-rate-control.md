# Task 08 — Scheduler & Rate Control

## Objective
Implement an async scheduler with hybrid index-timebase for alphastream-rs, ensuring rate control at 60 fps with adaptive backpressure, prefetch, and priority handling.

## Timebase
The scheduler uses an index-based timebase at target_fps=60, where the time for frame n is calculated as:
\[ t_n = \frac{n}{60} \]

This provides a deterministic timeline for frame scheduling.

## Adaptive Backpressure
- **Duplicate when behind**: If the scheduler lags behind the target timebase, duplicate the current frame to maintain continuity.
- **Skip when oversupplied**: If frames are produced faster than consumed, skip excess frames to prevent buffer overflow.

## Prefetch & Cache
Prefetch window set to 120 frames, bounded by the cache capacity. The prefetch formula ensures ahead-of-time loading:
\[ \text{prefetch_window} = 120 \]

Cache bounds are enforced to limit memory usage.

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
- **Coordination with cache and runtime**: Integrate with async runtime and cache policy.

## Acceptance Criteria
- Timeline simulation passes: Verify that the scheduler maintains the 60 fps timebase under load.
- Latency bounds respected: Ensure `get_frame` completes within 12ms for high-priority requests.

## References
- [docs/RUST_IMPLEMENTATION.md](docs/RUST_IMPLEMENTATION.md)
- [docs/tasks/09-frame-cache-policy.md](docs/tasks/09-frame-cache-policy.md)
