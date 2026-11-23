#!/usr/bin/env python3
"""Живой фон главной: бесшовная панорама неба, нарисованные облака, птица.

  python3 tools/sprites/menu_backdrop.py

art/2d/sky/pano_ring.png — панорама из Gemini (pano_c.jpeg), замкнутая в кольцо и увеличенная до высоты
экрана в ComfyUI: python3 tools/comfy/sky_pano.py --fix …, затем --up … Стык перерисован инпейнтом,
правый край продолжает левый — в игре едет влево без шва и без двоения.
art/2d/sky/clouds.png (три облака) и clouds_bank.png (длинная гряда) — на чистом зелёном: вырезаются
ключом по зелёному, каждое облако — свой файл menu/cloud_N.png.
Нужны Pillow и numpy.
"""
from pathlib import Path

import numpy as np
from PIL import Image, ImageDraw, ImageFilter  # noqa: F401

ROOT = Path(__file__).resolve().parents[2]
ASSETS = ROOT / "crates" / "game" / "assets"
SKY = ROOT / "art" / "2d" / "sky"
SCREEN_H = 960


def backdrop(src):
    """Панорама на всю высоту экрана. Исходник уже замкнут в кольцо (tools/comfy/sky_pano.py), склейки нет."""
    if src.height != SCREEN_H:
        src = src.resize((round(src.width * SCREEN_H / src.height), SCREEN_H), Image.Resampling.LANCZOS)
    return src.convert("RGB").filter(ImageFilter.UnsharpMask(1.0, 35, 2))


def clouds(src):
    """Ключ по зелёному: альфа по перевесу зелёного, зелень из полупрозрачных краёв убрана."""
    a = np.asarray(src.convert("RGB")).astype(float) / 255
    r, g, b = a[..., 0], a[..., 1], a[..., 2]
    spill = g - np.maximum(r, b)
    alpha = 1 - np.clip((spill - 0.02) / 0.3, 0, 1)
    # Кайма по краю — смесь с зелёным: срезаем 3 px и смягчаем край.
    edge = Image.fromarray((alpha * 255).astype(np.uint8)).filter(ImageFilter.MinFilter(7))
    alpha = np.asarray(edge.filter(ImageFilter.GaussianBlur(1.2))).astype(float) / 255
    rgb = a.copy()
    rgb[..., 1] = np.minimum(g, (r + b) / 2 + 0.03)             # без зелёного отлива
    out = np.dstack([rgb, alpha])
    img = Image.fromarray((out * 255).clip(0, 255).astype(np.uint8), "RGBA")
    # Облака стоят рядом, не друг над другом: делим по пустым столбцам.
    solid = alpha > 0.05
    cols = solid.any(axis=0)
    pieces, x = [], 0
    while x < len(cols):
        if not cols[x]:
            x += 1
            continue
        x0 = x
        while x < len(cols) and cols[x]:
            x += 1
        if x - x0 > 60:
            rows = np.nonzero(solid[:, x0:x].any(axis=1))[0]
            pieces.append((x0, rows.min(), x, rows.max() + 1))
    return [img.crop(box) for box in pieces]


def bird():
    img = Image.new("RGBA", (64, 32), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)
    d.line([(4, 8), (18, 14), (32, 22)], fill=(60, 52, 70, 255), width=4, joint="curve")
    d.line([(60, 8), (46, 14), (32, 22)], fill=(60, 52, 70, 255), width=4, joint="curve")
    return img.filter(ImageFilter.GaussianBlur(0.6))


def main():
    sky = backdrop(Image.open(SKY / "pano_ring.png"))
    print("ширина неба:", sky.width)
    sky.save(ASSETS / "menu" / "sky_pano.jpg", quality=90)
    print("панорама", sky.size)
    i = 0
    for sheet in ("clouds.png", "clouds_bank.png"):
        for c in clouds(Image.open(SKY / sheet)):
            i += 1
            c.save(ASSETS / "menu" / f"cloud_{i}.png", optimize=True)
            print(f"облако {i}", c.size)
    bird().save(ASSETS / "fx" / "bird.png")


if __name__ == "__main__":
    main()
