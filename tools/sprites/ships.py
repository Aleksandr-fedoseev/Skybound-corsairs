#!/usr/bin/env python3
"""Корабли верфи: рисунки из Gemini → crates/game/assets/ships/<id>/.

  python3 tools/sprites/ships.py            # все корабли
  python3 tools/sprites/ships.py lastochka  # один

art/2d/ships/<id>/: cutaway.png — разрез с обставленными комнатами; exterior.png — тот же корабль
снаружи (борт закрыт); locked.png — тот же разрез, комнаты заколочены. Все три — одна поза на чистом
зелёном фоне, 1024×1024: зелёный вырезается ключом. Рамки комнат — в data/ship.json (ships[].rooms).
art/2d/ship/scaffold.png — леса стройки на белом (общие для всех кораблей) → assets/ship/scaffold.png.
Нужны Pillow и numpy.
"""
import json
import math
import sys
from pathlib import Path

import numpy as np
from PIL import Image, ImageDraw, ImageFilter

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(Path(__file__).resolve().parent))
from keypose import mask_of  # noqa: E402

ART = ROOT / "art" / "2d" / "ships"
OUT = ROOT / "crates" / "game" / "assets" / "ships"


def key_green(img):
    """Мягкий ключ по зелёному фону: альфа по «лишнему зелёному», кайма очищена от зелёного отлива.
    Тёмно-зелёная отделка борта не страдает: у неё зелёный едва выше красного и синего."""
    a = np.asarray(img.convert("RGB")).astype(np.float32) / 255
    r, g, b = a[..., 0], a[..., 1], a[..., 2]
    spill = g - np.maximum(r, b)
    alpha = 1 - np.clip((spill - 0.18) / 0.32, 0, 1)
    # Пыль и одиночные пиксели фона внутри — прочь; край мягче на пиксель.
    m = Image.fromarray((alpha * 255).astype(np.uint8))
    m = m.filter(ImageFilter.MaxFilter(3)).filter(ImageFilter.MinFilter(3)).filter(ImageFilter.GaussianBlur(0.6))
    alpha = np.minimum(alpha, np.asarray(m).astype(np.float32) / 255 + 0.0)
    rgb = a.copy()
    rgb[..., 1] = np.minimum(g, np.maximum(r, b) + 0.06)
    out = np.dstack([rgb, alpha])
    return Image.fromarray((out * 255).clip(0, 255).astype(np.uint8), "RGBA")


# Эти корабли нарисованы носом влево, а на главной корабль летит вправо: разрез зеркалится,
# чтобы на осмотре был тот же корабль, что и на главной. Рамки в data/ship.json — уже по зеркалу.
FLIP = {"lastochka", "petrel", "falcon"}


# Нарисованные винты за кормой стираются (в px готового рисунка, уже по зеркалу): вместо них в игре
# крутятся винты из data/ship.json (ships[].propellers). Многоугольники обходят корпус — втулка с
# валом на корме остаются, на них и садится вращающийся винт.
ERASE = {
    "lastochka": [[(40, 588), (135, 588), (140, 640), (144, 700), (144, 815), (40, 815)]],
    "petrel": [
        [(80, 640), (114, 640), (116, 700), (110, 742), (80, 742)],
        [(110, 720), (134, 720), (136, 800), (130, 816), (110, 816)],
    ],
    "leviathan": [
        [(28, 474), (80, 474), (80, 590), (28, 590)],
        [(74, 604), (108, 604), (112, 640), (108, 690), (104, 720), (74, 720)],
        [(144, 724), (178, 724), (180, 770), (185, 800), (178, 838), (144, 838)],
    ],
    "falcon": [[(52, 452), (92, 452), (92, 708), (52, 708)]],
}


def erase_propellers(ship, img):
    if ship not in ERASE:
        return img
    mask = Image.new("L", img.size, 0)
    for poly in ERASE[ship]:
        ImageDraw.Draw(mask).polygon(poly, fill=255)
    a = np.asarray(img).copy()
    a[..., 3][np.asarray(mask) > 0] = 0
    # Крошки лопастей у края многоугольника: мелкие обрывки рядом со стёртым — прочь.
    near = np.asarray(mask.filter(ImageFilter.MaxFilter(31))) > 0
    for x, y in zip(*np.nonzero((a[..., 3] > 10) & near)[::-1]):
        if a[y, x, 3] > 10:
            piece = component(a[..., 3] > 10, x, y, limit=400)
            if piece is not None:
                for px, py in piece:
                    a[py, px, 3] = 0
    return Image.fromarray(a, "RGBA")


def component(solid, x, y, limit):
    """Связный кусок с точкой (x, y), если он меньше `limit` px; иначе None."""
    seen, stack = {(x, y)}, [(x, y)]
    h, w = solid.shape
    while stack:
        cx, cy = stack.pop()
        for nx, ny in ((cx + 1, cy), (cx - 1, cy), (cx, cy + 1), (cx, cy - 1)):
            if 0 <= nx < w and 0 <= ny < h and solid[ny, nx] and (nx, ny) not in seen:
                seen.add((nx, ny))
                if len(seen) >= limit:
                    return None
                stack.append((nx, ny))
    return seen


def cut(ship):
    src = ART / ship
    dst = OUT / ship
    dst.mkdir(parents=True, exist_ok=True)
    for name, out in (("cutaway", "hull"), ("exterior", "exterior"), ("locked", "locked")):
        if not (src / f"{name}.png").exists():
            print(f"{ship}: нет {name}.png — пропускаю")
            continue
        img = key_green(Image.open(src / f"{name}.png"))
        if ship in FLIP:
            img = img.transpose(Image.Transpose.FLIP_LEFT_RIGHT)
        img = img.resize((1024, 1024), Image.Resampling.LANCZOS) if img.size != (1024, 1024) else img
        img = erase_propellers(ship, img)
        rgb = img.convert("RGB").filter(ImageFilter.UnsharpMask(1.2, 50, 2))
        rgb.putalpha(img.getchannel("A"))
        rgb.save(dst / f"{out}.png", optimize=True)
        box = img.getchannel("A").point(lambda v: 255 if v > 40 else 0).getbbox()
        print(f"{ship}/{out}.png рамка корабля {box}")


def propeller(size=128):
    """Латунный трёхлопастной винт анфас; в игре он сплющен по ширине — вид сбоку — и крутится."""
    k = 4
    n = size * k
    c = n / 2
    img = Image.new("RGBA", (n, n), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)
    for i in range(3):
        a = i * 2 * math.pi / 3
        blade = []
        for t, w in ((0.1, 0.1), (0.45, 0.34), (0.8, 0.3), (0.97, 0.12)):
            r = t * (n / 2 - k)
            blade.append((c + r * math.cos(a) - w * r * math.sin(a), c + r * math.sin(a) + w * r * math.cos(a)))
        for t, w in ((0.97, -0.08), (0.8, -0.16), (0.45, -0.18), (0.1, -0.08)):
            r = t * (n / 2 - k)
            blade.append((c + r * math.cos(a) - w * r * math.sin(a), c + r * math.sin(a) + w * r * math.cos(a)))
        d.polygon(blade, fill=(226, 182, 92, 255), outline=(110, 76, 38, 255), width=2 * k)
    d.ellipse([c - 0.16 * c, c - 0.16 * c, c + 0.16 * c, c + 0.16 * c], fill=(150, 110, 52, 255),
              outline=(80, 56, 30, 255), width=3 * k)
    return img.filter(ImageFilter.GaussianBlur(k * 0.3)).resize((size, size), Image.Resampling.LANCZOS)


def scaffold():
    src = Image.open(ROOT / "art" / "2d" / "ship" / "scaffold.png").convert("RGB")
    rgba = src.convert("RGBA")
    rgba.putalpha(mask_of(src))
    rgba.crop(rgba.getbbox()).save(ROOT / "crates" / "game" / "assets" / "ship" / "scaffold.png", optimize=True)


def main():
    if not sys.argv[1:]:
        scaffold()
        propeller().save(OUT.parent / "fx" / "propeller.png", optimize=True)
    ships = sys.argv[1:] or sorted(p.name for p in ART.iterdir() if (p / "cutaway.png").exists())
    for ship in ships:
        cut(ship)


if __name__ == "__main__":
    main()
