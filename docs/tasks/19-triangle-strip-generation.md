# Triangle Strip Generation from Polystreams

## Objective
Generate triangle strips from polystreams for caller-side rasterization.

## Scope
Input polystream, output Vec<f32> in triangle strip order (x,y pairs).

## Deliverables
Function in rasterizer.rs, tests.

## Dependencies
- [docs/tasks/10-rasterization-polystreams.md](docs/tasks/10-rasterization-polystreams.md)

## Checklist
- Parse polystream
- Triangulate
- Output strip

## Acceptance Criteria
Matches rasterizer input, correct strip format.

## References
- [docs/RUST_IMPLEMENTATION.md](docs/RUST_IMPLEMENTATION.md)