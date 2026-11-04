"""Generate assets/icon.png (256x256) and assets/icon.ico for MultiRoblox.

The mark is three stacked rounded windows, suggesting several instances
running side by side. Pure PIL, no external assets.
"""
from PIL import Image, ImageDraw, ImageFilter
import os, sys

OUT = os.path.join(os.path.dirname(__file__), "..", "assets")
SS = 4  # supersampling factor
N = 256 * SS

def lerp(a, b, t):
    return tuple(int(a[i] + (b[i] - a[i]) * t) for i in range(3))

def gradient(size, c1, c2, diagonal=True):
    img = Image.new("RGB", size)
    px = img.load()
    w, h = size
    for y in range(h):
        for x in range(w):
            t = ((x + y) / (w + h)) if diagonal else (y / h)
            px[x, y] = lerp(c1, c2, t)
    return img

def rounded_mask(size, radius):
    m = Image.new("L", size, 0)
    ImageDraw.Draw(m).rounded_rectangle([0, 0, size[0] - 1, size[1] - 1], radius=radius, fill=255)
    return m

def build():
    canvas = Image.new("RGBA", (N, N), (0, 0, 0, 0))
    # Background tile: deep navy to near-black gradient with rounded corners.
    bg = gradient((N, N), (30, 34, 52), (14, 16, 24)).convert("RGBA")
    bg.putalpha(rounded_mask((N, N), int(N * 0.22)))
    canvas.alpha_composite(bg)

    # Three windows, back to front, offset diagonally.
    win_w, win_h = int(N * 0.50), int(N * 0.40)
    offsets = [(0.17, 0.14), (0.25, 0.26), (0.33, 0.38)]
    colors = [((98, 84, 255), (150, 92, 255)),   # violet
              ((124, 92, 255), (186, 108, 255)),  # violet -> magenta
              ((255, 107, 129), (255, 150, 94))]  # coral -> orange (front)
    for i, ((ox, oy), (c1, c2)) in enumerate(zip(offsets, colors)):
        x0, y0 = int(N * ox), int(N * oy)
        layer = Image.new("RGBA", (N, N), (0, 0, 0, 0))
        # drop shadow
        sh = Image.new("RGBA", (N, N), (0, 0, 0, 0))
        ImageDraw.Draw(sh).rounded_rectangle([x0 + 6 * SS, y0 + 10 * SS, x0 + win_w + 6 * SS, y0 + win_h + 10 * SS],
                                             radius=int(N * 0.07), fill=(0, 0, 0, 120))
        sh = sh.filter(ImageFilter.GaussianBlur(6 * SS))
        canvas.alpha_composite(sh)
        body = gradient((win_w, win_h), c1, c2).convert("RGBA")
        body.putalpha(rounded_mask((win_w, win_h), int(N * 0.07)))
        layer.alpha_composite(body, (x0, y0))
        # title bar (slightly darker strip with three dots)
        d = ImageDraw.Draw(layer)
        bar_h = int(win_h * 0.22)
        bar = Image.new("RGBA", (win_w, bar_h), (0, 0, 0, 70))
        bar.putalpha(Image.composite(rounded_mask((win_w, win_h), int(N * 0.07)).crop((0, 0, win_w, bar_h)),
                                     Image.new("L", (win_w, bar_h), 0), Image.new("L", (win_w, bar_h), 70)))
        layer.alpha_composite(bar, (x0, y0))
        r = int(N * 0.018)
        for k in range(3):
            cx = x0 + int(win_w * 0.09) + k * int(r * 3.2)
            cy = y0 + bar_h // 2
            d.ellipse([cx - r, cy - r, cx + r, cy + r], fill=(255, 255, 255, 190))
        canvas.alpha_composite(layer)

    # Subtle inner highlight on the tile edge.
    hl = Image.new("RGBA", (N, N), (0, 0, 0, 0))
    ImageDraw.Draw(hl).rounded_rectangle([SS, SS, N - 1 - SS, N - 1 - SS], radius=int(N * 0.22),
                                         outline=(255, 255, 255, 28), width=2 * SS)
    canvas.alpha_composite(hl)

    icon = canvas.resize((256, 256), Image.LANCZOS)
    icon.save(os.path.join(OUT, "icon.png"))
    sizes = [16, 24, 32, 48, 64, 128, 256]
    icon.save(os.path.join(OUT, "icon.ico"), sizes=[(s, s) for s in sizes])
    # A tray-sized copy (32px) keeps the tray crisp without runtime resampling guesswork.
    icon.resize((32, 32), Image.LANCZOS).save(os.path.join(OUT, "icon-32.png"))
    print("wrote icon.png, icon-32.png, icon.ico")

if __name__ == "__main__":
    build()
