#!/usr/bin/env python3
"""Корабль-разрез на главной: рисунки из Gemini → crates/game/assets/ship/.

  python3 tools/sprites/ship_rooms.py

art/2d/ship/: base_a.png — корабль-разрез на небе (первый облик всех комнат); exterior.png — он же
снаружи, с закрытым бортом; exterior_sails / exterior_bare — правки exterior: паруса распущены / голые реи; up_<что>.png — тот же
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


def sky_mask(img):
    """Точно небо: цвет неба, связанный с краем рисунка, плюс крупные просветы выше корпуса."""
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
    return bg | (big & sky)


def blur(a, r):
    """Размытие (три прохода окна 2r+1 по осям) — для float-слоёв, PIL их не размывает."""
    out = a.astype(np.float32)
    for axis in (0, 1):
        for _ in range(3):
            pad = [(0, 0)] * out.ndim
            pad[axis] = (r + 1, r)
            c = np.cumsum(np.pad(out, pad, mode="edge"), axis=axis)
            hi = np.take(c, np.arange(2 * r + 1, c.shape[axis]), axis=axis)
            lo = np.take(c, np.arange(0, c.shape[axis] - 2 * r - 1), axis=axis)
            out = (hi - lo) / (2 * r + 1)
    return out


def without_sky(img, sharpen=True):
    """Мягкая вырезка: у края и на тонких канатах альфа — по отличию от цвета неба за ними,
    цвет очищен от неба (без светлой каймы). Внутри корабля — непрозрачно."""
    src = np.asarray(img.convert("RGB")).astype(np.float32)
    bg = sky_mask(img)
    # Цвет неба везде: размазанное небо, нормированное на его долю.
    wgt = bg.astype(np.float32)[..., None]
    sky = blur(src * wgt, 12) / np.maximum(blur(wgt, 12), 1e-3)
    sure = Image.fromarray((~bg).astype(np.uint8) * 255).filter(ImageFilter.MinFilter(5))
    sure = np.asarray(sure) > 0
    dist = np.sqrt(((src - sky) ** 2).sum(axis=2))
    alpha = np.clip((dist - 12) / 70, 0, 1)
    alpha[sure] = 1
    alpha[bg & (dist < 30)] = 0
    a = alpha[..., None]
    fg = np.where(a > 0.04, (src - (1 - a) * sky) / np.maximum(a, 0.04), src)
    out = Image.fromarray(np.dstack([fg.clip(0, 255), alpha * 255]).astype(np.uint8), "RGBA")
    if sharpen:
        rgb = out.convert("RGB").filter(ImageFilter.UnsharpMask(1.6, 70, 2))
        rgb.putalpha(out.getchannel("A"))
        out = rgb
    return out


def sail_boxes(a, b, pad=6):
    """Рамки, где рисунки a и b различаются (по пятнам), — x0, y0, x1, y1."""
    diff = np.abs(a.astype(int) - b.astype(int)).max(axis=2) > 40
    m = Image.fromarray(diff.astype(np.uint8) * 255).filter(ImageFilter.MaxFilter(9)).filter(ImageFilter.MinFilter(5))
    work = m.copy()
    boxes = []
    arr = np.asarray(m) > 0
    for y, x in zip(*np.nonzero(arr[::4, ::4])):
        x, y = int(x) * 4, int(y) * 4
        if work.getpixel((x, y)) != 255:
            continue
        ImageDraw.floodfill(work, (x, y), 128)
        blob = np.asarray(work) == 128
        work = Image.fromarray(np.where(blob, 64, np.asarray(work)).astype(np.uint8)).copy()
        ys, xs = np.nonzero(blob)
        if len(xs) > 600:
            boxes.append([xs.min() - pad, ys.min() - pad, xs.max() + pad, ys.max() + pad])
    return boxes


def sails():
    """Корабль снаружи с живыми парусами. Три правки одного рисунка в Gemini: exterior (паруса
    свёрнуты), exterior_sails (распущены), exterior_bare (голые реи). Основа — голые реи; поверх
    у каждой мачты слой скатки и слой паруса (в игре парус раскатывается, скатка гаснет)."""
    names = ("exterior", "exterior_sails", "exterior_bare")
    src = {n: Image.open(ART / f"{n}.png").convert("RGB") for n in names}
    arr = {n: np.asarray(img) for n, img in src.items()}
    cut = {n: without_sky(img) for n, img in src.items()}
    cut["exterior_bare"].save(OUT / "exterior.png", optimize=True)
    boxes = sail_boxes(arr["exterior_sails"], arr["exterior_bare"]) + sail_boxes(arr["exterior"], arr["exterior_bare"])
    # Парус и его скатка — одна рамка: сливаем пересекающиеся.
    merged = []
    for b in sorted(boxes):
        for m in merged:
            if b[0] < m[2] and m[0] < b[2] and b[1] < m[3] and m[1] < b[3]:
                m[:] = [min(m[0], b[0]), min(m[1], b[1]), max(m[2], b[2]), max(m[3], b[3])]
                break
        else:
            merged.append(list(b))
    for i, (x0, y0, x1, y1) in enumerate(merged, 1):
        cut["exterior"].crop((x0, y0, x1, y1)).save(OUT / f"sail_{i}_furled.png", optimize=True)
        cut["exterior_sails"].crop((x0, y0, x1, y1)).save(OUT / f"sail_{i}_open.png", optimize=True)
        print(f"парус {i}: rect [{x0}, {y0}, {x1 - x0}, {y1 - y0}]")


def main():
    OUT.mkdir(parents=True, exist_ok=True)
    view = json.loads((ROOT / "data" / "ship.json").read_text())["view"]
    rects = {r["id"]: r["rect"] for r in view["rooms"]}
    without_sky(Image.open(ART / "base_a.png").convert("RGB")).save(OUT / "hull.png", optimize=True)
    sails()
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
