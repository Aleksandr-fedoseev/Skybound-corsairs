#!/usr/bin/env python3
"""Текстуры эффектов умений → crates/game/assets/fx/.

  python3 tools/sprites/fx_textures.py

- tornado.png — смерч из art/2d/fx/tornado.png (рисунок на чёрном фоне): яркость → прозрачность.
- gust.png — серп порыва ветра из art/2d/fx/gust.png (тоже на чёрном).
- storm_cloud.png — грозовая туча (шум), рисуется здесь же.
- slash.png, thrust.png — разрез саблей (серп) и укол (веретено): белая сердцевина, голубое свечение.
- whirl.png — круговой взмах: кольцо-комета, голова справа, хвост гаснет по часовой стрелке.
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


BLADE_EDGE = np.array([0.45, 0.72, 1.0])


def blade_light(d, w):
    """d — расстояние до линии клинка, w — полуширина: белая сердцевина и голубой ореол."""
    core = np.clip(1 - np.abs(d) / np.maximum(w, 1e-3), 0, 1) ** 0.45
    halo = np.exp(-(d / np.maximum(w * 1.7, 1e-3)) ** 2) * 0.6 * (w > 0.5)
    a = np.maximum(core, halo)
    rgb = BLADE_EDGE[None, None, :] * (1 - core[..., None]) + core[..., None]
    return np.dstack([rgb, a])


def slash():
    """Серп разреза: дуга, тонкая на концах, толстая к началу удара (летит слева направо)."""
    w, h = 512, 200
    y, x = np.mgrid[0:h, 0:w].astype(np.float32)
    cx, cy, r0 = 256.0, 380.0, 330.0
    r = np.hypot(x - cx, y - cy)
    th = np.arctan2(x - cx, cy - y)
    span = np.radians(50)
    u = (th + span) / (2 * span)
    inside = (u >= 0) & (u <= 1)
    width = np.where(inside, 30 * np.clip(np.sin(np.pi * np.clip(u, 0, 1) ** 0.75), 0, 1) ** 0.6, 0)
    out = blade_light(r - r0, width)
    out[..., 3] *= inside
    Image.fromarray((out * 255).astype(np.uint8), "RGBA").save(OUT / "slash.png", optimize=True)


def thrust():
    """Укол: острое веретено, остриё справа."""
    w, h = 512, 128
    y, x = np.mgrid[0:h, 0:w].astype(np.float32)
    u = x / (w - 1)
    width = 20 * np.clip(np.sin(np.pi * u ** 0.55), 0, 1) ** 0.9
    out = blade_light(y - h / 2, width)
    Image.fromarray((out * 255).astype(np.uint8), "RGBA").save(OUT / "thrust.png", optimize=True)


def whirl():
    """Кольцо-комета для кругового удара: крутится против часовой — хвост тянется позади головы."""
    n = 512
    y, x = np.mgrid[0:n, 0:n].astype(np.float32)
    dx, dy = x - n / 2, n / 2 - y
    r = np.hypot(dx, dy)
    behind = np.mod(-np.arctan2(dy, dx), 2 * np.pi)     # 0 у головы, растёт назад по часовой
    u = behind / (2 * np.pi * 0.8)
    inside = u <= 1
    width = np.where(inside, 4 + 34 * np.clip(1 - u, 0, 1) ** 0.8, 0)
    out = blade_light(r - 200, width)
    # Ореол — только у кольца, без бледного квадрата вокруг головы.
    near = np.clip(1 - np.abs(r - 200) / 52, 0, 1) ** 1.5
    near *= np.clip(behind / 0.1, 0, 1) ** 0.7       # голова скруглена, без прямого среза
    out[..., 3] *= inside * np.clip(1 - u, 0, 1) ** 1.3 * near
    Image.fromarray((out * 255).astype(np.uint8), "RGBA").save(OUT / "whirl.png", optimize=True)


tornado()
gust()
whirl()
slash()
thrust()
glow()
storm_cloud()
for f in sorted(OUT.glob("*.png")):
    print(f.relative_to(ROOT), Image.open(f).size)
