#!/usr/bin/env python3
"""Рисованный боец через Gemini: сначала образ героя, потом его ключевые позы по этому образу.

  python3 tools/sprites/gemini_hero.py borg design        # образ (art/2d/borg/design.png)
  python3 tools/sprites/gemini_hero.py borg idle attack   # позы по образу
  python3 tools/sprites/gemini_hero.py borg               # всё, чего ещё нет

Образец стиля — стойки готовых бойцов из их спрайт-листов (STYLE_FROM). Позы — на белом фоне,
дальше их режет tools/sprites/keypose.py. Описание героя и поз — в HEROES ниже.
Ключ: GEMINI_API_KEY или ~/.config/skybound-corsairs/gemini.key (в репозиторий не кладём).
"""
import base64
import io
import json
import sys
from pathlib import Path

from PIL import Image

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import gemini_ui  # noqa: E402  (тот же вызов API и ключ)

ROOT = Path(__file__).resolve().parents[2]
STYLE_FROM = ["ren", "kaya", "mei"]

STYLE = (
    "The attached sheet shows fighters from my mobile anime sky-pirate gacha game. Match their "
    "painted art style exactly: clean anime line art, soft cel shading with painterly texture, "
    "the same proportions, rendering quality and level of detail. "
)
SOLO = (
    "Draw ONE character only, full body from head to boots, nothing cropped, standing on an "
    "invisible floor, facing three-quarters to the right of the viewer. Plain pure white "
    "background, no floor, no shadow, no text, no frame, no other characters. "
)

HEROES = {
    "borg": {
        "who": (
            "Borg, a huge good-natured former convict giant who became a sky-pirate boarder. "
            "About two heads taller and much broader than the other heroes: massive shoulders, "
            "thick arms, a round strong belly, short reddish-brown beard, shaved head with a "
            "small scar, kind cheerful eyes. Wears a torn striped prison shirt with rolled "
            "sleeves under a battered steel shoulder plate and a leather harness, heavy work "
            "trousers, big boots, a broken iron shackle still on one wrist. His weapon is a "
            "big iron boarding hook (grappling hook) on a thick chain. Steel-grey and rust-"
            "orange palette. Friendly, a little goofy, but a fearsome brawler."
        ),
        "poses": {
            "design": "Calm neutral standing pose, hook resting on his shoulder, friendly smile.",
            "idle": "Battle-ready but relaxed stance, feet apart, the boarding hook held low "
                    "in his right hand with the chain wrapped around the forearm, grinning.",
            "attack": "Swinging the boarding hook forward at an enemy on the right in a wide "
                      "horizontal arc, chain stretched, body twisted into the swing.",
            "skill": "Shoulder-ram charge to the right: leaning forward low, steel shoulder "
                     "plate first, one boot pushing off, dust kicked up behind him.",
            "ult": "Roaring with both fists raised, a shining steel-blue aura and metal sparks "
                   "around his body, chain whirling around him, muscles tensed.",
            "hit": "Staggering back from a hit, one arm raised to shield his face, grimacing.",
            "death": "Defeated: kneeling on one knee, head down, leaning on the hook planted "
                     "in the floor, exhausted.",
            "victory": "Laughing happily with one fist raised high, the other hand patting his "
                       "belly, hook hanging from his shoulder.",
        },
    }
}


def b64(im):
    buf = io.BytesIO()
    im.convert("RGB").save(buf, "JPEG", quality=90)
    return base64.b64encode(buf.getvalue()).decode()


def style_sheet():
    """Стойки готовых бойцов рядом на белом — образец стиля."""
    anim = json.loads((ROOT / "data/animated.json").read_text())
    figs = []
    for unit in STYLE_FROM:
        a = anim[unit]
        w, h = a["frame"]
        sheet = Image.open(ROOT / "crates/game/assets" / a["sheet"]).convert("RGBA")
        figs.append(sheet.crop((0, 0, w, h)))
    out = Image.new("RGB", (sum(f.width for f in figs) + 40 * len(figs), 420), "white")
    x = 20
    for f in figs:
        out.paste(f, (x, 18), f)
        x += f.width + 40
    return out


def generate(parts, api_key):
    body = {
        "contents": [{"parts": parts}],
        "generationConfig": {"responseModalities": ["IMAGE"], "imageConfig": {"aspectRatio": "3:4"}},
    }
    # Тот же запрос, что в gemini_ui.generate, но с несколькими картинками-образцами.
    import urllib.request
    import urllib.error
    import time

    url = f"https://generativelanguage.googleapis.com/v1beta/models/{gemini_ui.MODEL}:generateContent"
    req = urllib.request.Request(
        url,
        data=json.dumps(body).encode(),
        headers={"content-type": "application/json", "x-goog-api-key": api_key},
    )
    for attempt in range(3):
        try:
            with urllib.request.urlopen(req, timeout=240) as r:
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
    hero = sys.argv[1]
    spec = HEROES[hero]
    out = ROOT / "art/2d" / hero
    out.mkdir(parents=True, exist_ok=True)
    wanted = sys.argv[2:] or [p for p in spec["poses"] if not (out / f"{p}.png").exists()]
    api_key = gemini_ui.key()
    style = {"inline_data": {"mime_type": "image/jpeg", "data": b64(style_sheet())}}
    for pose in wanted:
        parts = [style]
        if pose == "design":
            text = STYLE + SOLO + "Character design: " + spec["who"] + " Pose: " + spec["poses"][pose]
        else:
            design = out / "design.png"
            if not design.exists():
                raise SystemExit("Сначала образ: gemini_hero.py <герой> design")
            parts.append({"inline_data": {"mime_type": "image/jpeg", "data": b64(Image.open(design))}})
            text = (
                STYLE + "The second picture is the character design of " + hero.capitalize()
                + ". Keep his face, body, outfit, colors and weapon exactly the same. " + SOLO
                + "Pose: " + spec["poses"][pose]
            )
        parts.append({"text": text})
        print(f"{hero}/{pose} …", flush=True)
        im = generate(parts, api_key)
        im.convert("RGB").save(out / f"{pose}.png")
        print(f"   → {out / f'{pose}.png'} {im.size}")


if __name__ == "__main__":
    main()
