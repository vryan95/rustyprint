"""Writes the small ICC profiles that the @color-profile fixtures load.

    python tests/color/make_test_profiles.py

- gray.icc: a gray monitor profile with a linear tone curve.
- linear-rgb.icc: sRGB primaries with linear tone curves, so its colors differ from sRGB.
- cmyk.icc: a printer profile whose colors mix like naive CMYK: each ink takes away its
  share of red, green or blue, and black darkens all three.

The profiles go into the assets directory of each fixture tracker that uses them.
"""

import os
import struct

D50 = (0.9642, 1.0, 0.8249)


def s15f16(value):
    return struct.pack(">i", round(value * 65536))


def pad(data):
    return data + b"\0" * (-len(data) % 4)


def desc(text):
    ascii = text.encode("ascii") + b"\0"
    return (
        b"desc" + b"\0" * 4 + struct.pack(">I", len(ascii)) + ascii
        + struct.pack(">II", 0, 0) + struct.pack(">HB", 0, 0) + b"\0" * 67
    )


def text(value):
    return b"text" + b"\0" * 4 + value.encode("ascii") + b"\0"


def xyz(x, y, z):
    return b"XYZ " + b"\0" * 4 + s15f16(x) + s15f16(y) + s15f16(z)


def curve(gamma):
    return b"curv" + b"\0" * 4 + struct.pack(">IH", 1, round(gamma * 256))


def profile(device_class, space, pcs, tags):
    """An ICC v2.1 profile from its tags, a list of (signature, data)."""
    count = len(tags)
    offset = 128 + 4 + 12 * count
    table = struct.pack(">I", count)
    body = b""
    for signature, data in tags:
        data = pad(data)
        table += signature + struct.pack(">II", offset + len(body), len(data))
        body += data
    size = offset + len(body)
    header = (
        struct.pack(">I", size) + b"\0" * 4 + struct.pack(">I", 0x02100000)
        + device_class + space + pcs + b"\0" * 12 + b"acsp" + b"\0" * 4
        + struct.pack(">I", 0) + b"\0" * 8 + b"\0" * 8 + struct.pack(">I", 0)
        + s15f16(D50[0]) + s15f16(D50[1]) + s15f16(D50[2]) + b"\0" * 4 + b"\0" * 16
        + b"\0" * 28
    )
    assert len(header) == 128
    return header + table + body


def srgb_to_lab(r, g, b):
    """Lab (D50) of a gamma-encoded sRGB color, through the D50 sRGB colorants."""
    def linear(c):
        return c / 12.92 if c <= 0.04045 else ((c + 0.055) / 1.055) ** 2.4

    r, g, b = linear(r), linear(g), linear(b)
    x = 0.4361 * r + 0.3851 * g + 0.1431 * b
    y = 0.2225 * r + 0.7169 * g + 0.0606 * b
    z = 0.0139 * r + 0.0971 * g + 0.7141 * b

    def f(t):
        return t ** (1 / 3) if t > (6 / 29) ** 3 else t / (3 * (6 / 29) ** 2) + 4 / 29

    fx, fy, fz = f(x / D50[0]), f(y / D50[1]), f(z / D50[2])
    return 116 * fy - 16, 500 * (fx - fy), 200 * (fy - fz)


def cmyk_lut():
    """An `mft2` table from CMYK to Lab, with two grid points per ink."""
    data = b"mft2" + b"\0" * 4 + struct.pack(">BBBB", 4, 3, 2, 0)
    for row in ((1, 0, 0), (0, 1, 0), (0, 0, 1)):
        data += b"".join(s15f16(v) for v in row)
    data += struct.pack(">HH", 2, 2)
    data += struct.pack(">HH", 0, 65535) * 4
    for c in (0, 1):
        for m in (0, 1):
            for y in (0, 1):
                for k in (0, 1):
                    rgb = [(1 - ink) * (1 - k) for ink in (c, m, y)]
                    lightness, a, b = srgb_to_lab(*rgb)
                    # Legacy 16-bit Lab: L in 0..0xFF00, a and b offset by 128.
                    data += struct.pack(
                        ">HHH",
                        max(0, min(0xFF00, round(lightness / 100 * 0xFF00))),
                        max(0, min(0xFFFF, round((a + 128) * 256))),
                        max(0, min(0xFFFF, round((b + 128) * 256))),
                    )
    data += struct.pack(">HH", 0, 65535) * 3
    return data


def main():
    gray = profile(b"mntr", b"GRAY", b"XYZ ", [
        (b"desc", desc("rustyprint test gray")),
        (b"cprt", text("No copyright, use freely")),
        (b"wtpt", xyz(*D50)),
        (b"kTRC", curve(1.0)),
    ])
    rgb = profile(b"mntr", b"RGB ", b"XYZ ", [
        (b"desc", desc("rustyprint test linear RGB")),
        (b"cprt", text("No copyright, use freely")),
        (b"wtpt", xyz(*D50)),
        (b"rXYZ", xyz(0.4361, 0.2225, 0.0139)),
        (b"gXYZ", xyz(0.3851, 0.7169, 0.0971)),
        (b"bXYZ", xyz(0.1431, 0.0606, 0.7141)),
        (b"rTRC", curve(1.0)),
        (b"gTRC", curve(1.0)),
        (b"bTRC", curve(1.0)),
    ])
    cmyk = profile(b"prtr", b"CMYK", b"Lab ", [
        (b"desc", desc("rustyprint test CMYK")),
        (b"cprt", text("No copyright, use freely")),
        (b"wtpt", xyz(*D50)),
        (b"A2B0", cmyk_lut()),
        (b"A2B1", cmyk_lut()),
    ])
    root = os.path.join(os.path.dirname(__file__), "..", "fixtures")
    for tracker in ("css-at-rules", "css-values"):
        assets = os.path.join(root, tracker, "assets")
        os.makedirs(assets, exist_ok=True)
        for name, data in (("gray.icc", gray), ("linear-rgb.icc", rgb), ("cmyk.icc", cmyk)):
            with open(os.path.join(assets, name), "wb") as out:
                out.write(data)


if __name__ == "__main__":
    main()
