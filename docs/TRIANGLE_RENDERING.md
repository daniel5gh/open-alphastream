# Rendering the Triangle Output

`get_triangle_strip_vertices` / `CV_get_triangle_strip_vertices` return a frame's mask as triangles instead of a bitmap. Generating them is a single pass over the points, and a typical frame is about 5 KB (about 230 triangles), whatever the output resolution. The GPU then rasterizes the mask at any resolution, optionally with MSAA. The triangles are **not** a ready-to-fill mesh, though: drawn naively they give a wrong mask. This page explains what you get and how to draw it to get the same shape as the bitmap rasterizer (and libalphastream).

The result can still differ from the bitmap at edge pixels. The GPU samples pixel centres, while the bitmap samples crossings at the integer row and also draws each polygon's outline, which makes it up to one pixel larger on the right and bottom edges.

## What you get

- A flat `f32` array of `x, y` pairs. The float count is `count`, so there are `count / 2` vertices and `count / 6` triangles.
- **Triangle list** topology: every three vertices form one triangle, and no vertices are shared. Do not draw with strip topology. A strip would draw every triangle twice, which cancels out in the stencil below, and would add triangles bridging one channel to the next.
- Coordinates are in the **2048×1024 source space** with a top-left origin and y pointing down. That space covers the whole mask (the same area the bitmap covers at any size). Points stay inside it for real content.
- Each channel's polygon is fanned from its first vertex: `(v0, v1, v2), (v0, v2, v3), …`. The fans of all channels are concatenated.
- From C: the pointer stays valid until the next `CV_get_triangle_strip_vertices` call on the same handle.

## Why a plain fill is wrong

The polygons are mostly concave: about 40% of the vertices in real content are reflex (they bend inward). A fan of a concave polygon has triangles that overlap each other and stick out of the polygon; in real content, filling them normally covers about 27% too much area. Some channels are also holes, nested inside another channel, which a plain fill draws solid.

What *is* true is that every pixel inside the mask is covered an **odd** number of times, and every pixel outside an even number of times. This is the even-odd rule, and it holds across all channels together, so holes come out right as well. Counting parity per pixel is exactly what a stencil buffer with `INVERT` does.

## Stencil-then-cover

Draw the mask in two passes. You need a stencil buffer (one bit is enough) on the render target you draw the mask into, or on the eye buffer itself.

**Pass 1 — stencil.** Draw all triangles of the frame:
- color writes and depth test/writes off;
- face culling **off**, because fan triangles have both windings;
- stencil test `ALWAYS`, pass op `INVERT`, write mask `0x01`.

Afterwards bit 0 of the stencil is 1 exactly where the mask is.

**Pass 2 — cover.** Draw a quad covering the mask area (full screen, or the frame's bounding box):
- stencil test `EQUAL` with reference `1` and read mask `0x01`;
- stencil pass op `ZERO`, which leaves the stencil clean for the next frame where it was set;
- color writes on, with whatever the mask should do: write alpha, discard the passthrough, and so on.

If the cover quad doesn't cover everything pass 1 touched, clear the stencil each frame instead of relying on `ZERO`.

### Vertex transform

Map source space to clip space (top-left origin, y down) in the vertex shader:

```glsl
// a_pos: vec2 in 2048x1024 source space
gl_Position = vec4(a_pos.x / 1024.0 - 1.0, 1.0 - a_pos.y / 512.0, 0.0, 1.0);
```

To draw the mask onto part of the screen (or one eye of a stereo layout), apply your usual viewport or model transform after mapping to `[0,1]²` with `a_pos / vec2(2048.0, 1024.0)`.

### OpenGL / GLES pseudo-code

```c
// once per frame, after fetching the vertices
float *v; size_t count;
if (!CV_get_triangle_strip_vertices(handle, frame, &v, &count)) return;
glBindBuffer(GL_ARRAY_BUFFER, vbo);
glBufferData(GL_ARRAY_BUFFER, count * sizeof(float), v, GL_STREAM_DRAW);

// pass 1: stencil parity
glEnable(GL_STENCIL_TEST);
glDisable(GL_CULL_FACE);
glDisable(GL_DEPTH_TEST);
glColorMask(GL_FALSE, GL_FALSE, GL_FALSE, GL_FALSE);
glStencilMask(0x01);
glStencilFunc(GL_ALWAYS, 0, 0x01);
glStencilOp(GL_KEEP, GL_KEEP, GL_INVERT);
glUseProgram(mask_program);                       // vertex shader above, empty fragment shader
glDrawArrays(GL_TRIANGLES, 0, (GLsizei)(count / 2));   // TRIANGLES, not TRIANGLE_STRIP

// pass 2: cover where the stencil is odd, resetting it
glColorMask(GL_TRUE, GL_TRUE, GL_TRUE, GL_TRUE);
glStencilFunc(GL_EQUAL, 1, 0x01);
glStencilOp(GL_KEEP, GL_KEEP, GL_ZERO);
glUseProgram(cover_program);                      // writes the mask's effect
draw_fullscreen_quad();
glDisable(GL_STENCIL_TEST);
```

### wgpu / Vulkan state

| | Pass 1 (stencil) | Pass 2 (cover) |
|---|---|---|
| Topology | `TriangleList` | `TriangleList` / `TriangleStrip` (the quad only) |
| Cull mode | none | any |
| Color write mask | empty | as needed |
| Stencil compare (front and back) | `Always` | `Equal`, reference 1 |
| Stencil pass op | `Invert` | `Zero` |
| Stencil read / write mask | – / `0x01` | `0x01` / `0x01` |
| Depth | test and write off | as needed |

In Vulkan this is `VK_STENCIL_OP_INVERT` / `VK_COMPARE_OP_ALWAYS` in pass 1 and `VK_COMPARE_OP_EQUAL` / `VK_STENCIL_OP_ZERO` in pass 2, with `VK_CULL_MODE_NONE`. Set both `front` and `back` stencil states; fan triangles hit both.

### Anti-aliasing

With an MSAA target the stencil is evaluated per sample, so the cover pass produces anti-aliased edges for free. The bitmap path cannot do this: it is a hard 0/255 mask, like the native one.

## Without a stencil buffer

When no stencil is available on the target, count coverage in an offscreen texture instead and test parity when sampling it:

- Render target `R8_UNORM`, cleared to 0; additive blending (`ONE, ONE`); fragment output `1.0 / 255.0`. Draw the triangle list.
- When compositing, `inside = (int(round(texel * 255.0)) & 1) == 1`.

R8 saturates at 255 overlaps, far more than real fans reach; use `R16F` with an output of `1.0` if in doubt. On desktop OpenGL, `glLogicOp(GL_XOR)` on an `R8` target works too. This costs one extra texture compared to the stencil approach, but the result can be sampled like the bitmap mask.

## Nonzero instead of even-odd

Holes are wound opposite to their outer channel, so the nonzero rule gives the same mask for real content. With stencil `INCR_WRAP` for front faces and `DECR_WRAP` for back faces, the fan's signed triangles add up to the winding number, and the cover pass tests `NOTEQUAL 0`. Even-odd (`INVERT`) is what libalphastream uses and is simpler; prefer it.

## Checklist

- [ ] List topology, `count / 2` vertices
- [ ] Culling off, stencil ops set for both faces
- [ ] All channels of a frame drawn into the same stencil before the cover pass
- [ ] Stencil reset (`ZERO` in the cover pass, or a clear) before the next frame
- [ ] Source space 2048×1024, y down
