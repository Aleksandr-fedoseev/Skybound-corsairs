#!/usr/bin/env python3
"""Лучи за героем на экране найма: crates/game/assets/ui/rays.png (белые, цвет даёт игра).

  python3 tools/sprites/reveal_fx.py
"""
import numpy as np
from PIL import Image, ImageFilter

SIZE = 512
RAYS = 14

y, x = np.mgrid[0:SIZE, 0:SIZE].astype(np.float32) - SIZE / 2 + 0.5
r = np.hypot(x, y) / (SIZE / 2)
a = np.arctan2(y, x)
# Лучи разной ширины: широкий-узкий по кругу, с мягкими краями.
wave = np.cos(a * RAYS) * 0.6 + np.cos(a * RAYS * 2 + 1.3) * 0.4
rays = np.clip((wave - 0.15) / 0.85, 0, 1) ** 1.5
fall = np.clip(1 - r, 0, 1) ** 1.4
core = np.clip(1 - r / 0.35, 0, 1) ** 2
alpha = np.clip(rays * fall * 0.85 + core * 0.9, 0, 1)
img = Image.new("RGBA", (SIZE, SIZE), (255, 255, 255, 0))
img.putalpha(Image.fromarray((alpha * 255).astype(np.uint8)).filter(ImageFilter.GaussianBlur(2)))
img.save("crates/game/assets/ui/rays.png")
print("crates/game/assets/ui/rays.png")
