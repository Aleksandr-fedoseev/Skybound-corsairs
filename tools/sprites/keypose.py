#!/usr/bin/env python3
"""Ключевые позы бойца (рисунки на белом фоне, например из Gemini) → кадры для pack_sheets.py.

  python3 tools/sprites/keypose.py art/2d/kaya idle attack skill hit
  python3 tools/sprites/keypose.py art/2d/ren idle@1.25 attack@1.25 skill hit   # поза@масштаб

Масштаб — если Gemini нарисовал позу мельче или крупнее остальных (голова другого размера).

Берёт <папка>/<клип>.jpeg|png, делает фон прозрачным (белое, связанное с краем кадра; вспышки и
дым, отдельные от тела, остаются), выравнивает ноги всех поз на одну линию и кладёт по кадру в
<папка>/frames/<клип>/0000.png на общем холсте — масштаб у всех поз одинаковый. Нужны Pillow и numpy.
"""
import sys
from pathlib import Path

import numpy as np
from PIL import Image, ImageDraw, ImageFilter

root = Path(sys.argv[1])
clips = [c.split("@")[0] for c in sys.argv[2:]]
scales = {c.split("@")[0]: float(c.split("@")[1]) if "@" in c else 1.0 for c in sys.argv[2:]}
FOOT_MARGIN = 24   # ноги — на столько пикселей выше нижнего края холста
EDGE_FADE = 0.04   # эффекты, упёртые в край рисунка (дым, вихрь), гаснут на этой доле ширины


def load(clip):
    for ext in ("png", "jpeg", "jpg"):
        p = root / f"{clip}.{ext}"
        if p.exists():
            return Image.open(p).convert("RGB")
    raise SystemExit(f"нет картинки {clip} в {root}")


def mask_of(img):
    rgb = np.asarray(img).astype(np.int16)
    whiteish = (rgb.min(axis=2) > 232) & (rgb.max(axis=2) - rgb.min(axis=2) < 14)
    work = Image.fromarray(whiteish.astype(np.uint8) * 255, "L").copy()
    h, w = whiteish.shape
    arr = np.asarray(work)
    seeds = [(x, 0) for x in range(0, w, 4)] + [(x, h - 1) for x in range(0, w, 4)]
    seeds += [(0, y) for y in range(0, h, 4)] + [(w - 1, y) for y in range(0, h, 4)]
    for x, y in seeds:
        if arr[y, x] == 255:
            ImageDraw.floodfill(work, (x, y), 128)
            arr = np.asarray(work)
    outside = arr == 128
    # Замкнутые куски фона (внутри вихря, между рукой и телом): крупные области чисто белого.
    # Порог строже, чем у фона у края, и только крупные пятна — светлая одежда не страдает.
    pure = (rgb.min(axis=2) > 246) & (rgb.max(axis=2) - rgb.min(axis=2) < 8)
    k = max(9, (w // 60) | 1)
    islands = np.asarray(
        Image.fromarray(pure.astype(np.uint8) * 255, "L")
        .filter(ImageFilter.MinFilter(k)).filter(ImageFilter.MaxFilter(k))
    ) > 0
    outside |= islands
    alpha = Image.fromarray((~outside).astype(np.uint8) * 255, "L")
    # Убрать пыль, смягчить край на пиксель.
    alpha = alpha.filter(ImageFilter.MinFilter(3)).filter(ImageFilter.MaxFilter(3))
    alpha = np.asarray(alpha.filter(ImageFilter.GaussianBlur(0.7))).astype(np.float32)
    # Плавно гасить у левого, правого и верхнего края: обрезанный кадром дым не даёт прямой линии.
    n = max(2, int(w * EDGE_FADE))
    ramp = np.clip(np.arange(w) / n, 0, 1) * np.clip((w - 1 - np.arange(w)) / n, 0, 1)
    top = np.clip(np.arange(h) / n, 0, 1)
    alpha *= ramp[None, :] * top[:, None]
    return Image.fromarray(alpha.astype(np.uint8), "L")


out = []
size = None
for clip in clips:
    img = load(clip)
    # Позы, нарисованные в другом размере (тот же кадр 3:4), — к размеру первой: рост одинаковый.
    size = size or img.size
    if img.size != size:
        img = img.resize(size, Image.Resampling.LANCZOS)
    k = scales[clip]
    if k != 1.0:
        # Крупнее/мельче вокруг низа кадра (ноги остаются внизу), холст прежний, фон белый.
        big = img.resize((round(size[0] * k), round(size[1] * k)), Image.Resampling.LANCZOS)
        img = Image.new("RGB", size, (255, 255, 255))
        img.paste(big, ((size[0] - big.width) // 2, size[1] - big.height))
    alpha = mask_of(img)
    a = np.asarray(alpha) > 64
    rows = np.nonzero(a.any(axis=1))[0]
    out.append((clip, img, alpha, rows.max()))

w, h = out[0][1].size
for clip, img, alpha, foot in out:
    rgba = img.convert("RGBA")
    rgba.putalpha(alpha)
    canvas = Image.new("RGBA", (w, h), (0, 0, 0, 0))
    canvas.paste(rgba, (0, (h - FOOT_MARGIN) - foot), rgba)
    d = root / "frames" / clip
    d.mkdir(parents=True, exist_ok=True)
    for old in d.glob("*.png"):
        old.unlink()
    canvas.save(d / "0000.png")
    print(f"{clip}: ноги {foot} → {h - FOOT_MARGIN}")
