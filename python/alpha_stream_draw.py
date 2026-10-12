"""alpha_stream_draw.py

Python rasterizer for AlphaStream vector data (single frame), mirroring libalphastream
(identical in 14.3 through 15.6, see docs/REVERSE_ENGINEERING.md):
- decode each channel into a polygon (absolute uint16 base point + signed int8 deltas)
- scale points from the 2048x1024 source space to the mask size (truncating)
- draw every polygon outline (including the closing edge) with clipped midpoint lines
- put the edges of *all* channels into one edge table and fill it once with the even-odd
  rule, so a channel nested inside another punches a hole
- crossings are kept in 16.16 fixed point with a +0.5 bias; spans are inclusive

The output is a 1-bit-per-pixel, MSB-first, row-major bitmap like the native one
(the width should be a multiple of 8, as the native row addressing assumes).

Usage:
- See draw_frame_to_mask() for the main entry.
- See save_frame_png() to render a PNG via Pillow.

Dependencies:
- Relies on AlphaStream from alpha_stream_crypto.py for decryption/parsing.
- Requires Pillow: pip install pillow
"""
from __future__ import annotations
from typing import Iterable, List, Optional, Tuple

from PIL import Image

from alpha_stream_crypto import AlphaStream

# Coordinate space of polystream points (constant in every libalphastream version)
SOURCE_WIDTH = 2048
SOURCE_HEIGHT = 1024

FRAC_BITS = 16
HALF = 1 << (FRAC_BITS - 1)

# Native byte-mask tables (DAT_001175c0 / _c8 / _d0 in 14.3): span start, span end, single pixel
START_MASK = bytes([0xFF, 0x7F, 0x3F, 0x1F, 0x0F, 0x07, 0x03, 0x01])
END_MASK = bytes([0x80, 0xC0, 0xE0, 0xF0, 0xF8, 0xFC, 0xFE, 0xFF])
BIT_MASK = bytes([0x80, 0x40, 0x20, 0x10, 0x08, 0x04, 0x02, 0x01])

Point = Tuple[int, int]
# Edge: [ymin, ymax, x (16.16 at current row), slope (16.16 per row)]
Edge = List[int]


def _set_pixel_bit(mask: bytearray, width: int, height: int, x: int, y: int) -> None:
    """Set one pixel in the row-major MSB-first 1-bit buffer."""
    if 0 <= x < width and 0 <= y < height:
        mask[(y * width) // 8 + (x >> 3)] |= BIT_MASK[x & 7]


def _fill_span(mask: bytearray, width: int, y: int, x0: int, x1: int) -> None:
    """Set pixels x0..=x1 (already clamped) of row y, byte-wise like the native fill."""
    row = (y * width) // 8
    b0, b1 = x0 >> 3, x1 >> 3
    if b0 == b1:
        mask[row + b0] |= START_MASK[x0 & 7] & END_MASK[x1 & 7]
        return
    mask[row + b0] |= START_MASK[x0 & 7]
    mask[row + b0 + 1:row + b1] = b"\xff" * (b1 - b0 - 1)
    mask[row + b1] |= END_MASK[x1 & 7]


def _trunc_div(a: int, b: int) -> int:
    """Integer division truncating toward zero, like C."""
    q = abs(a) // abs(b)
    return q if (a >= 0) == (b >= 0) else -q


def _clip_points(width: int, height: int, p0: Point, p1: Point) -> Tuple[bool, Point, Point]:
    """
    Cohen-Sutherland style clip to [0,width) x [0,height) in libalphastream's order: first move
    endpoints onto the top/bottom border, then onto the left/right border (double math, truncated).
    Returns (accepted, p0, p1); on rejection the points may already be partially moved.
    """
    LEFT, RIGHT, TOP, BOTTOM = 1, 2, 4, 8

    def x_code(x: int) -> int:
        return (LEFT if x < 0 else 0) | (RIGHT if x >= width else 0)

    def code(p: Point) -> int:
        return x_code(p[0]) | (TOP if p[1] < 0 else 0) | (BOTTOM if p[1] >= height else 0)

    def lerp(a: int, num1: int, num2: int, den: int) -> int:
        return a + int((float(num1) * float(num2)) / float(den))

    (x0, y0), (x1, y1) = p0, p1
    c0, c1 = code(p0), code(p1)
    if c0 & c1:
        return False, (x0, y0), (x1, y1)
    if c0 == 0 and c1 == 0:
        return True, (x0, y0), (x1, y1)
    if c0 & (TOP | BOTTOM):
        ny = height - 1 if c0 & BOTTOM else 0
        x0, y0 = lerp(x0, x1 - x0, ny - y0, y1 - y0), ny
        c0 = x_code(x0)
    if c1 & (TOP | BOTTOM):
        ny = height - 1 if c1 & BOTTOM else 0
        x1, y1 = lerp(x1, ny - y1, x1 - x0, y1 - y0), ny
        c1 = x_code(x1)
    if c0 & c1:
        return False, (x0, y0), (x1, y1)
    if c0:
        nx = 0 if c0 == LEFT else width - 1
        x0, y0 = nx, lerp(y0, nx - x0, y1 - y0, x1 - x0)
    if c1:
        nx = 0 if c1 == LEFT else width - 1
        x1, y1 = nx, lerp(y1, nx - x1, y1 - y0, x1 - x0)
    return True, (x0, y0), (x1, y1)


def _draw_line(mask: bytearray, width: int, height: int, a: Point, b: Point) -> None:
    """Clipped midpoint line, always drawn left to right like the native outline drawing."""
    ok, p0, p1 = _clip_points(width, height, a, b)
    if not ok:
        return
    (x, y), (x1, y1) = (p1, p0) if p1[0] < p0[0] else (p0, p1)
    dx = x1 - x
    dy = abs(y1 - y)
    sy = -1 if y1 < y else 1
    if dx >= dy:
        d = dx - 2 * dy
        for _ in range(dx + 1):
            _set_pixel_bit(mask, width, height, x, y)
            if d < 0:
                y += sy
                d += 2 * dx
            d -= 2 * dy
            x += 1
    else:
        d = dy - 2 * dx
        for _ in range(dy + 1):
            _set_pixel_bit(mask, width, height, x, y)
            if d < 0:
                x += 1
                d += 2 * dy
            d -= 2 * dx
            y += sy


def _decode_record_to_points(record: bytes) -> List[Point]:
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


def _make_edge(width: int, height: int, p0: Point, p1: Point) -> Optional[Edge]:
    """
    Build the fixed-point edge p0 -> p1 (None if horizontal). An edge with an endpoint outside
    the mask takes x and slope from the clipped endpoints without the +0.5 bias, as native does.
    """
    if p0[1] == p1[1]:
        return None

    def inside(p: Point) -> bool:
        return 0 <= p[0] < width and 0 <= p[1] < height

    c0, c1 = p0, p1
    if inside(p0) and inside(p1):
        fx0, fx1 = (p0[0] << FRAC_BITS) + HALF, (p1[0] << FRAC_BITS) + HALF
    else:
        _, a, b = _clip_points(width, height, p0, p1)  # native ignores the accept flag
        if a[1] != b[1]:
            c0, c1 = a, b
        fx0, fx1 = c0[0] << FRAC_BITS, c1[0] << FRAC_BITS
    slope = _trunc_div(fx1 - fx0, c1[1] - c0[1]) if c1[1] != c0[1] else 0
    if p0[1] < p1[1]:
        return [p0[1], p1[1], fx0 + slope * (p0[1] - c0[1]), slope]
    return [p1[1], p0[1], fx1 + slope * (p1[1] - c1[1]), slope]


def _scanline_fill(mask: bytearray, width: int, height: int, edges: List[Edge]) -> None:
    """Active-edge-table even-odd fill. Edges cover rows ymin..ymax-1, sampled at the integer row."""
    if len(edges) < 2:
        return
    edges.sort(key=lambda e: (e[0], e[2], e[3]))
    y_end = min(max(e[1] for e in edges), height)
    active: List[Edge] = []
    nxt = 0
    for y in range(max(edges[0][0], 0), y_end):
        active = [e for e in active if e[1] > y]
        while nxt < len(edges) and edges[nxt][0] <= y:
            ymin, ymax, x, slope = edges[nxt]
            nxt += 1
            if ymax > y:
                # Edges starting above row 0 are advanced to the first visible row
                active.append([ymin, ymax, x + slope * (y - ymin), slope])
        active.sort(key=lambda e: e[2])
        for left, right in zip(active[0::2], active[1::2]):
            x0, x1 = left[2] >> FRAC_BITS, right[2] >> FRAC_BITS
            if x0 < width and x1 >= 0:
                _fill_span(mask, width, y, max(x0, 0), min(x1, width - 1))
        for e in active:
            e[2] += e[3]


def rasterize_channels(records: Iterable[bytes], width: int, height: int, fill: bool = True) -> bytearray:
    """Rasterize all channel records of a frame into one packed 1-bit MSB-first mask."""
    mask = bytearray((width * height + 7) // 8)
    sx, sy = width / SOURCE_WIDTH, height / SOURCE_HEIGHT
    edges: List[Edge] = []
    for rec in records:
        pts = [(int(x * sx), int(y * sy)) for x, y in _decode_record_to_points(rec)]
        if not pts:
            continue
        # Outline including the closing edge last -> first
        for a, b in zip(pts, pts[1:] + pts[:1]):
            _draw_line(mask, width, height, a, b)
            edge = _make_edge(width, height, a, b)
            if edge:
                edges.append(edge)
    if fill:
        _scanline_fill(mask, width, height, edges)
    return mask


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
        1-bit-per-pixel MSB-first mask buffer (row-major), length ceil(width*height/8).
    """
    frame_data = asvr.get_frame_data(frame_index)
    _header_words, records = asvr.parse_frame_data(frame_data)
    return bytes(rasterize_channels(records, width, height, fill=fill))


def mask_bytes_to_image(mask_bytes: bytes, width: int, height: int) -> Image.Image:
    """Convert a packed 1-bit MSB-first row-major mask to an 8-bit grayscale Pillow image."""
    total = width * height
    pixels = bytearray(total)
    for idx in range(total):
        if mask_bytes[idx >> 3] & BIT_MASK[idx & 7]:
            pixels[idx] = 255
    return Image.frombytes('L', (width, height), bytes(pixels))


def save_frame_png(asvr: AlphaStream, frame_index: int, width: int, height: int, out_path: str, fill: bool = True) -> None:
    """
    Render a single frame to a PNG using Pillow.
    """
    mask = draw_frame_to_mask(asvr, frame_index, width, height, fill=fill)
    img = mask_bytes_to_image(mask, width, height)
    img.save(out_path, format='PNG')


if __name__ == "__main__":
    # Example usage: draw frame 1000 for scene 85342 and save PNG
    from pathlib import Path
    scene_id = 85342
    version = b"1.5.0"
    base_url = b"pov_mask.asvr"
    data_folder = Path("test_data") / str(scene_id)
    asvr_file = data_folder / "pov_mask.asvr"

    asvr = AlphaStream(asvr_file, scene_id, version, base_url)
    # Native source space is 2048x1024; any size works, a width that is a multiple of 8 keeps rows byte-aligned
    width, height = 2048, 1024
    frame_index = 1000

    png_path = Path(f"{scene_id}/mask_{frame_index:04d}_{width}x{height}.png")
    save_frame_png(asvr, frame_index, width, height, str(png_path), fill=True)
    print(f"Wrote PNG: {png_path}")
