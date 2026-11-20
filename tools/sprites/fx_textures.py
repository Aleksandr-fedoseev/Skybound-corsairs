#!/usr/bin/env python3
"""Текстуры эффектов умений → crates/game/assets/fx/.

  python3 tools/sprites/fx_textures.py

- tornado.png — смерч из art/2d/fx/tornado.png (рисунок на чёрном фоне): яркость → прозрачность.
- gust.png — серп порыва ветра из art/2d/fx/gust.png (тоже на чёрном).
- storm_cloud.png — грозовая туча (шум), рисуется здесь же.
Нужны Pillow и numpy.
"""
from pathlib import Path

import numpy as np
from PIL import Image, ImageFilter

ROOT = Path(__file__).resolve().parents[2]
OUT = ROOT / "crates" / "game" / "assets" / "fx"
OUT.mkdir(parents=True, exist_ok=True)


def from_black(img):
    """Светящийся рисунок на чёрном → RGBA: прозрачность по яркости, цвет без затемнения."""
    rgb = np.asarray(img.convert("RGB")).astype(np.float32) / 255.0
    a = rgb.max(axis=2)
    a = np.clip((a - 0.06) / 0.94, 0, 1)          # чёрный фон с шумом — в ноль
    color = rgb / np.maximum(rgb.max(axis=2, keepdims=True), 1e-3)
    out = np.dstack([color, a]) * 255
    return Image.fromarray(out.astype(np.uint8), "RGBA")


def tornado():
    img = from_black(Image.open(ROOT / "art" / "2d" / "fx" / "tornado.png"))
    img = img.crop(img.getchannel("A").point(lambda v: 255 if v > 20 else 0).getbbox())
    h = 512
    img = img.resize((round(img.width * h / img.height), h), Image.Resampling.LANCZOS)
    img.save(OUT / "tornado.png", optimize=True)


def gust():
    """Серп порыва ветра из art/2d/fx/gust.png (летит вправо)."""
    img = from_black(Image.open(ROOT / "art" / "2d" / "fx" / "gust.png"))
    img = img.crop(img.getchannel("A").point(lambda v: 255 if v > 20 else 0).getbbox())
    w = 512
    img = img.resize((w, round(img.height * w / img.width)), Image.Resampling.LANCZOS)
    img.save(OUT / "gust.png", optimize=True)


def noise(w, h, rng, octaves=5):
    total = np.zeros((h, w), np.float32)
    amp, size, norm = 1.0, 4, 0.0
    for _ in range(octaves):
        grid = rng.random((size, size * w // h + 1)).astype(np.float32)
        layer = np.asarray(Image.fromarray(grid * 255).convert("F").resize((w, h), Image.Resampling.BICUBIC))
        total += layer / 255.0 * amp
        norm += amp
        amp *= 0.5
        size *= 2
    return total / norm


def storm_cloud():
    """Тяжёлая туча: густая сверху, клубы снизу, подсветка снизу сиреневым."""
    w, h = 1024, 420
    rng = np.random.default_rng(7)
    n = noise(w, h, rng)
    y = np.linspace(0, 1, h)[:, None]
    # Плотность: сверху сплошная, к низу — рваные клубы.
    dens = np.clip((n - 0.28) * 2.6 + (0.8 - y) * 1.8, 0, 1)
    # Края по бокам тоже рваные.
    x = np.linspace(-1, 1, w)[None, :]
    dens *= np.clip((1.0 - np.abs(x)) * 4.0, 0, 1)
    dens = dens ** 1.2
    shade = np.clip(n * 1.3 - 0.25, 0, 1)
    dark = np.array([0.04, 0.04, 0.09])
    lit = np.array([0.30, 0.29, 0.46])
    t = (shade * (0.4 + 0.6 * y))[..., None]
    rgb = dark * (1 - t) + lit * t
    a = np.clip(dens * 1.1, 0, 1) * 0.95
    out = np.dstack([rgb, a]) * 255
    img = Image.fromarray(out.astype(np.uint8), "RGBA").filter(ImageFilter.GaussianBlur(1.5))
    img.save(OUT / "storm_cloud.png", optimize=True)


def glow():
    """Мягкое круглое свечение: белое в центре, к краю гаснет."""
    n = 256
    y, x = np.mgrid[0:n, 0:n] / (n - 1) * 2 - 1
    r = np.sqrt(x * x + y * y)
    a = np.clip(1 - r, 0, 1) ** 2.2
    out = np.dstack([np.ones((n, n, 3)), a]) * 255
    Image.fromarray(out.astype(np.uint8), "RGBA").save(OUT / "glow.png", optimize=True)


tornado()
gust()
glow()
storm_cloud()
for f in sorted(OUT.glob("*.png")):
    print(f.relative_to(ROOT), Image.open(f).size)
