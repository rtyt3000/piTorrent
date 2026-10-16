# Builds the application icons from assets/img.png (needs Pillow):
#   assets/app.ico          - embedded in the executable (build.rs)
#   assets/app_icon_64.rgba - raw 64x64 RGBA for the window, the tray and the header
import pathlib
from PIL import Image, ImageDraw

root = pathlib.Path(__file__).resolve().parent.parent
src = Image.open(root / "assets" / "img.png").convert("RGBA")

# The source is not square: cut a square around the middle of the glyph.
side = src.width
top = max(0, min(src.height - side, round(src.height / 2 - side / 2) + 28))
square = src.crop((0, top, side, top + side))

def rounded(img: Image.Image, size: int, radius: float = 0.225) -> Image.Image:
    # Draw the mask four times larger for smooth edges.
    big = size * 4
    mask = Image.new("L", (big, big), 0)
    ImageDraw.Draw(mask).rounded_rectangle((0, 0, big - 1, big - 1), radius=big * radius, fill=255)
    mask = mask.resize((size, size), Image.LANCZOS)
    out = img.resize((size, size), Image.LANCZOS)
    out.putalpha(mask)
    return out

sizes = [16, 24, 32, 48, 64, 128, 256]
frames = [rounded(square, s) for s in sizes]
frames[-1].save(root / "assets" / "app.ico", format="ICO", sizes=[(s, s) for s in sizes], append_images=frames[:-1])
(root / "assets" / "app_icon_64.rgba").write_bytes(rounded(square, 64).tobytes())
rounded(square, 256).save(root / "target" / "icon" / "preview.png") if (root / "target" / "icon").exists() else None
print("assets/app.ico and assets/app_icon_64.rgba written")
