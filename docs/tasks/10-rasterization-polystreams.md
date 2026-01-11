# Rasterization of Polystreams to R8 Alpha Masks or Triangle Strips

## Objective
Implement optional rasterization of polygon streams into R8 alpha bit masks or triangle strips.

## Input
Polystreams from ASVP/ASVR decoded content.

## Fill Rule
Even-odd winding.

## Output
R8 alpha masks or triangle strips, top-left origin, row-major for masks.

## Coordinate Mapping
To target width/height.

## Implementation Checklist
- Parse polystream primitives; build edge lists.
  ```rust
  // Build edge list from polystream points
  let mut edges = Vec::new();
  for i in 0..points.len() - 1 {
      let (x0, y0) = points[i];
      let (x1, y1) = points[i + 1];
      if y0 != y1 {
          edges.push((x0, y0, x1, y1));
      }
  }
  // Close polygon if endpoints differ
  if points[0] != points[points.len() - 1] {
      let (x0, y0) = points[points.len() - 1];
      let (x1, y1) = points[0];
      if y0 != y1 {
          edges.push((x0, y0, x1, y1));
      }
  }
  ```
- Scanline rasterization implementing even-odd fill.
  ```rust
  // Scanline fill for even-odd rule
  for y in 0..height {
      let mut xs = Vec::new();
      for &(x0, y0, x1, y1) in &edges {
          let ymin = y0.min(y1);
          let ymax = y0.max(y1);
          if y >= ymin && y < ymax {
              let x = if x0 == x1 {
                  x0
              } else {
                  x0 + (((y - y0) as f32 / (y1 - y0) as f32) * (x1 - x0) as f32) as i32
              };
              xs.push(x);
          }
      }
      xs.sort();
      for i in (0..xs.len()).step_by(2) {
          if i + 1 < xs.len() {
              let start = xs[i].max(0);
              let end = xs[i + 1].min(width - 1);
              for x in start..=end {
                  mask[y * width + x] = 255;
              }
          }
      }
  }
  ```
- Clipping, bounds checks, and writing R8 buffer.
- Unit tests with golden outputs derived from Python.
- Optional rasterization and storage of triangle strips.

## Acceptance Criteria
Bitwise parity with Python reference.

## Deliverables
- Rasterizer module producing alpha masks.
- Cross-check with Python reference [python/alpha_stream_draw.py](python/alpha_stream_draw.py) for parity on sample scenes.

## Dependencies
- [docs/tasks/02-format-abstraction.md](docs/tasks/02-format-abstraction.md)
- [docs/tasks/09-frame-cache-policy.md](docs/tasks/09-frame-cache-policy.md)
- [docs/RUST_IMPLEMENTATION.md](docs/RUST_IMPLEMENTATION.md)

## References
- [python/alpha_stream_draw.py](python/alpha_stream_draw.py)
- [docs/RUST_IMPLEMENTATION.md](docs/RUST_IMPLEMENTATION.md)
