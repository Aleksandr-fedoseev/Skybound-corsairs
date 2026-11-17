# Scope — the contract in numbers

What "the game is done" means for this project. The scale is agreed in phase 1,
the numbers are refined in phase 8. Lines are never deleted — they are ticked or marked
`BLOCKED: <why>`. Counted by `scope.py`.

Line syntax:

    - [ ] <name>: <N> <unit> — count: <command that prints a number>

## Reference inventory
Концепт: этап 1 — 5–6 героев, 3–4 типа врагов, 1 босс, 10 этапов главы 1. Запуск — 15–20 героев (4–5 SSR).

## Contract
- [ ] Этапы первой главы: 10 шт — count: python3 -c "import json;print(len(json.load(open('data/stages.json'))))"
- [ ] Герои прототипа: 6 шт — count: python3 -c "import json;print(len(json.load(open('data/heroes.json'))))"
- [ ] Типы врагов с боссом: 4 шт — count: python3 -c "import json;print(len(json.load(open('data/enemies.json'))))"
- [ ] Герои к запуску: 15 шт — count: python3 -c "import json;print(len(json.load(open('data/heroes.json'))))"
