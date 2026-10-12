# Ear-Clipping Triangulation (Stencil-Free Triangle Output)

**Priority: low.** The fan triangles drawn with an even-odd stencil are the primary GPU path and are exact. See [TRIANGLE_RENDERING.md](../TRIANGLE_RENDERING.md); it works in wgpu, Vulkan, GL and GLES. This task only adds a convenience mode for renderers that can only draw plain filled meshes, such as engine mesh components without stencil access.

## Objective
Add an optional triangle output whose triangles do not overlap and cover exactly the mask, so it can be drawn with an ordinary filled triangle list: no stencil, no culling requirements and no parity test.

## Background (measured on real content)
- Each frame holds about 5 channels (up to 36), about 46 points per channel (median 11, max 614), and about 227 points per frame. Points are in the 2048×1024 source space.
- Polygons are strongly concave: about 40% of vertices are reflex. A fan filled normally over-covers by about 27%, which is why the current fan output needs a stencil.
- **Holes:** a channel nested inside another channel is a hole (libalphastream fills all channels with one even-odd pass). In 85342, 66 of 150 sampled frames contain holes. Every overlapping pair was full containment, never a partial overlap. Holes are wound opposite to their outer channel in 196 of 197 pairs. Holes can contain further nested channels (islands).
- **Self-intersections:** 8 of 814 sampled polygons in 85342 (7 of 200 frames) and 1 of 1330 in a second scene cross themselves. About 1% of polygons repeat a vertex. Ear clipping assumes simple polygons, so these need a defined fallback.
- The bitmap rasterizer and the Python reference are bit-exact with libalphastream, so they can serve as ground truth for coverage tests.

## Scope
- Group the channels of a frame into outer polygons with their holes. Determine nesting depth with a containment (point-in-polygon) test: even depth is an outer polygon, odd depth is a hole. Use the winding only as a cross-check, never as the rule.
- Triangulate each outer polygon together with its holes. Join each hole to the outer ring with a bridge edge, then ear-clip. Evaluate an existing implementation of the earcut algorithm (a Mapbox earcut port such as the `earcut` or `earcutr` crates) before writing one. It must handle holes and degenerate input without panicking.
- Self-intersecting polygons: earcut's local intersection handling is acceptable. Document that coverage can then differ slightly from the even-odd mask. A per-polygon fallback to the fan (marked for stencil) is out of scope unless it proves necessary.
- Output a triangle list of x,y `f32` pairs in the same 2048×1024 source space and layout as the fan output, so callers can switch modes without other changes.
- Expose it as a new processing mode next to `Triangles` (for example `ProcessingMode::TriangleMesh`), and through the C ABI, either as a new function or as a mode on the existing one. Keep the existing fan output unchanged.

## Deliverables
- A triangulation function in `rasterizer.rs` (or a new module), wired into the frame-processing task in `api.rs`.
- Unit tests:
  - square with a hole
  - hole with an island
  - concave polygons
  - fewer than three points
  - repeated vertices
  - a self-intersecting polygon (no panic)
- A coverage test that renders the triangles with a simple CPU triangle rasterizer, or compares areas, against `rasterize_channels` on sampled real frames.
- A section in [TRIANGLE_RENDERING.md](../TRIANGLE_RENDERING.md) that describes the mode and its self-intersection caveat.

## Acceptance Criteria
- On simple polygons (the vast majority), triangles do not overlap and their union equals the even-odd fill, up to pixel-centre sampling at edges.
- Holes and islands are correct.
- No panics on fuzzed input (proptest, as for the existing rasterizer).
- Typical frame cost stays well below the bitmap path (about 0.2 ms at 2048×1024). The expected cost is tens of µs for about 230 vertices.

## References
- [TRIANGLE_RENDERING.md](../TRIANGLE_RENDERING.md): the stencil method and the fan output format
- [REVERSE_ENGINEERING.md](../REVERSE_ENGINEERING.md#rasterizer-details-and-parity): native fill semantics
- [19-triangle-generation.md](19-triangle-generation.md)
