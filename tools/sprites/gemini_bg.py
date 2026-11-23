#!/usr/bin/env python3
"""Фоны боя этапов: правка готового фона в Gemini — наша палуба та же, меняется то, что за бортом.

  python3 tools/sprites/gemini_bg.py            # все фоны, которых ещё нет
  python3 tools/sprites/gemini_bg.py 1-4 1-6    # только эти (перегенерировать)

Исходник ложится в art/2d/bg/<этап>.png, фон для игры — crates/game/assets/battle/<этап>.jpg
(1080×1920). Палубные этапы правятся из фона с имперским кораблём справа — враги стоят на его
палубе, поэтому корабль должен остаться на том же месте (enemy_slots в data/battlefield.json).
Ключ: GEMINI_API_KEY или ~/.config/skybound-corsairs/gemini.key.
"""
import base64
import io
import json
import sys
import time
import urllib.error
import urllib.request
from pathlib import Path

from PIL import Image

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import gemini_ui  # noqa: E402

ROOT = Path(__file__).resolve().parents[2]
BATTLE = ROOT / "crates/game/assets/battle"
SRC = ROOT / "art/2d/bg"

KEEP = (
    "Edit this vertical battle background from my mobile anime sky-pirate game. Keep the art "
    "style, the camera, and the whole foreground exactly the same: our wooden deck, the "
    "planks, the cannon, the barrels, crates, ropes and the mast on the left must not move "
    "or change. No characters, no people, no text. "
)
SHIP = (
    "Keep the enemy ship on the right in exactly the same place, size and angle, with its "
    "flat deck and boarding plank where they are now — fighters will stand on that deck. "
)

STAGES = {
    "1-4": ("1-1", KEEP + SHIP + "Change the enemy vessel into a smaller, lean imperial "
            "sentry brig with white-and-blue hull and a watch lantern. Early cold morning: "
            "pale blue-grey dawn light, low mist between distant floating islands."),
    "1-5": ("1-2", KEEP + "Change the sky into a gigantic storm front: a towering dark wall "
            "of thunderclouds from the clouds below to the top of the picture, heavy slanted "
            "rain curtains, a thin strip of blood-red sunset light under the storm, distant "
            "lightning. Wet shining deck."),
    "1-6": ("1-3", KEEP + SHIP + "Change the enemy vessel into an imperial gunboat with one "
            "enormous long brass cannon on its bow pointing up into the sky. Evening, orange "
            "fire light, drifting gunpowder smoke across the sky."),
    "1-7": ("1-2", KEEP + "Change the background into tall jagged floating rock spires very "
            "close to the ship, their ledges covered with big nests made of crackling blue "
            "crystal and twigs, small glowing eggs, teal and violet lightning between the "
            "rocks, dark stormy sky."),
    "1-8": ("1-1", KEEP + SHIP + "Make it overcast and dramatic: behind the enemy ship, a "
            "long line of four more imperial warships stands across the sky like a blockade, "
            "white sails and blue flags, and far behind them a dark swirling storm vortex on "
            "the horizon. Cold grey-violet light."),
    "1-9": ("1-boss", KEEP + "Move the giant storm vortex further away so it fills only the "
            "upper half of the sky in the distance; we are at its edge. Strong wind: torn "
            "sail scraps and wooden debris flying across the sky, sheets of rain, purple dusk "
            "light, the deck lanterns swinging."),
}


def b64(path):
    im = Image.open(path).convert("RGB")
    im.thumbnail((1080, 1920))
    buf = io.BytesIO()
    im.save(buf, "JPEG", quality=90)
    return base64.b64encode(buf.getvalue()).decode()


def generate(base, prompt, api_key):
    body = {
        "contents": [{"parts": [
            {"inline_data": {"mime_type": "image/jpeg", "data": b64(base)}},
            {"text": prompt},
        ]}],
        "generationConfig": {"responseModalities": ["IMAGE"], "imageConfig": {"aspectRatio": "9:16"}},
    }
    url = f"https://generativelanguage.googleapis.com/v1beta/models/{gemini_ui.MODEL}:generateContent"
    req = urllib.request.Request(
        url, data=json.dumps(body).encode(),
        headers={"content-type": "application/json", "x-goog-api-key": api_key},
    )
    for attempt in range(3):
        try:
            with urllib.request.urlopen(req, timeout=240) as r:
                data = json.load(r)
            break
        except urllib.error.HTTPError as e:
            msg = e.read().decode()[:300]
            if e.code in (500, 503) and attempt < 2:
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
    SRC.mkdir(parents=True, exist_ok=True)
    wanted = sys.argv[1:] or [s for s in STAGES if not (BATTLE / f"{s}.jpg").exists()]
    api_key = gemini_ui.key()
    for stage in wanted:
        base, prompt = STAGES[stage]
        print(f"{stage} (из {base}) …", flush=True)
        im = generate(BATTLE / f"{base}.jpg", prompt, api_key).convert("RGB")
        im.save(SRC / f"{stage}.png")
        im.resize((1080, 1920), Image.LANCZOS).save(BATTLE / f"{stage}.jpg", quality=88)
        print(f"   → crates/game/assets/battle/{stage}.jpg (исходник {im.size})")


if __name__ == "__main__":
    main()
