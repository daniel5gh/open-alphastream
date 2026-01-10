// Rasterizer module for polystream rasterization and image resizing

/// A rasterizer for polystream data.
/// Polystreams are encoded as: first 4 bytes are u16 x0, y0 (little-endian),
/// followed by pairs of i8 dx, dy deltas.
pub struct PolystreamRasterizer;

impl PolystreamRasterizer {
    /// Rasterizes a polystream into an R8 alpha mask.
    /// The polystream is parsed into vertices, edges are built, and scanline
    /// even-odd fill is applied to produce the mask.
    ///
    /// # Arguments
    /// * `polystream` - The raw bytes of the polystream data.
    /// * `width` - The width of the output mask.
    /// * `height` - The height of the output mask.
    ///
    /// # Returns
    /// A Vec<u8> of size width * height, where each byte is 0 or 255.
    pub fn rasterize(polystream: &[u8], width: u32, height: u32) -> Vec<u8> {
        let points = Self::decode_polystream(polystream);
        if points.len() < 3 {
            return vec![0; (width * height) as usize];
        }
        let edges = Self::build_edges(&points);
        Self::scanline_fill(&edges, width, height)
    }

    /// Converts a polystream into a triangle strip of vertices.
    /// Parses the polystream into polygon vertices, then triangulates using
    /// fan triangulation and outputs vertices in triangle strip order.
    ///
    /// # Arguments
    /// * `polystream` - The raw bytes of the polystream data.
    ///
    /// # Returns
    /// A Vec<f32> containing x,y pairs for each vertex in the triangle strip.
    pub fn polystream_to_triangle_strip(polystream: &[u8]) -> Vec<f32> {
        let points = Self::decode_polystream(polystream);
        if points.len() < 3 {
            return vec![];
        }
        // If the polygon is closed (last point equals first), remove the duplicate
        let vertices = if points[0] == *points.last().unwrap() && points.len() > 1 {
            &points[0..points.len() - 1]
        } else {
            &points
        };
        if vertices.len() < 3 {
            return vec![];
        }
        // Fan triangulation: generate triangles v0,v1,v2; v0,v2,v3; ... v0,vn-2,vn-1
        // Output as strip: v0,v1,v2,v0,v2,v3,...,v0,vn-2,vn-1
        let mut strip = vec![];
        for i in 0..vertices.len() - 2 {
            strip.push(vertices[0]);
            strip.push(vertices[i + 1]);
            strip.push(vertices[i + 2]);
        }
        // Convert to Vec<f32> with x,y pairs
        let mut result = vec![];
        for (x, y) in strip {
            result.push(x as f32);
            result.push(y as f32);
        }
        result
    }

    /// Decodes the polystream bytes into a list of (x, y) points.
    /// First 4 bytes: u16 x0, y0 little-endian.
    /// Then pairs of i8 dx, dy, accumulated.
    fn decode_polystream(data: &[u8]) -> Vec<(i32, i32)> {
        if data.len() < 4 {
            return vec![];
        }
        let mut x = u16::from_le_bytes([data[0], data[1]]) as i32;
        let mut y = u16::from_le_bytes([data[2], data[3]]) as i32;
        let mut points = vec![(x, y)];
        let mut i = 4;
        while i + 1 < data.len() {
            let dx = data[i] as i8 as i32;
            let dy = data[i + 1] as i8 as i32;
            x += dx;
            y += dy;
            points.push((x, y));
            i += 2;
        }
        points
    }

    /// Builds a list of edges from the points.
    /// Each edge is (x0, y0, x1, y1), skipping horizontal edges.
    fn build_edges(points: &[(i32, i32)]) -> Vec<(i32, i32, i32, i32)> {
        let mut edges = Vec::new();
        for window in points.windows(2) {
            let (x0, y0) = window[0];
            let (x1, y1) = window[1];
            if y0 != y1 {
                edges.push((x0, y0, x1, y1));
            }
        }
        // Close the polygon if not already closed
        if points.len() > 1 && points[0] != points[points.len() - 1] {
            let (x0, y0) = points[points.len() - 1];
            let (x1, y1) = points[0];
            if y0 != y1 {
                edges.push((x0, y0, x1, y1));
            }
        }
        edges
    }

    /// Performs scanline even-odd fill on the edges to produce the R8 mask.
    fn scanline_fill(edges: &[(i32, i32, i32, i32)], width: u32, height: u32) -> Vec<u8> {
        let mut mask = vec![0u8; (width * height) as usize];
        for y in 0..height as i32 {
            let mut xs = Vec::new();
            for &(x0, y0, x1, y1) in edges {
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
                    let start = xs[i].max(0).min(width as i32 - 1);
                    let end = xs[i + 1].max(0).min(width as i32 - 1);
                    if start <= end {
                        for x in start..=end {
                            mask[(y as u32 * width + x as u32) as usize] = 255;
                        }
                    }
                }
            }
        }
        mask
    }
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

    #[test]
    fn test_rasterize_triangle() {
        // Triangle: (0,0), (15,0), (7,15), closed
        // x0=0, y0=0, dx=15 dy=0, dx=-8 dy=15, dx=-7 dy=-15
        let data = vec![
            0, 0, // x0=0, y0=0
            0, 0, // y0=0 (little-endian u16)
            15, 0, // dx=15, dy=0 -> (15,0)
            248, 15, // dx=-8 (248 as i8), dy=15 -> (7,15)
            249, 241, // dx=-7 (249), dy=-15 (241) -> (0,0)
        ];
        let mask = PolystreamRasterizer::rasterize(&data, 16, 16);
        assert_eq!(mask.len(), 256);
        // Check that some pixels are filled
        assert!(mask.iter().any(|&x| x == 255));
    }

    proptest! {
        #[test]
        fn fuzz_rasterize_does_not_panic(data in proptest::collection::vec(any::<u8>(), 0..128), w in 1u32..32, h in 1u32..32) {
            let _ = PolystreamRasterizer::rasterize(&data, w, h);
        }
        #[test]
        fn fuzz_triangle_strip_does_not_panic(data in proptest::collection::vec(any::<u8>(), 0..128)) {
            let _ = PolystreamRasterizer::polystream_to_triangle_strip(&data);
        }
    }

    #[test]
    fn test_polystream_to_triangle_strip_triangle() {
        // Triangle: (0,0), (15,0), (7,15), closed
        let data = vec![
            0, 0, // x0=0, y0=0
            0, 0, // y0=0 (little-endian u16)
            15, 0, // dx=15, dy=0 -> (15,0)
            248, 15, // dx=-8, dy=15 -> (7,15)
            249, 241, // dx=-7, dy=-15 -> (0,0)
        ];
        let strip = PolystreamRasterizer::polystream_to_triangle_strip(&data);
        assert_eq!(strip, vec![0.0, 0.0, 15.0, 0.0, 7.0, 15.0]);
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

    #[test]
    fn test_rasterize_square() {
        // Square: (0,0), (10,0), (10,10), (0,10), closed
        // x0=0, y0=0, dx=10 dy=0, dx=0 dy=10, dx=-10 dy=0, dx=0 dy=-10
        let data = vec![
            0, 0, // x0=0
            0, 0, // y0=0
            10, 0, // dx=10, dy=0 -> (10,0)
            0, 10, // dx=0, dy=10 -> (10,10)
            246, 0, // dx=-10, dy=0 -> (0,10)
            0, 246, // dx=0, dy=-10 -> (0,0)
        ];
        let mask = PolystreamRasterizer::rasterize(&data, 16, 16);
        assert_eq!(mask.len(), 256);
        // Check that pixels inside the square are filled, e.g., (5,5)
        let idx = 5 * 16 + 5;
        assert_eq!(mask[idx], 255);
        // Check that outside is not, e.g., (15,15)
        let idx_out = 15 * 16 + 15;
        assert_eq!(mask[idx_out], 0);
    }

    #[test]
    fn test_triangle_strip_square() {
        // Square: (0,0), (10,0), (10,10), (0,10), closed
        let data = vec![
            0, 0, // x0=0
            0, 0, // y0=0
            10, 0, // dx=10, dy=0 -> (10,0)
            0, 10, // dx=0, dy=10 -> (10,10)
            246, 0, // dx=-10, dy=0 -> (0,10)
            0, 246, // dx=0, dy=-10 -> (0,0)
        ];
        let strip = PolystreamRasterizer::polystream_to_triangle_strip(&data);
        // Fan triangulation strip: v0,v1,v2,v0,v2,v3
        assert_eq!(strip, vec![0.0, 0.0, 10.0, 0.0, 10.0, 10.0, 0.0, 0.0, 10.0, 10.0, 0.0, 10.0]);
    }
}