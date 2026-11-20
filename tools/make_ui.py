#!/usr/bin/env python3
"""Детали интерфейса: crates/game/assets/ui/*.png и медальоны героев.

Пока нет рисованных деталей (промпты — .studioigor/UI_KIT.md), рисует временные
в той же палитре: дерево, латунь, пергамент. Готовая картинка с тем же именем файла
просто заменяет временную — код не меняется. Если в assets/ui_src/ лежит файл с таким
же именем, скрипт берёт его, а не рисует.

Медальоны героев: портрет из рисованной позы (units/<id>.png, первый кадр) на круге
цвета стихии и в кольце редкости.

Запуск из корня репозитория: python3 tools/make_ui.py
"""

import json
from pathlib import Path

import numpy as np
from PIL import Image, ImageDraw, ImageFilter, ImageFont

OUT = Path("crates/game/assets/ui")
SRC = Path("assets/ui_src")
UNITS = Path("crates/game/assets/units")
FONT = "crates/game/assets/fonts/NotoSans-Bold.ttf"
rng = np.random.default_rng(11)

WOOD_DARK = (58, 36, 22)
WOOD = (96, 62, 38)
BRASS = (201, 151, 63)
BRASS_DARK = (120, 82, 28)
PARCHMENT = (236, 220, 186)
INK = (70, 48, 30)

ELEMENT = {
    "wind": (115, 217, 158),
    "lightning": (178, 140, 255),
    "powder": (255, 153, 71),
    "steel": (158, 173, 191),
    "mist": (204, 224, 250),
}
RARITY = {"R": (180, 189, 204), "SR": (178, 140, 255), "SSR": (242, 193, 78)}


def noise(w, h, blur=1.5, scale=1.0):
    n = rng.normal(0, 1, (h, w))
    img = Image.fromarray(((n - n.min()) / (np.ptp(n) + 1e-9) * 255).astype(np.uint8))
    return np.asarray(img.filter(ImageFilter.GaussianBlur(blur))).astype(float) / 255 * scale


def wood(w, h):
    """Доски: вертикальные полосы-волокна и стыки."""
    grain = noise(w, h * 4, 2).reshape(h * 4, w)[::4]
    stripes = np.sin(np.linspace(0, 40, w))[None, :] * 0.08
    base = np.array(WOOD, float)[None, None, :]
    dark = np.array(WOOD_DARK, float)[None, None, :]
    t = np.clip(grain * 0.8 + stripes + 0.2, 0, 1)[:, :, None]
    rgb = dark * (1 - t) + base * t
    for x in range(0, w, 48):
        rgb[:, x : x + 2] *= 0.55
    return rgb


def finish(rgb, alpha=None):
    rgb = np.clip(rgb, 0, 255).astype(np.uint8)
    im = Image.fromarray(rgb, "RGB").convert("RGBA")
    if alpha is not None:
        im.putalpha(Image.fromarray(alpha.astype(np.uint8)))
    return im


def rounded_mask(w, h, r):
    m = Image.new("L", (w, h), 0)
    ImageDraw.Draw(m).rounded_rectangle((0, 0, w - 1, h - 1), r, fill=255)
    return m


def brass_border(im, width, r):
    d = ImageDraw.Draw(im)
    w, h = im.size
    d.rounded_rectangle((1, 1, w - 2, h - 2), r, outline=BRASS_DARK, width=width + 2)
    d.rounded_rectangle((3, 3, w - 4, h - 4), r - 2, outline=BRASS, width=width)
    d.rounded_rectangle((3 + width, 3 + width, w - 4 - width, h - 4 - width), max(r - width, 2), outline=(240, 205, 120), width=1)
    for cx, cy in [(12, 12), (w - 13, 12), (12, h - 13), (w - 13, h - 13)]:
        d.ellipse((cx - 5, cy - 5, cx + 5, cy + 5), fill=BRASS_DARK)
        d.ellipse((cx - 3, cy - 4, cx + 3, cy + 2), fill=(250, 220, 140))
    return im


def panel():
    w = h = 192
    im = finish(wood(w, h))
    im.putalpha(rounded_mask(w, h, 18))
    return brass_border(im, 4, 18)


def parchment(w=128, h=128):
    n = noise(w, h, 3)
    base = np.array(PARCHMENT, float)
    rgb = base[None, None, :] * (0.88 + n[:, :, None] * 0.18)
    yy, xx = np.mgrid[0:h, 0:w]
    edge = np.minimum.reduce([xx, yy, w - 1 - xx, h - 1 - yy]).astype(float)
    rgb *= (0.78 + 0.22 * np.clip(edge / 14, 0, 1))[:, :, None]
    im = finish(rgb)
    im.putalpha(rounded_mask(w, h, 12))
    d = ImageDraw.Draw(im)
    d.rounded_rectangle((1, 1, w - 2, h - 2), 12, outline=(150, 110, 70), width=2)
    return im


def button(top, bottom, outline, w=192, h=64):
    t = np.linspace(0, 1, h)[:, None, None]
    rgb = np.array(top, float) * (1 - t) + np.array(bottom, float) * t
    rgb = np.repeat(rgb, w, axis=1) * (0.95 + noise(w, h, 2)[:, :, None] * 0.1)
    im = finish(rgb)
    im.putalpha(rounded_mask(w, h, 16))
    d = ImageDraw.Draw(im)
    d.rounded_rectangle((1, 1, w - 2, h - 2), 16, outline=outline, width=3)
    # Блик — тонкая линия в верхней рамке: она не растягивается при нарезке.
    d.line((14, 6, w - 15, 6), fill=(255, 255, 255, 90), width=2)
    return im


def plank(w=512, h=64):
    im = finish(wood(w, h) * 1.05)
    im.putalpha(rounded_mask(w, h, 10))
    d = ImageDraw.Draw(im)
    d.rounded_rectangle((1, 1, w - 2, h - 2), 10, outline=BRASS_DARK, width=3)
    for x in (16, w - 17):
        for y in (14, h - 15):
            d.ellipse((x - 4, y - 4, x + 4, y + 4), fill=BRASS)
    return im


def tab(on):
    return button((214, 150, 60), (150, 92, 28), BRASS) if on else button((70, 52, 38), (44, 30, 20), (120, 95, 60))


def icon(kind, size=64):
    im = Image.new("RGBA", (size * 4, size * 4), (0, 0, 0, 0))
    d = ImageDraw.Draw(im)
    s = size * 4
    if kind in ("silver", "doubloons"):
        face, rim = ((215, 222, 232), (130, 140, 155)) if kind == "silver" else ((246, 200, 80), (150, 100, 20))
        d.ellipse((s * .1, s * .1, s * .9, s * .9), fill=rim)
        d.ellipse((s * .17, s * .15, s * .83, s * .82), fill=face)
        d.ellipse((s * .35, s * .33, s * .65, s * .63), outline=rim, width=s // 28)
    elif kind == "planks":
        for i, y in enumerate((.18, .42, .66)):
            d.rounded_rectangle((s * .08, s * y, s * .92, s * (y + .2)), s // 20, fill=(176 - i * 15, 118 - i * 10, 70), outline=(90, 58, 30), width=s // 40)
    elif kind == "ether":
        d.polygon([(s * .5, s * .06), (s * .82, s * .55), (s * .5, s * .94), (s * .18, s * .55)], fill=(95, 211, 201), outline=(30, 120, 115))
        d.ellipse((s * .38, s * .3, s * .5, s * .45), fill=(220, 255, 250))
    elif kind == "journals":
        d.rounded_rectangle((s * .16, s * .12, s * .84, s * .88), s // 16, fill=(140, 52, 40), outline=(70, 25, 20), width=s // 30)
        d.rectangle((s * .22, s * .14, s * .3, s * .86), fill=(236, 220, 186))
        d.rectangle((s * .42, s * .3, s * .74, s * .38), fill=(242, 193, 78))
    elif kind == "charts":
        d.rounded_rectangle((s * .12, s * .2, s * .88, s * .8), s // 14, fill=PARCHMENT, outline=(150, 110, 70), width=s // 30)
        d.line((s * .25, s * .62, s * .45, s * .4, s * .6, s * .55, s * .78, s * .32), fill=(160, 60, 40), width=s // 28)
        d.ellipse((s * .72, s * .26, s * .82, s * .36), fill=(160, 60, 40))
    elif kind == "details":
        d.ellipse((s * .14, s * .14, s * .86, s * .86), fill=(150, 160, 175), outline=(80, 88, 100), width=s // 24)
        for k in range(8):
            a = k * np.pi / 4
            x, y = s * .5 + np.cos(a) * s * .4, s * .5 + np.sin(a) * s * .4
            d.ellipse((x - s * .08, y - s * .08, x + s * .08, y + s * .08), fill=(150, 160, 175))
        d.ellipse((s * .36, s * .36, s * .64, s * .64), fill=(60, 66, 76))
    return im.resize((size, size), Image.LANCZOS)


def portrait_crop(hero, anim):
    """Голова и плечи из первого кадра рисованной позы."""
    info = anim.get(hero)
    path = UNITS / f"{hero}.png"
    if not info or not path.exists():
        return None
    sheet = Image.open(path).convert("RGBA")
    fw, fh = info["frame"]
    frame = sheet.crop((0, 0, fw, fh))
    box = frame.getbbox()
    if not box:
        return None
    x0, y0, x1, y1 = box
    height = y1 - y0
    side = int(height * 0.36)
    # Центр головы — по непрозрачным пикселям верхней части фигуры.
    top = np.asarray(frame.crop((x0, y0, x1, y0 + side // 2)))[:, :, 3] > 40
    cols = np.where(top.any(axis=0))[0]
    cx = x0 + (int(cols.mean()) if len(cols) else (x1 - x0) // 2)
    left = cx - side // 2
    return frame.crop((left, y0 - side // 12, left + side, y0 - side // 12 + side))


def medallion(hero, element, rarity, name, anim, size=192):
    """Круг цвета стихии, портрет и кольцо редкости — одной картинкой."""
    big = size * 2
    im = Image.new("RGBA", (big, big), (0, 0, 0, 0))
    col = ELEMENT[element]
    bg = Image.new("RGBA", (big, big), (0, 0, 0, 0))
    yy, xx = np.mgrid[0:big, 0:big]
    r = np.hypot(xx - big / 2, yy - big / 2) / (big / 2)
    shade = np.clip(1.15 - r * 0.6, 0.4, 1.2)[:, :, None]
    rgb = np.array(col, float)[None, None, :] * shade
    bg = finish(rgb, (r < 0.94) * 255)
    im.alpha_composite(bg)
    face = portrait_crop(hero, anim)
    if face is not None:
        face = face.resize((int(big * 0.92), int(big * 0.92)), Image.LANCZOS)
        layer = Image.new("RGBA", (big, big), (0, 0, 0, 0))
        off = (big - face.width) // 2
        layer.alpha_composite(face, (off, off + big // 20))
        mask = Image.fromarray(((r < 0.9) * 255).astype(np.uint8))
        a = np.minimum(np.asarray(layer)[:, :, 3], np.asarray(mask))
        layer.putalpha(Image.fromarray(a.astype(np.uint8)))
        im.alpha_composite(layer)
    else:
        d = ImageDraw.Draw(im)
        font = ImageFont.truetype(FONT, int(big * 0.45))
        d.text((big / 2, big / 2), name[0], font=font, fill=(25, 28, 40), anchor="mm")
    d = ImageDraw.Draw(im)
    ring = RARITY[rarity]
    w = big // 22
    d.ellipse((w, w, big - w, big - w), outline=BRASS_DARK, width=w * 2)
    d.ellipse((w + 2, w + 2, big - w - 2, big - w - 2), outline=ring, width=w)
    return im.resize((size, size), Image.LANCZOS)


def figure(hero, anim):
    """Фигура героя во весь рост (первый кадр позы), обрезанная по рисунку."""
    info = anim.get(hero)
    path = UNITS / f"{hero}.png"
    if not info or not path.exists():
        return Image.new("RGBA", (8, 8), (0, 0, 0, 0))
    fw, fh = info["frame"]
    frame = Image.open(path).convert("RGBA").crop((0, 0, fw, fh))
    frame = frame.crop(frame.getbbox())
    if frame.height < 300:
        k = 300 / frame.height
        frame = frame.resize((int(frame.width * k), 300), Image.LANCZOS)
    return frame


def from_source(path, size):
    """Рисованная деталь: ровный фон (пурпурный, белый — любой, какой в углах картинки)
    становится прозрачным, обрезка по рисунку, размер как у временной."""
    im = Image.open(path).convert("RGBA")
    w, h = im.size
    # Водяной знак в углу не мешает: цвет фона — медиана по краям, а не по одному углу.
    a = np.asarray(im).astype(int)
    edge = np.concatenate([a[0, :, :3], a[-1, :, :3], a[:, 0, :3], a[:, -1, :3]])
    bg = np.median(edge, axis=0)
    dist = np.abs(a[:, :, :3] - bg).sum(axis=2)
    near = dist < 60
    # Прозрачным становится только фон, связанный с краем картинки.
    mask = Image.fromarray((near * 255).astype(np.uint8), "L").copy()
    for x, y in [(0, 0), (w - 1, 0), (0, h - 1), (w - 1, h - 1), (w // 2, 0), (w // 2, h - 1), (0, h // 2), (w - 1, h // 2)]:
        if mask.getpixel((x, y)) == 255:
            ImageDraw.floodfill(mask, (x, y), 128)
    outside = np.asarray(mask) == 128
    # Мягкий край: полупрозрачность по близости к цвету фона на границе.
    soft = np.clip((dist - 30) / 60, 0, 1)
    edge_zone = np.asarray(Image.fromarray(outside.astype(np.uint8) * 255).filter(ImageFilter.MaxFilter(5))) > 0
    alpha = np.where(outside, 0, np.where(edge_zone, soft * 255, 255))
    a[:, :, 3] = alpha
    out = Image.fromarray(a.astype(np.uint8), "RGBA")
    # Водяной знак Gemini в углу — отдельное пятнышко: берём самый большой кусок рисунка.
    box = biggest_blob_box(np.asarray(out)[:, :, 3] > 128)
    out = out.crop(box)
    return out.resize(size, Image.LANCZOS)


def biggest_blob_box(solid):
    """Рамка рисунка без мелких пятнышек (водяной знак в углу): берём строки и столбцы,
    где рисунок занимает заметную долю ширины или высоты."""
    h, w = solid.shape
    rows = np.where(solid.sum(axis=1) > w * 0.06)[0]
    cols = np.where(solid.sum(axis=0) > h * 0.06)[0]
    if len(rows) == 0 or len(cols) == 0:
        return (0, 0, w, h)
    return (int(cols.min()), int(rows.min()), int(cols.max()) + 1, int(rows.max()) + 1)


def save(name, im):
    src = SRC / name
    if src.exists():
        im = from_source(src, im.size)
    im.save(OUT / name)
    print("  ", name, "(рисунок)" if src.exists() else "")


def main():
    OUT.mkdir(parents=True, exist_ok=True)
    save("panel.png", panel())
    save("card.png", parchment())
    save("button.png", button((236, 160, 70), (176, 96, 30), (255, 226, 160)))
    save("button_pressed.png", button((176, 96, 30), (136, 70, 20), (220, 180, 110)))
    save("button_off.png", button((110, 112, 118), (76, 78, 84), (150, 150, 150)))
    save("button_dark.png", button((52, 66, 98), (30, 38, 62), BRASS))
    save("tab_on.png", tab(True))
    save("tab_off.png", tab(False))
    save("plank.png", plank())
    for kind in ("silver", "planks", "ether", "journals", "doubloons", "charts", "details"):
        save(f"icon_{kind}.png", icon(kind))
    heroes = json.load(open("data/heroes.json"))
    anim = json.load(open("data/animated.json"))
    for h in heroes:
        save(f"hero_{h['id']}.png", medallion(h["id"], h["element"], h["rarity"], h["name"], anim))
        save(f"full_{h['id']}.png", figure(h["id"], anim))


if __name__ == "__main__":
    main()
