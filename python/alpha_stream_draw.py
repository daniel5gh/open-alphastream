"""alpha_stream_draw.py

Python rasterizer for AlphaStream vector data (single frame).
Implements a 1-bit alpha mask buffer and Bresenham line drawing with
scanline fill, reflecting the logic observed in libalphastream.so:
- decode per-record polylines (absolute uint16 base + signed int8 deltas)
- draw edges via Bresenham with clipping
- finalize by scanline filling between edge intersections per row

Usage:
- See draw_frame_to_mask() for the main entry.
- See save_frame_png() to render a PNG via Pillow.

Dependencies:
- Relies on AlphaStream from alpha_stream_crypto.py for decryption/parsing.
- Requires Pillow: pip install pillow
"""
from __future__ import annotations
from typing import List, Tuple

from PIL import Image

from alpha_stream_crypto import AlphaStream

# Bit lookup table like DAT_001175d0: [1,2,4,8,16,32,64,128]
BIT_LOOKUP = bytes([1, 2, 4, 8, 16, 32, 64, 128])


def _set_pixel_bit(mask: bytearray, width: int, height: int, x: int, y: int) -> None:
    """
    Set a single pixel bit in row-major 1-bit buffer.
    Equivalent to the bit setting in VIBRE_draw_line_on_mask.
    """
    if x < 0 or y < 0 or x >= width or y >= height:
        return
    pixel_index = y * width + x
    byte_index = pixel_index >> 3
    bit_index = pixel_index & 7
    mask[byte_index] |= BIT_LOOKUP[bit_index]


def _liang_barsky_clip(x0: int, y0: int, x1: int, y1: int, width: int, height: int) -> Tuple[int, int, int, int, bool]:
    """
    Liang-Barsky line clipping to rectangle [0,width-1]x[0,height-1].
    Returns (cx0,cy0,cx1,cy1,visible).
    """
    x_min, y_min = 0, 0
    x_max, y_max = width - 1, height - 1

    dx = x1 - x0
    dy = y1 - y0
    p = [-dx, dx, -dy, dy]
    q = [x0 - x_min, x_max - x0, y0 - y_min, y_max - y0]

    u1, u2 = 0.0, 1.0
    for pi, qi in zip(p, q):
        if pi == 0:
            if qi < 0:
                return x0, y0, x1, y1, False
        else:
            t = qi / pi
            if pi < 0:
                if t > u2:
                    return x0, y0, x1, y1, False
                if t > u1:
                    u1 = t
            else:
                if t < u1:
                    return x0, y0, x1, y1, False
                if t < u2:
                    u2 = t
    cx0 = int(round(x0 + u1 * dx))
    cy0 = int(round(y0 + u1 * dy))
    cx1 = int(round(x0 + u2 * dx))
    cy1 = int(round(y0 + u2 * dy))
    return cx0, cy0, cx1, cy1, True


def _draw_line(mask: bytearray, width: int, height: int, x0: int, y0: int, x1: int, y1: int) -> None:
    """
    Bresenham line drawing with clipping, mirrors VIBRE_draw_line_on_mask behavior.
    """
    cx0, cy0, cx1, cy1, ok = _liang_barsky_clip(x0, y0, x1, y1, width, height)
    if not ok:
        return
    x0, y0, x1, y1 = cx0, cy0, cx1, cy1

    dx = abs(x1 - x0)
    dy = abs(y1 - y0)
    sx = 1 if x0 < x1 else -1
    sy = 1 if y0 < y1 else -1
    err = dx - dy

    while True:
        _set_pixel_bit(mask, width, height, x0, y0)
        if x0 == x1 and y0 == y1:
            break
        e2 = 2 * err
        if e2 > -dy:
            err -= dy
            x0 += sx
        if e2 < dx:
            err += dx
            y0 += sy


def _decode_record_to_points(record: bytes) -> List[Tuple[int, int]]:
    """
    Decode a channel record into a list of absolute (x,y) points:
    - First 4 bytes: uint16 little-endian x0, y0
    - Remaining bytes: pairs of int8 (dx, dy) accumulated from the previous point
    """
    if len(record) < 4:
        return []
    x = int.from_bytes(record[0:2], 'little', signed=False)
    y = int.from_bytes(record[2:4], 'little', signed=False)
    points = [(x, y)]
    i = 4
    while i + 1 < len(record):
        dx = int.from_bytes(record[i:i+1], 'little', signed=True)
        dy = int.from_bytes(record[i+1:i+2], 'little', signed=True)
        x += dx
        y += dy
        points.append((x, y))
        i += 2
    return points


def _scanline_fill_polygon(mask: bytearray, width: int, height: int, points: List[Tuple[int, int]]) -> None:
    """
    Simple even-odd scanline fill for a polygon described by points.
    If polyline is not closed, it will be treated as open; closing is optional.
    """
    if len(points) < 3:
        return
    # Build edges
    edges = []
    for a, b in zip(points, points[1:]):
        (x0, y0), (x1, y1) = a, b
        if y0 == y1:
            # horizontal edges can be drawn but do not contribute to scanline intersections
            _draw_line(mask, width, height, x0, y0, x1, y1)
            continue
        edges.append((x0, y0, x1, y1))
    # Optionally close polygon if endpoints match
    if points[0] != points[-1]:
        x0, y0 = points[-1]
        x1, y1 = points[0]
        if y0 != y1:
            edges.append((x0, y0, x1, y1))
        _draw_line(mask, width, height, x0, y0, x1, y1)

    # Fill by scanline
    for y in range(0, height):
        xs: List[int] = []
        for x0, y0, x1, y1 in edges:
            # Check if scanline intersects edge (upper-exclusive)
            ymin = min(y0, y1)
            ymax = max(y0, y1)
            if y < ymin or y >= ymax:
                continue
            # Intersection x using linear interpolation
            if x0 == x1:
                x_int = x0
            else:
                t = (y - y0) / (y1 - y0)
                x_int = int(round(x0 + t * (x1 - x0)))
            xs.append(x_int)
        xs.sort()
        for i in range(0, len(xs) - 1, 2):
            x_start = max(0, xs[i])
            x_end = min(width - 1, xs[i + 1])
            if x_end < x_start:
                continue
            for x in range(x_start, x_end + 1):
                _set_pixel_bit(mask, width, height, x, y)


def draw_frame_to_mask(asvr: AlphaStream, frame_index: int, width: int, height: int, fill: bool = True) -> bytes:
    """
    Decode, draw, and finalize the alpha mask for a single frame.

    Parameters
    ----------
    asvr : AlphaStream
        Initialized AlphaStream instance.
    frame_index : int
        Index of the frame to draw.
    width, height : int
        Mask dimensions.
    fill : bool
        If True, perform scanline fill after drawing edges.

    Returns
    -------
    bytes
        1-bit-per-pixel mask buffer (row-major), length ceil(width*height/8).
    """
    frame_data = asvr.get_frame_data(frame_index)
    header_words, records = asvr.parse_frame_data(frame_data)

    buf_size = (width * height + 7) // 8
    mask = bytearray(buf_size)

    for rec in records:
        pts = _decode_record_to_points(rec)
        # Draw polyline segments
        for (x0, y0), (x1, y1) in zip(pts, pts[1:]):
            _draw_line(mask, width, height, x0, y0, x1, y1)
        # Finalize by filling interior if requested
        if fill:
            _scanline_fill_polygon(mask, width, height, pts)

    return bytes(mask)


def mask_bytes_to_image(mask_bytes: bytes, width: int, height: int) -> Image.Image:
    """
    Convert packed 1-bit row-major mask to an 8-bit grayscale Pillow image.
    Note: Binary uses LSB-first bit order within bytes; Pillow's '1' mode expects MSB-first.
    To avoid bit-order mismatch, expand to 'L' (0/255) pixels.
    """
    total = width * height
    pixels = bytearray(total)
    for idx in range(total):
        b = mask_bytes[idx >> 3]
        bit = 1 << (idx & 7)  # LSB-first
        pixels[idx] = 255 if (b & bit) else 0
    img = Image.frombytes('L', (width, height), bytes(pixels))
    return img


def save_frame_png(asvr: AlphaStream, frame_index: int, width: int, height: int, out_path: str, fill: bool = True) -> None:
    """
    Render a single frame to a PNG using Pillow.
    """
    mask = draw_frame_to_mask(asvr, frame_index, width, height, fill=fill)
    img = mask_bytes_to_image(mask, width, height)
    img.save(out_path, format='PNG')


if __name__ == "__main__":
    # Example usage: draw frame 1000 for scene 74596 and save PNG
    from pathlib import Path
    scene_id = 85342
    version = b"1.5.0"
    base_url = b"pov_mask.asvr"
    data_folder = Path("test_data") / str(scene_id)
    asvr_file = data_folder / "pov_mask.asvr"

    asvr = AlphaStream(asvr_file, scene_id, version, base_url)
    # Choose mask dimensions; if unknown, pick a reasonable default or infer from data
    width, height = 1024, 1024
    frame_index = 1000

    png_path = Path(f"{scene_id}/mask_{frame_index:04d}_{width}x{height}.png")
    save_frame_png(asvr, frame_index, width, height, str(png_path), fill=True)
    print(f"Wrote PNG: {png_path}")
