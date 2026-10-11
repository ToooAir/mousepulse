"""Generate assets/app.ico: a mouse outline with a battery badge on a dark rounded tile.

usage: python3 tools/gen_icon.py assets/app.ico [preview.png]

Shapes are signed distance fields in unit coordinates (0..1), so every size is
rendered natively with anti-aliased edges. Standard library only.
"""
import math
import struct
import sys
import zlib

SIZES = [16, 24, 32, 48, 64, 256]

TILE_TOP = (0x2B, 0x33, 0x45)
TILE_BOTTOM = (0x16, 0x1A, 0x24)
WHITE = (0xFF, 0xFF, 0xFF)
GREEN = (0x09, 0xBB, 0x3B)


def round_box(px, py, cx, cy, hw, hh, r):
    """Signed distance to a rounded rectangle centred at (cx, cy)."""
    qx, qy = abs(px - cx) - hw + r, abs(py - cy) - hh + r
    outside = math.hypot(max(qx, 0.0), max(qy, 0.0))
    return outside + min(max(qx, qy), 0.0) - r


def segment(px, py, ax, ay, bx, by):
    """Distance to the segment a-b."""
    abx, aby = bx - ax, by - ay
    t = max(0.0, min(1.0, ((px - ax) * abx + (py - ay) * aby) / (abx * abx + aby * aby)))
    return math.hypot(px - ax - t * abx, py - ay - t * aby)


# Mouse body: a tall rounded box, slightly left so the badge has room.
M_CX, M_CY, M_HW, M_HH = 0.45, 0.47, 0.215, 0.32
M_TOP = M_CY - M_HH
DIVIDE_Y = M_TOP + 0.26
STROKE = 0.06

# Battery badge, bottom-right, overlapping the mouse with a knockout ring.
B_CX, B_CY, B_HW, B_HH = 0.71, 0.77, 0.17, 0.10


def layers(px, py):
    """Yield (signed distance, rgb) from back to front."""
    t = py  # vertical gradient on the tile
    tile = tuple(round(a + (b - a) * t) for a, b in zip(TILE_TOP, TILE_BOTTOM))
    tile_d = round_box(px, py, 0.5, 0.5, 0.47, 0.47, 0.2)
    yield tile_d, tile

    body = round_box(px, py, M_CX, M_CY, M_HW, M_HH, M_HW)
    yield abs(body) - STROKE / 2, WHITE
    # Button split: horizontal divider plus the line between the buttons.
    inner = body < 0
    if inner:
        yield segment(px, py, M_CX - M_HW, DIVIDE_Y, M_CX + M_HW, DIVIDE_Y) - STROKE / 2.4, WHITE
        yield segment(px, py, M_CX, M_TOP, M_CX, DIVIDE_Y) - STROKE / 2.4, WHITE
    yield round_box(px, py, M_CX, M_TOP + 0.13, 0.035, 0.06, 0.035), WHITE  # wheel

    # Knock out a tile-colored ring so the badge reads as a separate sticker.
    # Clipped to the tile (max = intersection) so it never squares off the rounded corner.
    yield max(round_box(px, py, B_CX, B_CY, B_HW + 0.06, B_HH + 0.06, 0.06), tile_d), tile
    battery = round_box(px, py, B_CX - 0.015, B_CY, B_HW - 0.015, B_HH, 0.03)
    yield abs(battery) - 0.022, WHITE
    yield round_box(px, py, B_CX + B_HW + 0.01, B_CY, 0.02, 0.045, 0.01), WHITE  # nub
    yield round_box(px, py, B_CX - 0.015, B_CY, B_HW - 0.07, B_HH - 0.055, 0.01), GREEN


def render(size):
    """Return straight-alpha RGBA bytes for one size."""
    out = bytearray()
    for y in range(size):
        for x in range(size):
            px, py = (x + 0.5) / size, (y + 0.5) / size
            r = g = b = a = 0.0
            for dist, rgb in layers(px, py):
                cov = max(0.0, min(1.0, 0.5 - dist * size))  # 1px anti-aliasing band
                if cov <= 0:
                    continue
                na = cov + a * (1 - cov)
                r = (rgb[0] * cov + r * a * (1 - cov)) / na
                g = (rgb[1] * cov + g * a * (1 - cov)) / na
                b = (rgb[2] * cov + b * a * (1 - cov)) / na
                a = na
            out += bytes((round(r), round(g), round(b), round(a * 255)))
    return bytes(out)


def png(w, h, rgba):
    rows = b"".join(b"\x00" + rgba[y * w * 4:(y + 1) * w * 4] for y in range(h))

    def chunk(tag, data):
        return struct.pack(">I", len(data)) + tag + data + struct.pack(">I", zlib.crc32(tag + data))

    return (
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 6, 0, 0, 0))
        + chunk(b"IDAT", zlib.compress(rows, 9))
        + chunk(b"IEND", b"")
    )


def ico(images):
    """Pack PNG-compressed entries (supported since Windows Vista)."""
    header = struct.pack("<HHH", 0, 1, len(images))
    offset = len(header) + 16 * len(images)
    entries, blobs = b"", b""
    for size, data in images:
        dim = 0 if size >= 256 else size
        entries += struct.pack("<BBBBHHII", dim, dim, 0, 0, 1, 32, len(data), offset + len(blobs))
        blobs += data
    return header + entries + blobs


def preview(path, rendered):
    """Every size at 4x on dark and light backgrounds, for eyeballing."""
    scale, pad = 4, 8
    w = sum(s * scale + pad for s in SIZES) + pad
    h = (max(SIZES) * scale + pad) * 2 + pad
    canvas = bytearray()
    for y in range(h):
        bg = (0x20, 0x20, 0x20) if y < h // 2 else (0xF3, 0xF3, 0xF3)
        row = bytearray(bg * w)
        band_y = y - pad if y < h // 2 else y - h // 2 - pad // 2
        x0 = pad
        for s in SIZES:
            sy = band_y // scale
            if 0 <= band_y and sy < s:
                for sx in range(s):
                    i = (sy * s + sx) * 4
                    r, g, b, a = rendered[s][i:i + 4]
                    px = tuple(round(c * a / 255 + bc * (255 - a) / 255) for c, bc in zip((r, g, b), bg))
                    for k in range(scale):
                        row[(x0 + sx * scale + k) * 3:(x0 + sx * scale + k) * 3 + 3] = bytes(px)
            x0 += s * scale + pad
        canvas += bytes(row)
    rgba = bytearray()
    for i in range(0, len(canvas), 3):
        rgba += canvas[i:i + 3] + b"\xff"
    with open(path, "wb") as f:
        f.write(png(w, h, bytes(rgba)))


def main():
    rendered = {s: render(s) for s in SIZES}
    with open(sys.argv[1], "wb") as f:
        f.write(ico([(s, png(s, s, rendered[s])) for s in SIZES]))
    if len(sys.argv) > 2:
        preview(sys.argv[2], rendered)


main()
