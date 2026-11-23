#!/usr/bin/env python3
"""Живой фон главной: бесшовная панорама неба, нарисованные облака, птица.

  python3 tools/sprites/menu_backdrop.py

art/2d/sky/pano_ring.png — панорама из Gemini (pano_c.jpeg), замкнутая в кольцо и увеличенная до высоты
экрана в ComfyUI: python3 tools/comfy/sky_pano.py --fix …, затем --up … Стык перерисован инпейнтом,
правый край продолжает левый — в игре едет влево без шва и без двоения.
art/2d/sky/pano_live.png — она же без нарисованных крыльев мельниц (python3 tools/comfy/mills.py …):
в игре их заменяют вращающиеся крылья fx/mill_sails.png, а водопад — кадры menu/waterfall.png.
art/2d/sky/clouds.png (три облака) и clouds_bank.png (длинная гряда) — на чистом зелёном: вырезаются
ключом по зелёному, каждое облако — свой файл menu/cloud_N.png.
Нужны Pillow и numpy.
"""
from pathlib import Path

import math

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


def mill_sails(size=96):
    """Крылья мельницы: четыре махи с решётчатым парусом, сверху вниз — как на панораме."""
    k = 4
    n = size * k
    c = n / 2
    r = n / 2 - 2 * k
    img = Image.new("RGBA", (n, n), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)
    # Тёплое полотно под закатный свет панорамы, рейки чуть темнее полотна — без резкой «лесенки».
    wood = (96, 66, 52, 255)
    frame = (150, 112, 90, 255)
    cloth = (236, 212, 186, 240)
    shade = (206, 180, 160, 240)

    def at(angle, along, side):
        # along — доля радиуса от втулки, side — смещение поперёк махи (доли радиуса).
        ax, ay = math.sin(angle), -math.cos(angle)
        px, py = math.cos(angle), math.sin(angle)
        return (c + (ax * along + px * side) * r, c + (ay * along + py * side) * r)

    for i in range(4):
        a = i * math.pi / 2
        inner, outer, w = 0.2, 0.98, 0.21
        quad = [at(a, inner, 0.02), at(a, outer, 0.02), at(a, outer, w), at(a, inner, w)]
        d.polygon(quad, fill=cloth if i % 2 == 0 else shade)
        # Решётка: продольные и поперечные рейки.
        mid = (0.02 + w) / 2
        d.line([at(a, inner, mid), at(a, outer, mid)], fill=frame, width=k)
        steps = 4
        for j in range(steps + 1):
            t = inner + (outer - inner) * j / steps
            d.line([at(a, t, 0.02), at(a, t, w)], fill=frame, width=k)
        d.line([at(a, inner, w), at(a, outer, w)], fill=frame, width=int(1.5 * k))
        # Маха — от втулки до конца.
        d.line([at(a, 0, 0), at(a, 1.0, 0)], fill=wood, width=int(3 * k))
    hub = 0.08 * r
    d.ellipse([c - hub, c - hub, c + hub, c + hub], fill=(70, 48, 38, 255))
    return img.filter(ImageFilter.GaussianBlur(k * 0.45)).resize((size, size), Image.Resampling.LANCZOS)

WF_BOX = (488, 508, 552, 648)   # окно кадров водопада в px панорамы
WF_WATER = (499, 541)           # столб воды по x
WF_TOP, WF_MIST = 515, 596      # кромка обрыва; ниже — вода тает в дымке
WF_FRAMES, WF_COLS = 48, 8
WF_PERIOD = 128                 # период полос по y: за цикл они сдвигаются ровно на него


def blur_wrap(a, sx, sy):
    """Гауссово размытие с переносом по y (полосы бесшовно повторяются)."""
    def kern(s):
        r = max(1, int(3 * s))
        x = np.arange(-r, r + 1)
        k = np.exp(-x * x / (2 * s * s)); return k / k.sum(), r
    k, r = kern(sy)
    p = np.concatenate([a[-r:], a, a[:r]], axis=0)
    a = sum(k[i] * p[i:i + a.shape[0]] for i in range(len(k)))
    k, r = kern(sx)
    p = np.pad(a, ((0, 0), (r, r)), mode="edge")
    return sum(k[i] * p[:, i:i + a.shape[1]] for i in range(len(k)))


def smooth(e0, e1, x):
    t = np.clip((x - e0) / (e1 - e0), 0, 1); return t * t * (3 - 2 * t)

def waterfall(pano, seed=7):
    """Кадры водопада поверх панорамы: полосы бегут вниз и ускоряются, у подножия клубится дымка.
    Цикл бесшовный: за все кадры полосы сдвигаются ровно на свой период."""
    x0, y0, x1, y1 = WF_BOX
    w, h = x1 - x0, y1 - y0
    src = np.asarray(pano.convert("RGB").crop(WF_BOX)).astype(float)
    xs = np.arange(w)[None, :] + x0
    ys = np.arange(h)[:, None] + y0
    rng = np.random.default_rng(seed)
    streak = blur_wrap(rng.standard_normal((WF_PERIOD, w)), 0.8, 7)
    streak /= np.abs(streak).max()
    mist_n = blur_wrap(rng.standard_normal((WF_PERIOD, w)), 4, 6)
    mist_n /= np.abs(mist_n).max()
    # Основа воды — исходник, размытый по вертикали: неподвижные полосы рисунка уходят.
    base = np.stack([blur_wrap(src[..., c], 0.1, 10) for c in range(3)], -1)
    water = smooth(WF_WATER[0] - 1, WF_WATER[0] + 2, xs) * (1 - smooth(WF_WATER[1] - 2, WF_WATER[1] + 1, xs))
    water = water * smooth(WF_TOP - 2, WF_TOP + 4, ys) * (1 - smooth(WF_MIST, y1 - 12, ys))
    cx, cy = (WF_WATER[0] + WF_WATER[1]) / 2, y1 - 22
    mist = np.clip(1 - (((xs - cx) / 34) ** 2 + ((ys - cy) / 26) ** 2), 0, 1) ** 1.5
    # Ускорение: внизу полосы бегут быстрее и вытягиваются.
    v = (ys - WF_TOP).clip(0) / (WF_MIST - WF_TOP)
    warp = (ys - WF_TOP) - 0.25 * (ys - WF_TOP) * v.clip(0, 1.6)
    frames = []
    for f in range(WF_FRAMES):
        t = f / WF_FRAMES
        yy = (warp - t * WF_PERIOD).astype(int) % WF_PERIOD
        n = np.take_along_axis(streak, yy, axis=0)
        lit = base + n[..., None] * np.array([46, 44, 38]) + 6
        alpha_w = water
        my = ((ys - cy) * 1 + t * 40).astype(int) % WF_PERIOD
        m = np.take_along_axis(mist_n, my, axis=0)
        alpha_m = mist * (0.35 + 0.3 * m)
        mist_rgb = np.array([236, 230, 242])
        a = alpha_w + alpha_m * (1 - alpha_w)
        rgb = (lit * alpha_w[..., None] + mist_rgb * (alpha_m * (1 - alpha_w))[..., None]) / np.maximum(a, 1e-6)[..., None]
        frames.append(np.dstack([rgb.clip(0, 255), (a * 255).clip(0, 255)]).astype(np.uint8))
    rows = (WF_FRAMES + WF_COLS - 1) // WF_COLS
    sheet = Image.new("RGBA", (w * WF_COLS, h * rows))
    for i, fr in enumerate(frames):
        sheet.paste(Image.fromarray(fr, "RGBA"), ((i % WF_COLS) * w, (i // WF_COLS) * h))
    return sheet


def main():
    sky = backdrop(Image.open(SKY / "pano_live.png"))
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
    mill_sails().save(ASSETS / "fx" / "mill_sails.png", optimize=True)
    # Кадры — от готовой панорамы игры: цвет воды совпадает с фоном пиксель в пиксель.
    waterfall(sky).save(ASSETS / "menu" / "waterfall.png", optimize=True)
    print("водопад: окно", WF_BOX, "кадров", WF_FRAMES, "в ряду", WF_COLS)


if __name__ == "__main__":
    main()
