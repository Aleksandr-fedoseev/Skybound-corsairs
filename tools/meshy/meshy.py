#!/usr/bin/env python3
"""Клиент Meshy API для бойцов: модель по картинке → авториг → клипы из библиотеки.

Ключ — ~/.config/meshy/key (в репозиторий не кладётся). Задачи и скачанные файлы —
art/meshy/<имя>/ (вне git, это исходники). Только стандартная библиотека Python.

  python3 tools/meshy/meshy.py ref kaya front prompt.txt art/meshy/kaya/ref.png  # image-to-image (nano-banana-pro)
  python3 tools/meshy/meshy.py multi kaya front.png side.png       # модель по видам, поза — как на картинке
  python3 tools/meshy/meshy.py model kaya art/meshy/kaya/ref.png   # image-to-3D, A-поза
  python3 tools/meshy/meshy.py rig kaya                            # авториг готовой модели
  python3 tools/meshy/meshy.py anim kaya 0 232 178 189 59          # клипы по action_id
  python3 tools/meshy/meshy.py wait kaya                           # дождаться всех задач и скачать GLB
  python3 tools/meshy/meshy.py status kaya

Каждая задача тратит кредиты подписки: model ~ десятки, rig и каждый клип — меньше.
"""
import base64
import json
import sys
import time
import urllib.request
from pathlib import Path

API = "https://api.meshy.ai/openapi"
ROOT = Path(__file__).resolve().parents[2]
KEY = Path.home() / ".config" / "meshy" / "key"

# Где лежит задача каждого вида и какой файл из неё скачивать.
KINDS = {
    "model": ("v1/image-to-3d", lambda r: r.get("model_urls", {}).get("glb")),
    "multi": ("v1/multi-image-to-3d", lambda r: r.get("model_urls", {}).get("glb")),
    "ref": ("v1/image-to-image", lambda r: (r.get("image_urls") or [None])[0]),
    "rig": ("v1/rigging", lambda r: (r.get("result") or {}).get("rigged_character_glb_url")),
    "anim": ("v1/animations", lambda r: (r.get("result") or {}).get("animation_glb_url")),
}


def request(method, path, body=None):
    key = KEY.read_text().strip()
    data = json.dumps(body).encode() if body is not None else None
    req = urllib.request.Request(
        f"{API}/{path}",
        data=data,
        method=method,
        headers={"Authorization": f"Bearer {key}", "Content-Type": "application/json"},
    )
    with urllib.request.urlopen(req, timeout=120) as resp:
        return json.loads(resp.read())


def folder(name):
    d = ROOT / "art" / "meshy" / name
    d.mkdir(parents=True, exist_ok=True)
    return d


def tasks(name):
    p = folder(name) / "tasks.json"
    return json.loads(p.read_text()) if p.exists() else {}


def save(name, t):
    (folder(name) / "tasks.json").write_text(json.dumps(t, indent=2, ensure_ascii=False) + "\n")


def data_uri(image):
    raw = Path(image).read_bytes()
    mime = "image/png" if str(image).endswith(".png") else "image/jpeg"
    return f"data:{mime};base64,{base64.b64encode(raw).decode()}"


def cmd_ref(name, view, prompt_file, images):
    """Перерисовать референс: вид `view` (front/side) по картинкам и промпту из файла."""
    body = {
        "ai_model": "nano-banana-pro",
        "prompt": Path(prompt_file).read_text().strip(),
        "reference_image_urls": [data_uri(i) for i in images],
        "aspect_ratio": "3:4",
        "remove_background": True,
    }
    tid = request("POST", "v1/image-to-image", body)["result"]
    t = tasks(name)
    t.setdefault("ref", {})[view] = tid
    save(name, t)
    print(f"{name}: ref {view} {tid}")


def cmd_multi(name, images):
    """Модель по 1–4 видам (первый — спереди). Без pose_mode: поза берётся с картинки, оружие в руках."""
    body = {
        "image_urls": [data_uri(i) for i in images],
        "ai_model": "latest",
        "should_texture": True,
        "should_remesh": True,
        "target_polycount": 30000,
        "target_formats": ["glb"],
        "multi_view_thumbnails": True,
    }
    tid = request("POST", "v1/multi-image-to-3d", body)["result"]
    t = tasks(name)
    t["multi"] = tid
    t.pop("rig", None)
    t.pop("anim", None)
    save(name, t)
    print(f"{name}: multi {tid}")


def cmd_model(name, image):
    body = {
        "image_url": data_uri(image),
        "ai_model": "latest",
        "pose_mode": "a-pose",
        "should_texture": True,
        "should_remesh": True,
        "target_polycount": 30000,
        "target_formats": ["glb"],
        "multi_view_thumbnails": True,
    }
    tid = request("POST", "v1/image-to-3d", body)["result"]
    t = tasks(name)
    t["model"] = tid
    save(name, t)
    print(f"{name}: model {tid}")


def cmd_rig(name):
    t = tasks(name)
    model = t.get("multi") or t["model"]
    tid = request("POST", "v1/rigging", {"input_task_id": model, "height_meters": 1.6})["result"]
    t["rig"] = tid
    save(name, t)
    print(f"{name}: rig {tid}")


def cmd_anim(name, ids):
    t = tasks(name)
    anims = t.setdefault("anim", {})
    for action in ids:
        if str(action) in anims:
            print(f"{name}: anim {action} уже заказан ({anims[str(action)]})")
            continue
        tid = request("POST", "v1/animations", {"rig_task_id": t["rig"], "action_id": int(action)})["result"]
        anims[str(action)] = tid
        save(name, t)   # сразу: задача уже оплачена, номер нельзя потерять
        print(f"{name}: anim {action} {tid}")
        time.sleep(3)   # API отвечает 429 на частые запросы


def all_tasks(t):
    for view, tid in t.get("ref", {}).items():
        yield "ref", f"ref_{view}", tid
    for kind in ("model", "multi", "rig"):
        if kind in t:
            yield kind, kind, t[kind]
    for action, tid in t.get("anim", {}).items():
        yield "anim", f"anim_{action}", tid


def cmd_status(name, wait=False):
    t = tasks(name)
    pending = True
    while pending:
        pending = False
        for kind, label, tid in all_tasks(t):
            out = folder(name) / (f"{label}.png" if kind == "ref" else f"{label}.glb")
            if out.exists():
                continue
            path, url_of = KINDS[kind]
            r = request("GET", f"{path}/{tid}")
            status = r.get("status")
            print(f"{name} {label}: {status} {r.get('progress', '')}% credits={r.get('consumed_credits')}")
            if status == "SUCCEEDED":
                url = url_of(r)
                urllib.request.urlretrieve(url, out)
                if kind in ("model", "multi") and r.get("thumbnail_urls"):
                    for side, u in r["thumbnail_urls"].items():
                        urllib.request.urlretrieve(u, folder(name) / f"thumb_{side}.png")
                print(f"  -> {out.relative_to(ROOT)}")
            elif status in ("FAILED", "CANCELED"):
                print(f"  ошибка: {r.get('task_error')}")
            else:
                pending = True
        if pending and wait:
            time.sleep(15)
        else:
            break


def main():
    cmd, name, *rest = sys.argv[1:]
    if cmd == "ref":
        cmd_ref(name, rest[0], rest[1], rest[2:])
    elif cmd == "multi":
        cmd_multi(name, rest)
    elif cmd == "model":
        cmd_model(name, rest[0])
    elif cmd == "rig":
        cmd_rig(name)
    elif cmd == "anim":
        cmd_anim(name, rest)
    elif cmd in ("status", "wait"):
        cmd_status(name, wait=cmd == "wait")
    else:
        sys.exit(__doc__)


if __name__ == "__main__":
    main()
