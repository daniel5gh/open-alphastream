# Format Integration with Scheduler

## Objective
Integrate ASVR/ASVP parsing with the scheduler and cache for on-demand polystream to rasterized frame/triangle-strip flow.

## Scope
Describe the on-demand flow: user requests frame -> lib checks cache -> spawns tasks to read file range on demand -> decrypt/decompress -> rasterize -> cache/store.

## Deliverables
Integration code in scheduler or formats module to pass polystreams through to rasterization.

## Dependencies
- [docs/tasks/09-frame-cache-policy.md](docs/tasks/09-frame-cache-policy.md)
- [docs/tasks/20-asvr-parsing.md](docs/tasks/20-asvr-parsing.md)
- [docs/tasks/21-asvp-parsing.md](docs/tasks/21-asvp-parsing.md)
- [docs/tasks/08-scheduler-rate-control.md](docs/tasks/08-scheduler-rate-control.md)
- [docs/tasks/10-rasterization-polystreams.md](docs/tasks/10-rasterization-polystreams.md)

## Checklist
- Implement on-demand flow: user requests frame -> check cache for rasterized frame
- If cache miss: spawn task to read file range -> decrypt/decompress if needed -> parse to polystream -> rasterize -> cache/store result
- Scheduler controls rate of processing tasks

## Acceptance Criteria
End-to-end on-demand flow from frame request to cached rasterized output via cache, scheduler, and formats.

## References
- [rust/alphastream-rs/src/scheduler.rs](rust/alphastream-rs/src/scheduler.rs)
- [rust/alphastream-rs/src/rasterizer.rs](rust/alphastream-rs/src/rasterizer.rs)