#!/usr/bin/env python3
"""Покадровые клипы бойца из рисунков → кадры для pack_sheets.py.

  python3 tools/sprites/painted_frames.py art/2d/ren/clips.json
  python3 tools/meshy/pack_sheets.py ren --frames art/2d/ren/frames --frame-h 384 --cols 8

Рисунки двух видов (белый фон, как из Gemini):
- перерисовка 3D-кадра ("ref": кадр из render_sprites.py) — встаёт ровно туда, где был 3D-кадр:
  рост и положение ног между кадрами не прыгают. Рисунок должен быть сделан из картинки, которую
  готовит pose3d (кадр обрезан по силуэту, вписан в fit и выровнен по центру поля box);
- самостоятельная поза (стойка, выпад) — в общем масштабе рисунков ("scale" — поправка, если
  Gemini нарисовал мельче), ноги на линии ног 3D-стойки, по центру как она ("dx" — сдвиг в ростах).

clips.json:
  {"ref": "art/meshy/ren/frames/spin", "stand": "0000.png", "fit": [700, 900], "box": [800, 1000],
   "clips": {"idle": {"frames": ["idle.png"]},
             "skill": {"frames": [{"img": "spin/06.png", "ref": "0006.png"}, "idle.png"],
                       "seq": [0, 0, 1]}}}
"seq" — порядок показа кадров (повтор — задержка кадра), уходит в meta.json для pack_sheets.

  python3 tools/sprites/painted_frames.py pose3d art/meshy/ren/frames/spin 0006 0008 <папка>
— готовит 3D-кадры на белом фоне для перерисовки. Нужны Pillow и numpy.
"""
import json
import sys
from pathlib import Path

import numpy as np
from PIL import Image

sys.path.insert(0, str(Path(__file__).resolve().parent))
from keypose import mask_of  # noqa: E402

ROOT = Path(__file__).resolve().parents[2]
SCALE = 2          # холст = кадр 3D × SCALE: рисунки детальнее рендера
FIT, BOX = (700, 900), (800, 1000)


def alpha_box(img):
    a = np.asarray(img.getchannel("A")) > 0
    ys, xs = np.nonzero(a.any(axis=1))[0], np.nonzero(a.any(axis=0))[0]
    return xs.min(), ys.min(), xs.max() + 1, ys.max() + 1


def fit_factor(w, h, fit):
    return min(fit[0] / w, fit[1] / h)


def pose3d(src, names, out):
    """3D-кадры → белый фон, обрезка по силуэту, вписать в FIT по центру BOX (для перерисовки)."""
    out.mkdir(parents=True, exist_ok=True)
    for n in names:
        img = Image.open(src / f"{n}.png").convert("RGBA")
        x0, y0, x1, y1 = alpha_box(img)
        crop = img.crop((x0, y0, x1, y1))
        f = fit_factor(*crop.size, FIT)
        crop = crop.resize((round(crop.width * f), round(crop.height * f)), Image.Resampling.LANCZOS)
        page = Image.new("RGBA", BOX, (255, 255, 255, 255))
        page.alpha_composite(crop, ((BOX[0] - crop.width) // 2, (BOX[1] - crop.height) // 2))
        page.convert("RGB").save(out / f"{n}.png")
        print(out / f"{n}.png")


def cutout(path):
    img = Image.open(path).convert("RGB")
    rgba = img.convert("RGBA")
    rgba.putalpha(mask_of(img))
    return rgba


def main(spec_path):
    spec = json.loads(spec_path.read_text())
    art = spec_path.parent
    ref_dir = ROOT / spec["ref"]
    fit, box = spec.get("fit", FIT), spec.get("box", BOX)
    stand = Image.open(ref_dir / spec["stand"]).convert("RGBA")
    size = (stand.width * SCALE, stand.height * SCALE)
    sx0, sy0, sx1, sy1 = (v * SCALE for v in alpha_box(stand))
    base = None   # масштаб самостоятельных поз: рост стойки 3D / рост первой такой позы
    for clip, desc in spec["clips"].items():
        frames = []
        for entry in desc["frames"]:
            e = {"img": entry} if isinstance(entry, str) else entry
            pic = cutout(art / e["img"])
            canvas = Image.new("RGBA", size, (0, 0, 0, 0))
            if "ref" in e:
                # Обратно тому, что сделал pose3d: рисунок ↔ поле box ↔ обрезанный 3D-кадр.
                ref = Image.open(ref_dir / e["ref"]).convert("RGBA")
                x0, y0, x1, y1 = alpha_box(ref)
                f = fit_factor(x1 - x0, y1 - y0, fit)
                page = pic.resize(tuple(box), Image.Resampling.LANCZOS)
                k = SCALE / f
                page = page.resize((round(box[0] * k), round(box[1] * k)), Image.Resampling.LANCZOS)
                left = x0 * SCALE - (box[0] - (x1 - x0) * f) / 2 * k
                top = y0 * SCALE - (box[1] - (y1 - y0) * f) / 2 * k
                canvas.alpha_composite(page, (round(left), round(top)))
            else:
                bx0, by0, bx1, by1 = alpha_box(pic)
                if base is None:
                    base = (sy1 - sy0) / (by1 - by0)
                k = base * e.get("scale", 1.0)
                body = pic.crop((bx0, by0, bx1, by1))
                body = body.resize((round(body.width * k), round(body.height * k)), Image.Resampling.LANCZOS)
                cx = (sx0 + sx1) / 2 + e.get("dx", 0.0) * (sy1 - sy0)
                canvas.alpha_composite(body, (round(cx - body.width / 2), round(sy1 - body.height)))
            frames.append(canvas)
        d = art / "frames" / clip
        d.mkdir(parents=True, exist_ok=True)
        for old in list(d.glob("*.png")) + list(d.glob("meta.json")):
            old.unlink()
        for i, frame in enumerate(frames):
            frame.save(d / f"{i:04d}.png")
        meta = {k: desc[k] for k in ("seq", "fps") if k in desc}
        if meta:
            (d / "meta.json").write_text(json.dumps(meta) + "\n")
        print(f"{clip}: {len(frames)} кадр(ов){', порядок ' + str(len(desc['seq'])) if 'seq' in desc else ''}")


if __name__ == "__main__":
    if sys.argv[1] == "pose3d":
        pose3d(ROOT / sys.argv[2], sys.argv[3:-1], Path(sys.argv[-1]))
    else:
        main(Path(sys.argv[1]))
