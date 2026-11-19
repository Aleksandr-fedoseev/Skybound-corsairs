#!/usr/bin/env python3
"""Готовит арт из assets/ для клиента: crates/game/assets/menu/, battle/ и map/.

Корабль и модули нарисованы на ровном сером фоне — фон вырезается заливкой от краёв,
подписи под модулями отбрасываются. Фоны обрезаются до 9:16 и уменьшаются до 1080×1920.

Запуск из корня репозитория: python3 tools/prepare_art.py
"""

from pathlib import Path

import numpy as np
from PIL import Image, ImageDraw, ImageFilter

SRC = Path("assets")
OUT = Path("crates/game/assets/menu")
BATTLE = Path("crates/game/assets/battle")
MAP = Path("crates/game/assets/map")
# Серый фон листов (около 202,202,202) и допуск.
GREY = 203
TOL = 16
# Серая область внутри рисунка крупнее стольких пикселей — тоже фон.
HOLE_MIN = 600

# Отсеки на листе ship_modules.jpg: 3×3, по строкам.
MODULES = [
    "cannon_deck", "hold", "windcatchers",
    "tavern", "training_deck", "infirmary",
    "navigation", "lookout", "boats",
]


def cut_background(im: Image.Image) -> Image.Image:
    """Прозрачным становится серый фон, связанный с краями картинки."""
    rgb = np.asarray(im.convert("RGB")).astype(int)
    grey = (np.abs(rgb - GREY) < TOL).all(axis=2)
    grey &= (rgb.max(axis=2) - rgb.min(axis=2)) < 10
    # copy(): у картинки из numpy буфер только для чтения, floodfill её не меняет.
    mask = Image.fromarray((grey * 255).astype(np.uint8)).copy()
    h, w = grey.shape
    for x, y in [(0, 0), (w - 1, 0), (0, h - 1), (w - 1, h - 1), (w // 2, 0), (w // 2, h - 1)]:
        if mask.getpixel((x, y)) == 255:
            ImageDraw.floodfill(mask, (x, y), 128)
    # Замкнутые куски фона (между парусами и канатами): крупные серые области тоже фон.
    while True:
        left = np.argwhere(np.asarray(mask) == 255)
        if len(left) == 0:
            break
        y, x = left[0]
        ImageDraw.floodfill(mask, (int(x), int(y)), 64)
        region = np.asarray(mask) == 64
        value = 128 if region.sum() > HOLE_MIN else 0
        mask = Image.fromarray(np.where(region, value, np.asarray(mask)).astype(np.uint8)).copy()
    alpha = np.where(np.asarray(mask) == 128, 0, 255).astype(np.uint8)
    # Мягкий край без серого ореола.
    a = Image.fromarray(alpha).filter(ImageFilter.MinFilter(3)).filter(ImageFilter.GaussianBlur(1))
    out = im.convert("RGBA")
    out.putalpha(a)
    return out


def main_band(im: Image.Image) -> Image.Image:
    """Оставить самую высокую сплошную полосу по вертикали (отбросить подпись)."""
    alpha = np.asarray(im)[:, :, 3] > 40
    rows = alpha.sum(axis=1) > 3
    best, start = (0, 0), None
    for y, filled in enumerate(list(rows) + [False]):
        if filled and start is None:
            start = y
        elif not filled and start is not None:
            if y - start > best[1] - best[0]:
                best = (start, y)
            start = None
    top, bottom = best
    band = im.crop((0, top, im.width, bottom))
    return band.crop(band.getbbox())


def fit(im: Image.Image, width: int) -> Image.Image:
    return im.resize((width, round(im.height * width / im.width)), Image.LANCZOS)


def background(name: str, out: Path):
    im = Image.open(SRC / name).convert("RGB")
    target_h = round(im.width * 16 / 9)
    if target_h <= im.height:
        top = (im.height - target_h) // 2
        im = im.crop((0, top, im.width, top + target_h))
    else:
        target_w = round(im.height * 9 / 16)
        left = (im.width - target_w) // 2
        im = im.crop((left, 0, left + target_w, im.height))
    out.parent.mkdir(parents=True, exist_ok=True)
    im.resize((1080, 1920), Image.LANCZOS).save(out, quality=88)


def main():
    OUT.mkdir(parents=True, exist_ok=True)
    background("first_island.jpg", OUT / "island.jpg")
    background("battle_deck.jpg", BATTLE / "deck.jpg")
    background("world_map.jpg", MAP / "world.jpg")

    for level in (1, 3, 5):
        sheet = Image.open(SRC / f"ship_lvl{level}.jpg")
        side = sheet.crop((0, 0, sheet.width // 2, sheet.height))
        ship = main_band(cut_background(side))
        fit(ship, 900).save(OUT / f"ship_lvl{level}.png")

    sheet = Image.open(SRC / "ship_modules.jpg")
    cw, ch = sheet.width // 3, sheet.height // 3
    for i, name in enumerate(MODULES):
        x, y = i % 3 * cw, i // 3 * ch
        tile = cut_background(sheet.crop((x, y, x + cw, y + ch)))
        fit(main_band(tile), 256).save(OUT / f"module_{name}.png")
    print("готово:", OUT)


if __name__ == "__main__":
    main()
