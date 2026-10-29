"""Genera los iconos de la app. Uso: python scripts/make-icons.py  (requiere Pillow)."""
from PIL import Image, ImageDraw, ImageFilter
import math, os

S = 1024
img = Image.new("RGBA", (S, S), (0, 0, 0, 0))
# fondo: degradado grafito vertical con esquinas suaves
grad = Image.new("RGBA", (S, S))
gp = grad.load()
for y in range(S):
    t = y / (S - 1)
    c = (int(58 - 30 * t), int(60 - 31 * t), int(70 - 34 * t), 255)
    for x in range(S):
        gp[x, y] = c
mask = Image.new("L", (S, S), 0)
ImageDraw.Draw(mask).rounded_rectangle((40, 40, S - 40, S - 40), radius=230, fill=255)
img.paste(grad, (0, 0), mask)

# estrella de cuatro puntas con brillo
cx = cy = S // 2
def star(r_out, r_in, color):
    layer = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    pts = []
    for i in range(8):
        a = math.pi / 4 * i - math.pi / 2
        r = r_out if i % 2 == 0 else r_in
        pts.append((cx + r * math.cos(a), cy + r * math.sin(a)))
    ImageDraw.Draw(layer).polygon(pts, fill=color)
    return layer
glow = star(380, 70, (140, 156, 255, 200)).filter(ImageFilter.GaussianBlur(40))
img = Image.alpha_composite(img, glow)
img = Image.alpha_composite(img, star(330, 62, (186, 198, 255, 255)))

out = os.path.join(os.path.dirname(__file__), "..", "src-tauri", "icons")
os.makedirs(out, exist_ok=True)
img.save(os.path.join(out, "icon.png"))
for name, size in (("32x32.png", 32), ("128x128.png", 128), ("128x128@2x.png", 256)):
    img.resize((size, size), Image.LANCZOS).save(os.path.join(out, name))
img.save(os.path.join(out, "icon.ico"), sizes=[(16, 16), (24, 24), (32, 32), (48, 48), (64, 64), (128, 128), (256, 256)])
print("Iconos generados en", os.path.abspath(out))
