#!/usr/bin/env python3
"""Ключевые позы бойца (рисунки на белом фоне, например из Gemini) → кадры для pack_sheets.py.

  python3 tools/sprites/keypose.py art/2d/kaya idle attack skill hit

Берёт <папка>/<клип>.jpeg|png, делает фон прозрачным (белое, связанное с краем кадра; вспышки и
дым, отдельные от тела, остаются), выравнивает ноги всех поз на одну линию и кладёт по кадру в
<папка>/frames/<клип>/0000.png на общем холсте — масштаб у всех поз одинаковый. Нужны Pillow и numpy.
"""
import sys
from pathlib import Path

import numpy as np
from PIL import Image, ImageDraw, ImageFilter

root = Path(sys.argv[1])
clips = sys.argv[2:]
FOOT_MARGIN = 24   # ноги — на столько пикселей выше нижнего края холста


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
    alpha = Image.fromarray((~outside).astype(np.uint8) * 255, "L")
    # Убрать пыль, смягчить край на пиксель.
    alpha = alpha.filter(ImageFilter.MinFilter(3)).filter(ImageFilter.MaxFilter(3))
    return alpha.filter(ImageFilter.GaussianBlur(0.7))


out = []
for clip in clips:
    img = load(clip)
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
