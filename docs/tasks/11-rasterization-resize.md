# Task 11 — Rasterization: Resize

## Objective
Nearest-neighbor scaling of R8 alpha masks for alphastream-rs.

## Layout
- Top-left origin
- Row-major order
- Stride: $stride = width \times 1$

## Sampling
Nearest-neighbor with indices:

$$
x' = \left\lfloor \frac{x \cdot W_{src}}{W_{dst}} \right\rfloor,\ y' = \left\lfloor \frac{y \cdot H_{src}}{H_{dst}} \right\rfloor
$$

## Output
R8 format.

## Deliverables
- Scaler function ([resize_nn()](docs/tasks/11-rasterization-resize.md:1))
- Unit tests ([test_resize_nn()](docs/tasks/11-rasterization-resize.md:1))

## Dependencies
- [docs/tasks/08-frame-cache-policy.md](docs/tasks/08-frame-cache-policy.md)

## Implementation Checklist
- Bounds checks:
  ```rust
  if x >= dst_width || y >= dst_height {
      return Err("Out of bounds");
  }
  ```
- Stride handling:
  ```rust
  let stride = src_width;
  let pixel = src_data[y_src * stride + x_src];
  ```
- Unit tests:
  ```rust
  #[test]
  fn test_resize_nn() {
      // Arrange
      let src = vec![0u8, 255u8, 128u8, 64u8]; // 2x2 example
      let mut dst = vec![0u8; 1]; // 1x1
      // Act
      resize_nn(&src, 2, 2, &mut dst, 1, 1);
      // Assert
      assert_eq!(dst[0], 0u8); // Nearest neighbor sample
  }
  ```

## Acceptance Criteria
Pixel parity against reference implementation.

## References
- [docs/RUST_IMPLEMENTATION.md](docs/RUST_IMPLEMENTATION.md)
