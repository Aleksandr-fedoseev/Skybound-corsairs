#!/usr/bin/env python3
"""Вписать уровни врагов из `cargo run -p game-data --example retune` (строка RESULT) в data/.

Запуск: python3 tools/apply_retune.py result.json
stages.json и events.json — в формате json.dump(indent=2); в convoys.json меняются только числа
уровней в строках "enemies", остальное оформление файла остаётся как есть.
"""
import json
import re
import sys
from pathlib import Path

DATA = Path(__file__).resolve().parent.parent / "data"
res = json.loads(Path(sys.argv[1]).read_text())


def dump(path, value):
    path.write_text(json.dumps(value, indent=2, ensure_ascii=False) + "\n")


stages = json.loads((DATA / "stages.json").read_text())
for stage, levels in zip(stages, res["stages"]):
    for enemy, level in zip(stage["enemies"], levels):
        enemy["level"] = level
dump(DATA / "stages.json", stages)

events = json.loads((DATA / "events.json").read_text())
for key, part in (("regatta", "legs"),):
    for fight, levels in zip(events[key][part], res[key]):
        for pair, level in zip(fight["enemies"], levels):
            pair[1] = level
dump(DATA / "events.json", events)

text = (DATA / "convoys.json").read_text()
lines = iter(res["convoys"])


def tier(m):
    levels = iter(next(lines))
    return re.sub(r'(\["[a-z_]+", )\d+\]', lambda p: f"{p.group(1)}{next(levels)}]", m.group(0))


text = re.sub(r'"enemies": \[\[.*?\]\]', tier, text)
(DATA / "convoys.json").write_text(text)
print("уровни вписаны")
