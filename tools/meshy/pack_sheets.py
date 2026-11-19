#!/usr/bin/env python3
"""Кадры бойца (render_sprites.py) → спрайт-лист для игры и запись в data/animated.json.

  python3 tools/meshy/pack_sheets.py kaya --height 1.45

Лист: строка — клип, столбец — кадр. Рамка общая для всех клипов (объединение непрозрачного),
поэтому ноги стоят на одной линии во всех клипах. `--height` — рост бойца в долях высоты модели
на поле боя. Нужны Pillow и numpy.
"""
import argparse
import json
from pathlib import Path

import numpy as np
from PIL import Image, ImageFilter

ROOT = Path(__file__).resolve().parents[2]
FRAME_H = 256          # высота кадра в листе: на экране боец ~150 px, запас под резкость
CLIP_FPS = {"idle": 6, "victory": 6}   # зацикленные клипы; остальные растягиваются на время клипа игры
ORDER = ["idle", "attack", "skill", "hit", "death", "victory"]

ap = argparse.ArgumentParser()
ap.add_argument("name")
ap.add_argument("--height", type=float, default=1.4)
a = ap.parse_args()

src = ROOT / "art" / "meshy" / a.name / "frames"
clips = [c for c in ORDER if (src / c).is_dir()]
OUTLINE = (40, 26, 26)   # цвет контура, как у линий в рисованных референсах


def outlined(im):
    """Тёмный контур по краю силуэта (2 px на кадре 512): спрайт читается на любом фоне."""
    edge = im.getchannel("A").filter(ImageFilter.MaxFilter(5))
    line = Image.new("RGBA", im.size, OUTLINE + (255,))
    line.putalpha(edge)
    return Image.alpha_composite(line, im)


frames = {c: [outlined(Image.open(p).convert("RGBA")) for p in sorted((src / c).glob("*.png"))]
          for c in clips}

# Общая рамка по всем кадрам.
union = None
for imgs in frames.values():
    for im in imgs:
        box = im.getbbox()
        if box:
            union = box if union is None else (
                min(union[0], box[0]), min(union[1], box[1]), max(union[2], box[2]), max(union[3], box[3]))
pad = 4
left, top, right, bottom = union[0] - pad, union[1] - pad, union[2] + pad, union[3] + pad
crop_w, crop_h = right - left, bottom - top
# Где ноги: нижний непрозрачный ряд первого кадра ожидания, в долях высоты рамки снизу.
alpha = np.asarray(frames[clips[0]][0].crop((left, top, right, bottom)))[:, :, 3]
rows = np.nonzero(alpha.max(axis=1) > 32)[0]
foot = (crop_h - 1 - rows.max()) / crop_h
# Рост персонажа в кадре ожидания — от ног до макушки, в долях рамки.
stature = (rows.max() - rows.min()) / crop_h

scale = FRAME_H / crop_h
fw, fh = round(crop_w * scale), FRAME_H
cols = max(len(v) for v in frames.values())
sheet = Image.new("RGBA", (fw * cols, fh * len(clips)), (0, 0, 0, 0))
meta = {}
for r, c in enumerate(clips):
    for i, im in enumerate(frames[c]):
        tile = im.crop((left, top, right, bottom)).convert("RGBa").resize((fw, fh), Image.Resampling.LANCZOS)
        sheet.paste(tile.convert("RGBA"), (i * fw, r * fh))
    meta[c] = {"row": r, "frames": len(frames[c])}
    if c in CLIP_FPS:
        meta[c]["fps"] = CLIP_FPS[c]

out = ROOT / "crates" / "game" / "assets" / "units" / f"{a.name}.png"
out.parent.mkdir(parents=True, exist_ok=True)
sheet.save(out, optimize=True)

data_path = ROOT / "data" / "animated.json"
data = json.loads(data_path.read_text()) if data_path.exists() else {}
data[a.name] = {
    "sheet": f"units/{a.name}.png",
    "frame": [fw, fh],
    "columns": cols,
    "rows": len(clips),
    # Рост на поле в долях высоты модели и где ноги в кадре (доля снизу), чтобы ставить на слот.
    "height": round(a.height / stature, 3),
    "foot": round(foot, 4),
    "top": round(foot + stature, 4),
    "clips": meta,
}
lines = ["{"] + [f'  "{k}": {json.dumps(v, ensure_ascii=False)}' + ("," if i < len(data) - 1 else "")
                 for i, (k, v) in enumerate(data.items())] + ["}"]
data_path.write_text("\n".join(lines) + "\n")
print(f"{out.relative_to(ROOT)}: {cols}×{len(clips)} кадров {fw}×{fh}; клипы {', '.join(clips)}")
