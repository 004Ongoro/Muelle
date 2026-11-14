"""Draws the Muelle app icon and its size previews.

Usage: python scripts/make_icon.py
Output: assets/brand/
"""

from pathlib import Path

from PIL import Image, ImageChops, ImageDraw, ImageFilter

SIZE = 1024
SCALE = 2  # Supersampling factor for clean edges.
S = SIZE * SCALE
OUT = Path(__file__).resolve().parent.parent / "assets" / "brand"


def squircle_mask(size: int, inset: float, power: float = 5.0) -> Image.Image:
    """Superellipse, the continuous-corner shape of Apple app icons."""
    half = size / 2 - inset
    points = []
    steps = 720
    for i in range(steps):
        import math

        angle = 2 * math.pi * i / steps
        cos, sin = math.cos(angle), math.sin(angle)
        x = abs(cos) ** (2 / power) * half * (1 if cos >= 0 else -1)
        y = abs(sin) ** (2 / power) * half * (1 if sin >= 0 else -1)
        points.append((size / 2 + x, size / 2 + y))
    mask = Image.new("L", (size, size), 0)
    ImageDraw.Draw(mask).polygon(points, fill=255)
    return mask


def vertical_gradient(size: int, stops: list[tuple[float, tuple[int, int, int]]]) -> Image.Image:
    column = Image.new("RGB", (1, size))
    pixels = column.load()
    for y in range(size):
        t = y / (size - 1)
        for (t0, c0), (t1, c1) in zip(stops, stops[1:]):
            if t0 <= t <= t1:
                k = (t - t0) / (t1 - t0)
                pixels[0, y] = tuple(round(a + (b - a) * k) for a, b in zip(c0, c1))
                break
    return column.resize((size, size))


def rounded(size: tuple[int, int], radius: int, fill) -> Image.Image:
    layer = Image.new("RGBA", size, (0, 0, 0, 0))
    ImageDraw.Draw(layer).rounded_rectangle((0, 0, size[0] - 1, size[1] - 1), radius, fill=fill)
    return layer


def shadow_of(layer: Image.Image, blur: int, opacity: float) -> Image.Image:
    alpha = layer.getchannel("A").point(lambda a: round(a * opacity))
    shadow = Image.new("RGBA", layer.size, (10, 20, 60, 0))
    shadow.putalpha(alpha)
    return shadow.filter(ImageFilter.GaussianBlur(blur))


def draw_icon() -> Image.Image:
    canvas = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    mask = squircle_mask(S, inset=S * 0.085)

    # Sea at dusk: warm horizon over deep blue water.
    sky = vertical_gradient(
        S,
        [
            (0.0, (88, 86, 214)),
            (0.42, (255, 122, 140)),
            (0.60, (255, 184, 108)),
            (0.61, (0x1D, 0x6D, 0xF1)),
            (1.0, (0x1A, 0xD1, 0xFD)),
        ],
    ).convert("RGBA")
    # Soft sun glow on the horizon.
    glow = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    ImageDraw.Draw(glow).ellipse(
        (S * 0.30, S * 0.36, S * 0.70, S * 0.76), fill=(255, 236, 190, 150)
    )
    sky = Image.alpha_composite(sky, glow.filter(ImageFilter.GaussianBlur(S * 0.06)))
    canvas.paste(sky, (0, 0), mask)

    # The dock: a frosted glass bar.
    bar_w, bar_h = int(S * 0.64), int(S * 0.17)
    bar_x, bar_y = (S - bar_w) // 2, int(S * 0.62)
    bar = rounded((bar_w, bar_h), int(bar_h * 0.36), (255, 255, 255, 70))
    ImageDraw.Draw(bar).rounded_rectangle(
        (2, 2, bar_w - 3, bar_h - 3), int(bar_h * 0.36), outline=(255, 255, 255, 190), width=6
    )
    layer = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    layer.paste(bar, (bar_x, bar_y))
    canvas = Image.alpha_composite(canvas, shadow_of(layer, int(S * 0.02), 0.55))
    canvas = Image.alpha_composite(canvas, layer)

    # Three apps; the middle one magnified and lifted.
    small, big = int(S * 0.125), int(S * 0.215)
    base = bar_y + int(bar_h * 0.80)
    tiles = [
        (S * 0.5 - S * 0.215, small, [(0.0, (255, 255, 255)), (1.0, (214, 232, 255))]),
        (S * 0.5, big, [(0.0, (255, 214, 102)), (1.0, (255, 94, 126))]),
        (S * 0.5 + S * 0.215, small, [(0.0, (255, 255, 255)), (1.0, (214, 232, 255))]),
    ]
    for center, side, stops in tiles:
        tile = vertical_gradient(side, stops).convert("RGBA")
        tile_mask = squircle_mask(side, inset=0)
        tile.putalpha(tile_mask)
        # Top sheen, clipped to the tile.
        sheen = Image.new("RGBA", (side, side), (0, 0, 0, 0))
        ImageDraw.Draw(sheen).ellipse(
            (-side * 0.2, -side * 0.75, side * 1.2, side * 0.42), fill=(255, 255, 255, 70)
        )
        sheen.putalpha(ImageChops.multiply(sheen.getchannel("A"), tile_mask))
        tile = Image.alpha_composite(tile, sheen)
        layer = Image.new("RGBA", (S, S), (0, 0, 0, 0))
        layer.paste(tile, (int(center - side / 2), base - side), tile)
        canvas = Image.alpha_composite(canvas, shadow_of(layer, int(S * 0.012), 0.5))
        canvas = Image.alpha_composite(canvas, layer)

    # Glass highlight across the top of the icon.
    sheen = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    ImageDraw.Draw(sheen).ellipse((-S * 0.2, -S * 0.62, S * 1.2, S * 0.36), fill=(255, 255, 255, 46))
    sheen.putalpha(ImageChops.multiply(sheen.getchannel("A"), mask))
    canvas = Image.alpha_composite(canvas, sheen)

    return canvas.resize((SIZE, SIZE), Image.LANCZOS)


def main() -> None:
    OUT.mkdir(parents=True, exist_ok=True)
    icon = draw_icon()
    icon.save(OUT / "muelle-1024.png")
    icon.save(OUT / "muelle.ico", sizes=[(s, s) for s in (256, 128, 64, 48, 32, 24, 16)])

    # Preview on dark and light backgrounds at the sizes Windows uses.
    sizes = (256, 96, 48, 32, 16)
    sheet = Image.new("RGB", (760, 620), (32, 32, 36))
    ImageDraw.Draw(sheet).rectangle((0, 310, 760, 620), fill=(243, 243, 246))
    for row in (0, 1):
        x = 30
        for size in sizes:
            small = icon.resize((size, size), Image.LANCZOS)
            sheet.paste(small, (x, row * 310 + (310 - size) // 2), small)
            x += size + 40
    sheet.save(OUT / "muelle-preview.png")


if __name__ == "__main__":
    main()
