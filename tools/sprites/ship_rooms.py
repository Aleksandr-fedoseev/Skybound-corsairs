#!/usr/bin/env python3
"""Корабль-разрез на главной: рисунки из Gemini → crates/game/assets/ship/.

  python3 tools/sprites/ship_rooms.py

art/2d/ship/: base_a.png — корабль-разрез на небе (первый облик всех комнат); up_<что>.png — тот же
рисунок, где правкой в Gemini улучшена одна комната; locked.png — правка с заколоченной комнатой;
scaffold.png — леса на белом фоне; propeller.png — пропеллер анфас на белом (крутится в игре). Рамки комнат — в data/ship.json (view.rooms, пиксели 1024).
Небо вырезается: заливка от краёв по цвету неба и крупные просветы выше корпуса.
Нужны Pillow и numpy.
"""
import json
import sys
from pathlib import Path

import numpy as np
from PIL import Image, ImageDraw, ImageFilter

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(Path(__file__).resolve().parent))
from keypose import mask_of  # noqa: E402

ART = ROOT / "art" / "2d" / "ship"
OUT = ROOT / "crates" / "game" / "assets" / "ship"
# Улучшенный облик комнаты: из какого рисунка вырезать.
UPGRADES = {"tavern": "up_tavern", "cannon_deck": "up_cannon", "hold": "up_hold", "windcatchers": "up_wind"}
LOCKED_FROM = ("locked", "training_deck")   # заколоченная комната: рисунок и чья рамка


def without_sky(img):
    a = np.asarray(img).astype(int)
    r, g, b = a[..., 0], a[..., 1], a[..., 2]
    sky = (b - r >= 4) & ((r + g + b) > 360)
    work = Image.fromarray(sky.astype(np.uint8) * 255, "L").copy()
    h, w = sky.shape
    seeds = [(x, 0) for x in range(0, w, 8)] + [(x, h - 1) for x in range(0, w, 8)]
    seeds += [(0, y) for y in range(0, h, 8)] + [(w - 1, y) for y in range(0, h, 8)]
    for x, y in seeds:
        if work.getpixel((x, y)) == 255:
            ImageDraw.floodfill(work, (x, y), 128)
    bg = np.asarray(work) == 128
    # Просветы между канатами и парусами: пятна неба выше корпуса.
    k = 7
    big = np.asarray(Image.fromarray(sky.astype(np.uint8) * 255)
                     .filter(ImageFilter.MinFilter(k)).filter(ImageFilter.MaxFilter(k))) > 0
    big[int(h * 0.45):] = False
    bg |= big & sky
    alpha = Image.fromarray((~bg * 255).astype(np.uint8)).filter(ImageFilter.MinFilter(3))
    alpha = alpha.filter(ImageFilter.MaxFilter(3)).filter(ImageFilter.GaussianBlur(0.8))
    out = img.convert("RGBA")
    out.putalpha(alpha)
    return out


def main():
    OUT.mkdir(parents=True, exist_ok=True)
    view = json.loads((ROOT / "data" / "ship.json").read_text())["view"]
    rects = {r["id"]: r["rect"] for r in view["rooms"]}
    without_sky(Image.open(ART / "base_a.png").convert("RGB")).save(OUT / "hull.png", optimize=True)
    for room, src in UPGRADES.items():
        x, y, w, h = rects[room]
        pic = without_sky(Image.open(ART / f"{src}.png").convert("RGB"))
        pic.crop((x, y, x + w, y + h)).save(OUT / f"{room}_2.png", optimize=True)
    src, room = LOCKED_FROM
    x, y, w, h = rects[room]
    Image.open(ART / f"{src}.png").convert("RGB").crop((x, y, x + w, y + h)).save(OUT / "locked.png", optimize=True)
    scaffold = Image.open(ART / "scaffold.png").convert("RGB")
    rgba = scaffold.convert("RGBA")
    rgba.putalpha(mask_of(scaffold))
    rgba.crop(rgba.getbbox()).save(OUT / "scaffold.png", optimize=True)
    # Пропеллер: квадрат вокруг центра рисунка — вращается вокруг втулки.
    prop = Image.open(ART / "propeller.png").convert("RGB")
    rgba = prop.convert("RGBA")
    rgba.putalpha(mask_of(prop))
    x0, y0, x1, y1 = rgba.getbbox()
    cx, cy = prop.width / 2, prop.height / 2
    r = max(cx - x0, x1 - cx, cy - y0, y1 - cy)
    rgba.crop((round(cx - r), round(cy - r), round(cx + r), round(cy + r))).resize((256, 256), Image.Resampling.LANCZOS).save(OUT / "propeller.png", optimize=True)
    for f in sorted(OUT.glob("*.png")):
        print(f.relative_to(ROOT), Image.open(f).size)


if __name__ == "__main__":
    main()
