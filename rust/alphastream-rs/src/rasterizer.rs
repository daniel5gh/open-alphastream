//! Polystream rasterization (R8 masks), triangle generation and image resizing.
//!
//! The mask rasterizer follows libalphastream (identical in 14.3 through 15.6, see
//! `docs/REVERSE_ENGINEERING.md`):
//! - points are scaled from the [`SOURCE_WIDTH`] x [`SOURCE_HEIGHT`] space to the output size
//!   (truncating), per channel;
//! - every channel's outline (including the closing edge) is drawn with clipped Bresenham lines;
//! - the edges of **all channels go into one edge table** that is filled once with the even-odd
//!   rule, so a channel nested inside another punches a hole;
//! - crossings are tracked in 16.16 fixed point with a +0.5 bias, i.e. rounded to the nearest
//!   pixel at each integer row, and spans are inclusive on both ends.
//!
//! The native output is a 1-bit MSB-first bitmap; ours is R8 (0 or 255), which is what a GPU
//! texture upload wants.

/// Width of the coordinate space polystream points are stored in (libalphastream constant).
pub const SOURCE_WIDTH: u32 = 2048;
/// Height of the coordinate space polystream points are stored in (libalphastream constant).
pub const SOURCE_HEIGHT: u32 = 1024;

/// Fixed-point fraction bits used for edge crossings, as in libalphastream.
const FRAC_BITS: u32 = 16;
const HALF: i64 = 1 << (FRAC_BITS - 1);

/// A non-horizontal polygon edge, active for rows `ymin..ymax`.
#[derive(Debug, Clone, Copy)]
struct Edge {
    ymin: i32,
    ymax: i32,
    /// Crossing x at the current row, 16.16 fixed point with +0.5 bias.
    x: i64,
    /// Change of `x` per row, 16.16 fixed point (truncated like the native division).
    slope: i64,
}

/// A rasterizer for polystream data.
/// Polystreams are encoded as: first 4 bytes are u16 x0, y0 (little-endian),
/// followed by pairs of i8 dx, dy deltas.
pub struct PolystreamRasterizer;

impl PolystreamRasterizer {
    /// Rasterizes a single polystream channel into an R8 alpha mask (`width * height` bytes,
    /// each 0 or 255). Use [`Self::rasterize_channels`] for a whole frame: filling channels
    /// separately would lose the holes formed by nested channels.
    pub fn rasterize(polystream: &[u8], width: u32, height: u32) -> Vec<u8> {
        Self::rasterize_channels([polystream], width, height)
    }

    /// Rasterizes all channels of a frame into one R8 alpha mask with a shared even-odd fill.
    pub fn rasterize_channels<'a>(channels: impl IntoIterator<Item = &'a [u8]>, width: u32, height: u32) -> Vec<u8> {
        let mut mask = vec![0u8; width as usize * height as usize];
        Self::rasterize_channels_into(channels, width, height, &mut mask);
        mask
    }

    /// Like [`Self::rasterize_channels`], but ORs into a caller-provided `width * height` buffer.
    pub fn rasterize_channels_into<'a>(
        channels: impl IntoIterator<Item = &'a [u8]>,
        width: u32,
        height: u32,
        mask: &mut [u8],
    ) {
        let (w, h) = (width as i64, height as i64);
        assert!(mask.len() >= (w * h) as usize, "mask buffer too small");
        if w == 0 || h == 0 {
            return;
        }
        let scale_x = width as f64 / SOURCE_WIDTH as f64;
        let scale_y = height as f64 / SOURCE_HEIGHT as f64;
        let scale = |(x, y): (i32, i32)| ((x as f64 * scale_x) as i64, (y as f64 * scale_y) as i64);

        let mut edges = Vec::new();
        for channel in channels {
            let mut points = decode_points(channel).map(scale);
            let Some(first) = points.next() else { continue };
            let mut prev = first;
            for p in points.chain(std::iter::once(first)) {
                draw_line(mask, w, h, prev, p);
                push_edge(&mut edges, w, h, prev, p);
                prev = p;
            }
        }
        fill_even_odd(&mut edges, w, h, mask);
    }

    /// Converts a polystream into triangles by fanning each polygon from its first vertex.
    ///
    /// The output is a **triangle list** (three vertices per triangle, no shared vertices) of
    /// x,y pairs in the unscaled [`SOURCE_WIDTH`] x [`SOURCE_HEIGHT`] space. Fan triangles of a
    /// concave polygon overlap and extend outside it, so they must be drawn with an even-odd
    /// stencil (invert per fragment, then cover) to get the mask; see `docs/RUST_IMPLEMENTATION.md`.
    pub fn polystream_to_triangle_strip(polystream: &[u8]) -> Vec<f32> {
        let mut points: Vec<(i32, i32)> = decode_points(polystream).collect();
        // A closed polygon (last point equals first) adds nothing to the fan
        if points.len() > 1 && points.first() == points.last() {
            points.pop();
        }
        if points.len() < 3 {
            return vec![];
        }
        let v0 = points[0];
        let mut result = Vec::with_capacity((points.len() - 2) * 6);
        for pair in points[1..].windows(2) {
            for (x, y) in [v0, pair[0], pair[1]] {
                result.push(x as f32);
                result.push(y as f32);
            }
        }
        result
    }
}

/// Decodes a polystream channel into absolute points: u16 x0, y0, then i8 (dx, dy) deltas.
/// Yields `(len >> 1) - 1` points, as libalphastream does.
fn decode_points(data: &[u8]) -> impl Iterator<Item = (i32, i32)> + '_ {
    let start = (data.len() >= 4).then(|| {
        (u16::from_le_bytes([data[0], data[1]]) as i32, u16::from_le_bytes([data[2], data[3]]) as i32)
    });
    let deltas = data.get(4..).unwrap_or_default().chunks_exact(2);
    start.into_iter().chain(deltas.scan(start, |pos, d| {
        let (x, y) = pos.as_mut()?;
        *x += d[0] as i8 as i32;
        *y += d[1] as i8 as i32;
        Some((*x, *y))
    }))
}

/// Adds the edge `p0 -> p1` to the edge table unless it is horizontal.
///
/// Like libalphastream, an edge with an endpoint outside the mask takes its slope and x from
/// the clipped endpoints (without the +0.5 bias) and extrapolates back to the original rows;
/// this only shifts crossings by rounding, but keeps the output bit-exact.
fn push_edge(edges: &mut Vec<Edge>, w: i64, h: i64, p0: (i64, i64), p1: (i64, i64)) {
    if p0.1 == p1.1 {
        return;
    }
    let inside = |(x, y): (i64, i64)| (0..w).contains(&x) && (0..h).contains(&y);
    let (mut c0, mut c1) = (p0, p1);
    let (fx0, fx1) = if inside(p0) && inside(p1) {
        ((p0.0 << FRAC_BITS) + HALF, (p1.0 << FRAC_BITS) + HALF)
    } else {
        // The native code ignores whether the clip accepted the line and uses whatever it left
        let (_, a, b) = clip_points(w, h, p0, p1);
        if a.1 != b.1 {
            (c0, c1) = (a, b);
        }
        (c0.0 << FRAC_BITS, c1.0 << FRAC_BITS)
    };
    let slope = if c1.1 != c0.1 { (fx1 - fx0) / (c1.1 - c0.1) } else { 0 };
    let (ymin, ymax, x) = if p0.1 < p1.1 {
        (p0.1, p1.1, fx0 + slope * (p0.1 - c0.1))
    } else {
        (p1.1, p0.1, fx1 + slope * (p1.1 - c1.1))
    };
    edges.push(Edge { ymin: ymin as i32, ymax: ymax as i32, x, slope });
}

/// Active-edge-table scanline fill with the even-odd rule. Each edge covers rows
/// `ymin..ymax` and is sampled at the integer row; spans are inclusive.
fn fill_even_odd(edges: &mut [Edge], w: i64, h: i64, mask: &mut [u8]) {
    if edges.len() < 2 {
        return;
    }
    edges.sort_unstable_by_key(|e| (e.ymin, e.x, e.slope));
    let y_start = edges[0].ymin.max(0);
    let y_end = edges.iter().map(|e| e.ymax).max().unwrap_or(0).min(h as i32);

    let mut active: Vec<Edge> = Vec::with_capacity(16);
    let mut next = 0;
    for y in y_start..y_end {
        active.retain(|e| e.ymax > y);
        while let Some(&e) = edges.get(next).filter(|e| e.ymin <= y) {
            next += 1;
            if e.ymax > y {
                // Edges starting above row 0 are advanced to the first visible row
                active.push(Edge { x: e.x + e.slope * (y - e.ymin) as i64, ..e });
            }
        }
        // Insertion sort: the order barely changes from one row to the next
        for i in 1..active.len() {
            let mut j = i;
            while j > 0 && active[j - 1].x > active[j].x {
                active.swap(j - 1, j);
                j -= 1;
            }
        }
        let row = &mut mask[(y as i64 * w) as usize..((y as i64 + 1) * w) as usize];
        for pair in active.chunks_exact(2) {
            let (left, right) = (pair[0].x >> FRAC_BITS, pair[1].x >> FRAC_BITS);
            if left < w && right >= 0 {
                row[left.max(0) as usize..=right.min(w - 1) as usize].fill(255);
            }
        }
        for e in &mut active {
            e.x += e.slope;
        }
    }
}

/// Draws a clipped midpoint (Bresenham) line exactly like libalphastream's outline drawing:
/// always left to right, minor-axis step only when the decision value is negative.
fn draw_line(mask: &mut [u8], w: i64, h: i64, a: (i64, i64), b: (i64, i64)) {
    let Some((p0, p1)) = clip_line(w, h, a, b) else { return };
    let ((mut x, mut y), (x1, y1)) = if p1.0 < p0.0 { (p1, p0) } else { (p0, p1) };
    let dx = x1 - x;
    let dy = (y1 - y).abs();
    let sy = if y1 < y { -1 } else { 1 };
    let mut plot = |x: i64, y: i64| {
        // The native clipper can leave a corner-crossing endpoint slightly outside; never write there
        if (0..w).contains(&x) && (0..h).contains(&y) {
            mask[(y * w + x) as usize] = 255;
        }
    };
    if dx >= dy {
        let mut d = dx - 2 * dy;
        for _ in 0..=dx {
            plot(x, y);
            if d < 0 {
                y += sy;
                d += 2 * dx;
            }
            d -= 2 * dy;
            x += 1;
        }
    } else {
        let mut d = dy - 2 * dx;
        for _ in 0..=dy {
            plot(x, y);
            if d < 0 {
                x += 1;
                d += 2 * dy;
            }
            d -= 2 * dx;
            y += sy;
        }
    }
}

/// Cohen-Sutherland style clip to `[0, w) x [0, h)` in libalphastream's order: first move
/// endpoints onto the top/bottom border, then onto the left/right border (double math, truncated).
/// Returns `None` when the line is entirely outside.
fn clip_line(w: i64, h: i64, p0: (i64, i64), p1: (i64, i64)) -> Option<((i64, i64), (i64, i64))> {
    let (accepted, p0, p1) = clip_points(w, h, p0, p1);
    accepted.then_some((p0, p1))
}

/// The clip of [`clip_line`], also returning the (partially) moved endpoints when it rejects.
fn clip_points(w: i64, h: i64, mut p0: (i64, i64), mut p1: (i64, i64)) -> (bool, (i64, i64), (i64, i64)) {
    const LEFT: u8 = 1;
    const RIGHT: u8 = 2;
    const TOP: u8 = 4;
    const BOTTOM: u8 = 8;
    let x_code = |x: i64| (x < 0) as u8 * LEFT | (x >= w) as u8 * RIGHT;
    let code = |(x, y): (i64, i64)| x_code(x) | (y < 0) as u8 * TOP | (y >= h) as u8 * BOTTOM;
    // `a + (b_num * c_num) / den`, evaluated in double and truncated like the native code
    let lerp = |a: i64, num1: i64, num2: i64, den: i64| a + ((num1 as f64 * num2 as f64) / den as f64) as i64;

    let (mut c0, mut c1) = (code(p0), code(p1));
    if c0 & c1 != 0 {
        return (false, p0, p1);
    }
    if c0 == 0 && c1 == 0 {
        return (true, p0, p1);
    }
    if c0 & (TOP | BOTTOM) != 0 {
        let ny = if c0 & BOTTOM != 0 { h - 1 } else { 0 };
        p0 = (lerp(p0.0, p1.0 - p0.0, ny - p0.1, p1.1 - p0.1), ny);
        c0 = x_code(p0.0);
    }
    if c1 & (TOP | BOTTOM) != 0 {
        let ny = if c1 & BOTTOM != 0 { h - 1 } else { 0 };
        p1 = (lerp(p1.0, ny - p1.1, p1.0 - p0.0, p1.1 - p0.1), ny);
        c1 = x_code(p1.0);
    }
    if c0 & c1 != 0 {
        return (false, p0, p1);
    }
    if c0 != 0 {
        let nx = if c0 == LEFT { 0 } else { w - 1 };
        p0 = (nx, lerp(p0.1, nx - p0.0, p1.1 - p0.1, p1.0 - p0.0));
    }
    if c1 != 0 {
        let nx = if c1 == LEFT { 0 } else { w - 1 };
        p1 = (nx, lerp(p1.1, nx - p1.0, p1.1 - p0.1, p1.0 - p0.0));
    }
    (true, p0, p1)
}

/// Resizes an R8 image using nearest-neighbor scaling.
///
/// # Arguments
/// * `input` - The input R8 image data, row-major.
/// * `in_w` - Input width.
/// * `in_h` - Input height.
/// * `out_w` - Output width.
/// * `out_h` - Output height.
///
/// # Returns
/// A Vec<u8> of the resized image.
pub fn resize_nearest_neighbor(input: &[u8], in_w: u32, in_h: u32, out_w: u32, out_h: u32) -> Vec<u8> {
    let mut output = vec![0u8; (out_w * out_h) as usize];
    for y in 0..out_h {
        for x in 0..out_w {
            // Calculate source coordinates
            let src_x = ((x as f32 * in_w as f32) / out_w as f32).floor() as u32;
            let src_y = ((y as f32 * in_h as f32) / out_h as f32).floor() as u32;
            // Clamp to bounds
            let src_x = src_x.min(in_w - 1);
            let src_y = src_y.min(in_h - 1);
            // Sample
            let src_idx = (src_y * in_w + src_x) as usize;
            let dst_idx = (y * out_w + x) as usize;
            output[dst_idx] = input[src_idx];
        }
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    /// Encodes absolute points as a polystream channel (deltas must fit in i8).
    fn channel(points: &[(u16, u16)]) -> Vec<u8> {
        let mut data = Vec::new();
        data.extend_from_slice(&points[0].0.to_le_bytes());
        data.extend_from_slice(&points[0].1.to_le_bytes());
        for w in points.windows(2) {
            data.push((w[1].0 as i32 - w[0].0 as i32) as i8 as u8);
            data.push((w[1].1 as i32 - w[0].1 as i32) as i8 as u8);
        }
        data
    }

    /// Axis-aligned square with corners (x, y) and (x + size, y + size), closed.
    fn square(x: u16, y: u16, size: u16) -> Vec<u8> {
        channel(&[(x, y), (x + size, y), (x + size, y + size), (x, y + size), (x, y)])
    }

    /// Render at native resolution so coordinates map 1:1.
    fn render(channels: &[&[u8]]) -> Vec<u8> {
        PolystreamRasterizer::rasterize_channels(channels.iter().copied(), SOURCE_WIDTH, SOURCE_HEIGHT)
    }

    fn px(mask: &[u8], x: u32, y: u32) -> u8 {
        mask[(y * SOURCE_WIDTH + x) as usize]
    }

    #[test]
    fn decode_points_counts() {
        assert_eq!(decode_points(&[1, 0, 2]).count(), 0);
        assert_eq!(decode_points(&[1, 0, 2, 0]).collect::<Vec<_>>(), vec![(1, 2)]);
        assert_eq!(decode_points(&[1, 0, 2, 0, 0xff, 3, 9]).collect::<Vec<_>>(), vec![(1, 2), (0, 5)]);
    }

    #[test]
    fn square_fill_is_inclusive() {
        let mask = render(&[&square(10, 20, 10)]);
        // Outline plus fill cover exactly x 10..=20, y 20..=30
        assert_eq!(mask.iter().filter(|&&v| v == 255).count(), 11 * 11);
        assert_eq!(px(&mask, 10, 20), 255);
        assert_eq!(px(&mask, 20, 30), 255);
        assert_eq!(px(&mask, 21, 25), 0);
        assert_eq!(px(&mask, 15, 31), 0);
    }

    #[test]
    fn nested_channel_is_a_hole() {
        let outer = square(100, 100, 100);
        let inner = channel(&[(130, 130), (130, 170), (170, 170), (170, 130), (130, 130)]);
        let mask = render(&[&outer, &inner]);
        assert_eq!(px(&mask, 110, 150), 255, "between outer and inner");
        assert_eq!(px(&mask, 150, 150), 0, "inside the hole");
        assert_eq!(px(&mask, 130, 150), 255, "hole outline is drawn");
        // A channel nested inside the hole is filled again
        let island = square(145, 145, 10);
        let mask = render(&[&outer, &inner, &island]);
        assert_eq!(px(&mask, 150, 150), 255);
        assert_eq!(px(&mask, 140, 150), 0);
    }

    #[test]
    fn right_edge_maps_to_last_column() {
        let poly = channel(&[(1950, 10), (2047, 10), (2047, 60), (1950, 60), (1950, 10)]);
        for (w, h) in [(2048, 1024), (1024, 512), (4096, 2048)] {
            let mask = PolystreamRasterizer::rasterize(&poly, w, h);
            let row = (h / 30) as usize * w as usize;
            // x = 2047 truncates to the last source column's first output pixel
            let last = 2047 * w as usize / 2048;
            assert_eq!(mask[row + last], 255, "{w}x{h}: column of x=2047 filled");
            assert_eq!(mask[row + last + 1..row + w as usize].iter().filter(|&&v| v != 0).count(), 0);
            assert_eq!(mask[row + (w as usize * 1900 / 2048)], 0, "{w}x{h}: left of the polygon empty");
        }
    }

    #[test]
    fn off_screen_part_is_clipped_not_clamped() {
        // Square extends past the right border (x up to 2100) and bottom border (y up to 1100)
        let poly = channel(&[(2000, 1000), (2100, 1000), (2100, 1100), (2000, 1100), (2000, 1000)]);
        let mask = render(&[&poly]);
        // Visible part is filled up to the borders
        assert_eq!(px(&mask, 2047, 1023), 255);
        assert_eq!(px(&mask, 2000, 1010), 255);
        // Nothing drawn outside the polygon's visible area, e.g. no clamped lines along the border
        assert_eq!(px(&mask, 1990, 1023), 0);
        assert_eq!(px(&mask, 2047, 990), 0);
        assert_eq!(mask.iter().filter(|&&v| v == 255).count(), 48 * 24);
    }

    #[test]
    fn fully_off_screen_draws_nothing() {
        let poly = channel(&[(2050, 10), (2100, 10), (2100, 60), (2050, 60), (2050, 10)]);
        assert!(render(&[&poly]).iter().all(|&v| v == 0));
    }

    #[test]
    fn degenerate_channels_draw_outline_only() {
        let dot = channel(&[(5, 5)]);
        let line = channel(&[(10, 10), (20, 10)]);
        let mask = render(&[&dot, &line]);
        assert_eq!(px(&mask, 5, 5), 255);
        assert_eq!(px(&mask, 15, 10), 255);
        assert_eq!(mask.iter().filter(|&&v| v == 255).count(), 1 + 11);
    }

    #[test]
    fn clip_line_matches_native_order() {
        // Crosses the bottom border first, then the right border
        assert_eq!(clip_line(100, 100, (50, 50), (150, 150)), Some(((50, 50), (99, 99))));
        assert_eq!(clip_line(100, 100, (-10, 50), (-1, 60)), None);
        assert_eq!(clip_line(100, 100, (-10, 50), (10, 50)), Some(((0, 50), (10, 50))));
    }

    proptest! {
        #[test]
        fn fuzz_rasterize_does_not_panic(data in proptest::collection::vec(any::<u8>(), 0..128), w in 1u32..32, h in 1u32..32) {
            let _ = PolystreamRasterizer::rasterize(&data, w, h);
        }
        #[test]
        fn fuzz_rasterize_channels_does_not_panic(
            a in proptest::collection::vec(any::<u8>(), 0..64),
            b in proptest::collection::vec(any::<u8>(), 0..64),
            w in 0u32..48, h in 0u32..48,
        ) {
            let _ = PolystreamRasterizer::rasterize_channels([a.as_slice(), b.as_slice()], w, h);
        }
        #[test]
        fn fuzz_triangle_strip_does_not_panic(data in proptest::collection::vec(any::<u8>(), 0..128)) {
            let _ = PolystreamRasterizer::polystream_to_triangle_strip(&data);
        }
    }

    #[test]
    fn test_polystream_to_triangle_strip_triangle() {
        let data = channel(&[(0, 0), (15, 0), (7, 15), (0, 0)]);
        let strip = PolystreamRasterizer::polystream_to_triangle_strip(&data);
        assert_eq!(strip, vec![0.0, 0.0, 15.0, 0.0, 7.0, 15.0]);
    }

    #[test]
    fn test_triangle_strip_square() {
        let strip = PolystreamRasterizer::polystream_to_triangle_strip(&square(0, 0, 10));
        // Triangle list of the fan: (v0, v1, v2), (v0, v2, v3)
        assert_eq!(strip, vec![0.0, 0.0, 10.0, 0.0, 10.0, 10.0, 0.0, 0.0, 10.0, 10.0, 0.0, 10.0]);
    }

    #[test]
    fn test_resize_nearest_neighbor() {
        let input = vec![0, 255, 128, 64]; // 2x2
        let output = resize_nearest_neighbor(&input, 2, 2, 1, 1);
        assert_eq!(output.len(), 1);
        assert_eq!(output[0], 0); // top-left
    }

    #[test]
    fn test_resize_upscale() {
        let input = vec![255]; // 1x1
        let output = resize_nearest_neighbor(&input, 1, 1, 2, 2);
        assert_eq!(output, vec![255, 255, 255, 255]);
    }
}
