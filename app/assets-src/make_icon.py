"""Draws the original Claude Talk icon: a calm terracotta squircle with a
symmetric five-bar sound wave in warm cream. Rendered at 4x and downsampled
for smooth edges."""
import sys
from PIL import Image, ImageDraw

S = 1024 * 4
img = Image.new("RGBA", (S, S), (0, 0, 0, 0))
d = ImageDraw.Draw(img)

# squircle background
pad = int(S * 0.04)
d.rounded_rectangle([pad, pad, S - pad, S - pad], radius=int(S * 0.235), fill=(217, 119, 87, 255))
# sound wave bars
cream = (250, 249, 245, 255)
heights = [0.20, 0.40, 0.58, 0.40, 0.20]
bar_w = S * 0.085
gap = S * 0.05
total = len(heights) * bar_w + (len(heights) - 1) * gap
x0 = (S - total) / 2
cy = S / 2
for i, h in enumerate(heights):
    x = x0 + i * (bar_w + gap)
    hh = S * h / 2
    d.rounded_rectangle([x, cy - hh, x + bar_w, cy + hh], radius=bar_w / 2, fill=cream)

out = img.resize((1024, 1024), Image.LANCZOS)
out.save(sys.argv[1] if len(sys.argv) > 1 else "icon.png")
