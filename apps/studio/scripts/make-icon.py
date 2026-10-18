"""Draw the 1024x1024 Image Studio app icon (purple rounded square, white frame, small sparkle).

Usage: .venv/Scripts/python.exe apps/studio/scripts/make-icon.py <out.png>
Then: pnpm --dir apps/studio tauri icon <out.png>
"""

from __future__ import annotations

import sys
from pathlib import Path

from PIL import Image, ImageDraw

SIZE = 1024
SCALE = 4  # draw large, then downsample for smooth edges
TOP = (0x7C, 0x4D, 0xFF)
BOTTOM = (0x8B, 0x5C, 0xF6)


def gradient(size: int) -> Image.Image:
    img = Image.new("RGBA", (size, size))
    draw = ImageDraw.Draw(img)
    for y in range(size):
        t = y / (size - 1)
        color = tuple(round(a + (b - a) * t) for a, b in zip(TOP, BOTTOM))
        draw.line([(0, y), (size, y)], fill=(*color, 255))
    return img


def sparkle(draw: ImageDraw.ImageDraw, cx: float, cy: float, r: float, fill: tuple[int, int, int, int]) -> None:
    w = r * 0.28
    draw.polygon([(cx, cy - r), (cx + w, cy - w), (cx + r, cy), (cx + w, cy + w), (cx, cy + r), (cx - w, cy + w), (cx - r, cy), (cx - w, cy - w)], fill=fill)


def make_icon() -> Image.Image:
    s = SIZE * SCALE
    margin = round(s * 0.06)
    radius = round(s * 0.2)

    mask = Image.new("L", (s, s), 0)
    ImageDraw.Draw(mask).rounded_rectangle([margin, margin, s - margin, s - margin], radius=radius, fill=255)
    icon = Image.new("RGBA", (s, s), (0, 0, 0, 0))
    icon.paste(gradient(s), (0, 0), mask)

    draw = ImageDraw.Draw(icon)
    white = (255, 255, 255, 255)
    frame = round(s * 0.26)
    stroke = round(s * 0.065)
    draw.rounded_rectangle([frame, frame, s - frame, s - frame], radius=round(s * 0.06), outline=white, width=stroke)
    sparkle(draw, s * 0.5, s * 0.5, s * 0.13, white)
    sparkle(draw, s * 0.715, s * 0.285, s * 0.07, white)
    return icon.resize((SIZE, SIZE), Image.Resampling.LANCZOS)


def main(argv: list[str]) -> int:
    if len(argv) != 2:
        print(__doc__, file=sys.stderr)
        return 2
    out = Path(argv[1])
    out.parent.mkdir(parents=True, exist_ok=True)
    make_icon().save(out)
    print(out)
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
