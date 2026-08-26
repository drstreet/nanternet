import math
import struct
import sys
import zlib

SIZE = 1024
SAMPLES = 2

BACKDROP_TOP = (13, 148, 136)
BACKDROP_BOTTOM = (7, 59, 76)
INK = (240, 253, 250)


def squircle(x, y):
    return abs(x) ** 5 + abs(y) ** 5 <= 0.93**5


def globe(x, y):
    radius = math.hypot(x, y)
    if radius > 0.56:
        return False
    ring = abs(radius - 0.5) < 0.052
    equator = abs(y) < 0.048 and radius < 0.5
    meridian = abs((x / 0.27) ** 2 + (y / 0.5) ** 2 - 1) < 0.14 and radius < 0.52
    upper = abs(y + 0.25) < 0.042 and radius < 0.46
    lower = abs(y - 0.25) < 0.042 and radius < 0.46
    return ring or equator or meridian or upper or lower


def cut(x, y):
    return abs(x + y - 0.02) < 0.085


def sample(x, y):
    if not squircle(x, y):
        return None
    shade = (y + 1.0) / 2.0
    backdrop = tuple(
        round(BACKDROP_TOP[channel] + (BACKDROP_BOTTOM[channel] - BACKDROP_TOP[channel]) * shade)
        for channel in range(3)
    )
    if globe(x, y) and not cut(x, y):
        return INK
    return backdrop


def render():
    rows = []
    step = 2.0 / (SIZE * SAMPLES)
    for row in range(SIZE):
        line = bytearray()
        for column in range(SIZE):
            red = green = blue = alpha = 0
            for sub_y in range(SAMPLES):
                y = -1.0 + (row * SAMPLES + sub_y + 0.5) * step
                for sub_x in range(SAMPLES):
                    x = -1.0 + (column * SAMPLES + sub_x + 0.5) * step
                    pixel = sample(x, y)
                    if pixel is None:
                        continue
                    red += pixel[0]
                    green += pixel[1]
                    blue += pixel[2]
                    alpha += 255
            taken = SAMPLES * SAMPLES
            if alpha == 0:
                line += bytes((0, 0, 0, 0))
            else:
                covered = alpha // 255
                line += bytes(
                    (red // covered, green // covered, blue // covered, alpha // taken)
                )
        rows.append(bytes(line))
    return rows


def chunk(kind, payload):
    body = kind + payload
    return struct.pack(">I", len(payload)) + body + struct.pack(">I", zlib.crc32(body))


def write(path, rows):
    raw = b"".join(b"\x00" + row for row in rows)
    png = b"\x89PNG\r\n\x1a\n"
    png += chunk(b"IHDR", struct.pack(">IIBBBBB", SIZE, SIZE, 8, 6, 0, 0, 0))
    png += chunk(b"IDAT", zlib.compress(raw, 9))
    png += chunk(b"IEND", b"")
    with open(path, "wb") as handle:
        handle.write(png)


if __name__ == "__main__":
    write(sys.argv[1] if len(sys.argv) > 1 else "icon.png", render())
