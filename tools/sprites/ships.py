#!/usr/bin/env python3
"""Корабли верфи: рисунки из Gemini → crates/game/assets/ships/<id>/.

  python3 tools/sprites/ships.py            # все корабли
  python3 tools/sprites/ships.py lastochka  # один

art/2d/ships/<id>/: cutaway.png — разрез с обставленными комнатами; exterior.png — тот же корабль
снаружи (борт закрыт); locked.png — тот же разрез, комнаты заколочены. Все три — одна поза на чистом
зелёном фоне, 1024×1024: зелёный вырезается ключом. Рамки комнат — в data/ship.json (ships[].rooms).
far.png — тот же корабль упрощённым и компактным, для главной (там он маленький): обрезается по кораблю
и дополняется полями до пропорций рамки корабля (ships[].bounds) — в игре он встаёт ровно в неё.
art/2d/ship/scaffold.png — леса стройки на белом (общие для всех кораблей) → assets/ship/scaffold.png.
Нужны Pillow и numpy.
"""
import json
import sys
from pathlib import Path

import numpy as np
from PIL import Image, ImageFilter

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


def cut(ship):
    src = ART / ship
    dst = OUT / ship
    dst.mkdir(parents=True, exist_ok=True)
    for name, out in (("cutaway", "hull"), ("exterior", "exterior"), ("locked", "locked")):
        if not (src / f"{name}.png").exists():
            print(f"{ship}: нет {name}.png — пропускаю")
            continue
        img = key_green(Image.open(src / f"{name}.png"))
        img = img.resize((1024, 1024), Image.Resampling.LANCZOS) if img.size != (1024, 1024) else img
        rgb = img.convert("RGB").filter(ImageFilter.UnsharpMask(1.2, 50, 2))
        rgb.putalpha(img.getchannel("A"))
        rgb.save(dst / f"{out}.png", optimize=True)
        box = img.getchannel("A").point(lambda v: 255 if v > 40 else 0).getbbox()
        print(f"{ship}/{out}.png рамка корабля {box}")


FAR_WIDTH = 512   # на главной корабль ~180 px — запас на плотные экраны


def far(ship):
    src = ART / ship / "far.png"
    if not src.exists():
        print(f"{ship}: нет far.png — пропускаю")
        return
    ships = json.loads((ROOT / "data" / "ship.json").read_text())["ships"]
    x0, y0, x1, y1 = next(s["bounds"] for s in ships if s["id"] == ship)
    img = key_green(Image.open(src))
    img = img.crop(img.getchannel("A").point(lambda v: 255 if v > 20 else 0).getbbox())
    # Вписать в пропорции рамки корабля, по центру.
    bw, bh = x1 - x0, y1 - y0
    k = min(bw / img.width, bh / img.height)
    w, h = round(bw / k), round(bh / k)
    canvas = Image.new("RGBA", (w, h), (0, 0, 0, 0))
    canvas.alpha_composite(img, ((w - img.width) // 2, (h - img.height) // 2))
    canvas = canvas.resize((FAR_WIDTH, round(FAR_WIDTH * h / w)), Image.Resampling.LANCZOS)
    rgb = canvas.convert("RGB").filter(ImageFilter.UnsharpMask(1.0, 40, 2))
    rgb.putalpha(canvas.getchannel("A"))
    rgb.save(OUT / ship / "far.png", optimize=True)
    print(f"{ship}/far.png {rgb.size}")


def scaffold():
    src = Image.open(ROOT / "art" / "2d" / "ship" / "scaffold.png").convert("RGB")
    rgba = src.convert("RGBA")
    rgba.putalpha(mask_of(src))
    rgba.crop(rgba.getbbox()).save(ROOT / "crates" / "game" / "assets" / "ship" / "scaffold.png", optimize=True)


def main():
    if not sys.argv[1:]:
        scaffold()
    ships = sys.argv[1:] or sorted(p.name for p in ART.iterdir() if (p / "cutaway.png").exists())
    for ship in ships:
        cut(ship)
        far(ship)


if __name__ == "__main__":
    main()
