# Triangle List Generation from Polystreams

## Objective
Generate triangle lists from polystreams for caller-side rasterization.

## Scope
Input polystream, output Vec<f32> as a triangle list (x,y pairs, three vertices per triangle).

## Deliverables
Function in rasterizer.rs, tests.

## Dependencies
- [docs/tasks/10-rasterization-polystreams.md](docs/tasks/10-rasterization-polystreams.md)

## Checklist
- Parse polystream
- Triangulate
- Output triangle list

## Acceptance Criteria
Matches rasterizer input, correct triangle list format.

## References
- [docs/RUST_IMPLEMENTATION.md](docs/RUST_IMPLEMENTATION.md)