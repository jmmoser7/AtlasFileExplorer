"""Check native before/after fill captures; requires Python + Pillow, no app state."""
from collections import Counter
from pathlib import Path

from PIL import Image


ROOT = Path(__file__).resolve().parent
REGIONS = {
    "trim": (45, 60, 525, 235),
    "concave corner": (355, 115, 410, 180),
    "hole": (740, 90, 825, 205),
    "small curves": (635, 340, 1080, 430),
    "translucent trim": (45, 300, 525, 475),
}


def colors_in(image, rect):
    pixels = image.load()
    x0, y0, x1, y1 = rect
    return Counter(pixels[x, y] for y in range(y0, y1) for x in range(x0, x1))


def check():
    for theme in ("light", "dark"):
        before = Image.open(ROOT / f"fill-before-{theme}.png").convert("RGB")
        after = Image.open(ROOT / f"fill-after-{theme}.png").convert("RGB")
        assert before.size == after.size == (1180, 750), "Capture at 1 pixel per point"
        for name, rect in REGIONS.items():
            old_colors = colors_in(before, rect)
            new_colors = colors_in(after, rect)
            assert len(old_colors) == 2, f"{theme} {name}: baseline changed"
            coverage = sum(n for color, n in new_colors.items() if color not in old_colors)
            assert coverage > 0, f"{theme} {name}: missing antialiased boundary pixels"
            print(f"{theme} {name}: {coverage} blended edge pixels")

        # A safely inset mask spans every internal triangle, not just one sample patch.
        # The translucent interior must remain identical: seams would alter these pixels.
        bp, ap = before.load(), after.load()
        fill = bp[200, 370]
        checked = 0
        for y in range(307, 469):
            for x in range(52, 519):
                if all(bp[x + dx, y + dy] == fill
                       for dy in range(-2, 3) for dx in range(-2, 3)):
                    assert ap[x, y] == fill, f"{theme}: interior seam at {(x, y)}"
                    checked += 1
        assert checked > 50_000
        print(f"{theme}: {checked} translucent interior pixels unchanged; no seams")


if __name__ == "__main__":
    check()
