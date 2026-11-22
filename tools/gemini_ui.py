#!/usr/bin/env python3
"""Рисованные детали интерфейса и фоны экранов через Gemini API.

  python3 tools/gemini_ui.py                 # всё, чего ещё нет
  python3 tools/gemini_ui.py panel card      # только эти (перегенерировать)

Образец стиля — фон главного меню (assets/first_island.jpg). Детали ложатся в
assets/ui_src/ (их вырезает и подгоняет tools/make_ui.py), фоны — в
crates/game/assets/backgrounds/. Ключ: переменная GEMINI_API_KEY или файл
~/.config/skybound-corsairs/gemini.key (в репозиторий ключ не кладём).
"""

import base64
import io
import json
import os
import sys
import time
import urllib.error
import urllib.request
from pathlib import Path

from PIL import Image

MODEL = "gemini-3-pro-image"
STYLE_REF = Path("assets/first_island.jpg")
UI_SRC = Path("assets/ui_src")
BACKGROUNDS = Path("crates/game/assets/backgrounds")

STYLE = (
    "The attached picture is the main menu of my mobile anime sky-pirate gacha game. "
    "Match its painted art style exactly: warm dark wood, polished brass, aged parchment, "
    "soft painterly shading, light from the top-left. "
)
ASSET = (
    "Create a single 2D game UI asset. Orthographic front view, no perspective, no text, "
    "no letters, no characters. The asset is isolated and centered on a perfectly flat solid "
    "magenta background (#FF00FF) with wide empty margins on all sides, no shadow on the "
    "background. "
)
FRAME = (
    "The frame has the same thickness on all four sides (about 12% of the height); "
    "decorations only in the four corners, the straight sides are plain and even so the "
    "image can be 9-slice stretched. "
)
BACKGROUND = (
    "Create a vertical 9:16 painted background for a mobile game screen in the same style "
    "and palette. No characters, no text, no UI. Keep the middle calm and slightly darker "
    "so interface panels stay readable. "
)

# (имя, соотношение сторон, промпт)
ITEMS = [
    ("panel", "1:1", ASSET + FRAME + "A square panel of dark vertical wooden planks inside a thick polished brass frame with a brass rivet in each corner, slightly rounded corners."),
    ("card", "1:1", ASSET + FRAME + "A square sheet of aged cream parchment, flat and evenly lit, with a thin dark-brown inked border and slightly darker edges, slightly rounded corners, very subtle paper texture."),
    ("plank", "21:9", ASSET + FRAME + "A long thin horizontal wooden signboard plank (about 8 times wider than tall) with a thin brass edge and a brass bolt at each end, rounded corners."),
    ("button", "21:9", ASSET + FRAME + "A wide rounded game button (about 3 times wider than tall) of glossy amber-orange enamel set in a polished brass bezel, a gentle highlight along the top edge."),
    ("button_pressed", "21:9", ASSET + FRAME + "A wide rounded game button (about 3 times wider than tall) of darker amber-orange enamel in a polished brass bezel, looking pressed in, highlight along the bottom edge."),
    ("button_off", "21:9", ASSET + FRAME + "A wide rounded game button (about 3 times wider than tall) of matte desaturated grey enamel in a dull brass bezel, looks disabled."),
    ("button_dark", "21:9", ASSET + FRAME + "A wide rounded game button (about 3 times wider than tall) of deep navy-blue enamel in a polished brass bezel, a gentle highlight along the top edge."),
    ("tab_on", "21:9", ASSET + FRAME + "A wide rounded tab (about 3 times wider than tall) of warm amber leather with a brass-stitched edge, looks active and bright."),
    ("tab_off", "21:9", ASSET + FRAME + "A wide rounded tab (about 3 times wider than tall) of dark brown leather with a dull stitched edge, looks inactive and muted."),
    ("icon_silver", "1:1", ASSET + "A game resource icon: a single shiny silver coin with an engraved airship, slight tilt, bold readable silhouette."),
    ("icon_doubloons", "1:1", ASSET + "A game resource icon: a single gold doubloon coin with a winged skull emblem, slight tilt, bold readable silhouette."),
    ("icon_planks", "1:1", ASSET + "A game resource icon: a small stack of three wooden planks tied with rope, bold readable silhouette."),
    ("icon_ether", "1:1", ASSET + "A game resource icon: a glowing teal ether crystal shard, bold readable silhouette."),
    ("icon_journals", "1:1", ASSET + "A game resource icon: a red leather ship's logbook with brass corners, bold readable silhouette."),
    ("icon_charts", "1:1", ASSET + "A game resource icon: a partly unrolled parchment navigation chart with a red route line, bold readable silhouette."),
    ("icon_details", "1:1", ASSET + "A game resource icon: a brass gear cog with a small steel bolt, bold readable silhouette."),
    ("bg_crew", "9:16", BACKGROUND + "Cozy crew quarters below deck of a wooden airship: hammocks, warm lanterns, maps pinned to the walls, round portholes showing clouds outside."),
    ("bg_tavern", "9:16", BACKGROUND + "A lively sky-port tavern interior: wooden bar counter, hanging lanterns, a notice board with wanted posters, a big window to the clouds."),
    ("bg_activities", "9:16", BACKGROUND + "View from an airship crow's nest over a vast stormy cloud abyss, distant floating islands and the silhouette of an imperial warship."),
]


def key():
    k = os.environ.get("GEMINI_API_KEY")
    if k:
        return k.strip()
    path = Path.home() / ".config/skybound-corsairs/gemini.key"
    if path.exists():
        return path.read_text().strip()
    raise SystemExit("Нет ключа: GEMINI_API_KEY или ~/.config/skybound-corsairs/gemini.key")


def style_ref():
    im = Image.open(STYLE_REF).convert("RGB")
    im.thumbnail((768, 768))
    buf = io.BytesIO()
    im.save(buf, "JPEG", quality=88)
    return base64.b64encode(buf.getvalue()).decode()


def generate(prompt, aspect, ref, api_key):
    body = {
        "contents": [
            {
                "parts": [
                    {"inline_data": {"mime_type": "image/jpeg", "data": ref}},
                    {"text": STYLE + prompt},
                ]
            }
        ],
        "generationConfig": {
            "responseModalities": ["IMAGE"],
            "imageConfig": {"aspectRatio": aspect},
        },
    }
    url = f"https://generativelanguage.googleapis.com/v1beta/models/{MODEL}:generateContent"
    req = urllib.request.Request(
        url,
        data=json.dumps(body).encode(),
        headers={"content-type": "application/json", "x-goog-api-key": api_key},
    )
    for attempt in range(3):
        try:
            with urllib.request.urlopen(req, timeout=180) as r:
                data = json.load(r)
            break
        except urllib.error.HTTPError as e:
            msg = e.read().decode()[:300]
            if e.code in (429, 500, 503) and attempt < 2:
                print(f"   повтор ({e.code})")
                time.sleep(20)
                continue
            raise SystemExit(f"Ошибка API {e.code}: {msg}")
    for cand in data.get("candidates", []):
        for part in cand.get("content", {}).get("parts", []):
            blob = part.get("inlineData") or part.get("inline_data")
            if blob:
                return Image.open(io.BytesIO(base64.b64decode(blob["data"])))
    raise SystemExit(f"В ответе нет картинки: {json.dumps(data)[:300]}")


def main():
    only = set(sys.argv[1:])
    api_key = key()
    ref = style_ref()
    UI_SRC.mkdir(parents=True, exist_ok=True)
    BACKGROUNDS.mkdir(parents=True, exist_ok=True)
    for name, aspect, prompt in ITEMS:
        if name.startswith("bg_"):
            out = BACKGROUNDS / f"{name[3:]}.jpg"
        else:
            out = UI_SRC / f"{name}.png"
        if only and name not in only:
            continue
        if not only and out.exists():
            continue
        print(" ", name)
        im = generate(prompt, aspect, ref, api_key)
        if name.startswith("bg_"):
            im.convert("RGB").resize((1080, 1920), Image.LANCZOS).save(out, quality=88)
        else:
            im.convert("RGB").save(out)


if __name__ == "__main__":
    main()
