# Skybound Corsairs — заметки для Claude

Мобильная 2D-гача на Rust + Bevy. Команда из двух человек, код пишется с помощью Claude.
Общайся с командой по-русски; комментарии в коде и тексты для игрока тоже на русском.

## Версии

- **Bevy 0.20.1, зафиксирована** в корневом `Cargo.toml`. API Bevy меняется между версиями:
  не пиши код по памяти о старых версиях. Сомневаешься — смотри исходники в
  `~/.cargo/registry/src/*/bevy_*-0.20.1/`. Например, в 0.20 `TextFont.font_size` имеет тип
  `FontSize` (`FontSize::Px(16.0)`), а выход из приложения — `MessageWriter<AppExit>`.
- Обновлять Bevy только отдельной задачей по официальному migration guide.

## Архитектура

- `battle-core` не зависит от Bevy и не должен. Вся логика боя здесь, движок только
  показывает `Event`. Случайность — только через `battle_core::Rng` с seed (бой
  детерминирован: это нужно для повторов и проверки на сервере).
- Баланс (герои, враги, этапы) — в `data/*.json`, а не в коде. Клиент вшивает данные при
  сборке (`GameData::bundled`), утилиты читают их с диска (`GameData::load_dir`).
- Экономика (валюта, крутки, покупки) будет считаться только на сервере (Firebase Cloud
  Functions). Клиенту не доверяем.

## Проверка изменений

1. `cargo test --workspace` и `cargo clippy --workspace --all-targets` без предупреждений.
2. Изменил бой или данные — `cargo run -p game-data --example simulate --release`.
3. Изменил экран — `SKYBOUND_SCREENSHOT=<путь>.png cargo run -p game` и посмотри кадр.

## Коммиты

- Без строки `Co-Authored-By: Claude …` и любых упоминаний Claude в сообщениях коммитов и PR.
- Коммитить обычным `git commit`.

## Шрифты

Стандартный шрифт Bevy не содержит кириллицу. Используй Rubik (Medium/SemiBold/ExtraBold) из
`crates/game/assets/fonts/`. Символов вроде ▶ → − в нём может не быть: проверяй на скриншоте.


# Skybound Corsairs — project rules (studioigor)

The game "Skybound Corsairs" is built with the studioigor pipeline. The agent leads the user step by step
from idea to release; the user decides taste and direction and plays often. Run the
pipeline work through the studioigor skill (`/home/nusip/.claude/skills/studioigor/SKILL.md`, phase rules in
`/home/nusip/.claude/skills/studioigor/references/`).

## Start of every session
1. `python3 /home/nusip/.claude/skills/studioigor/scripts/studio.py status` — shows the phase, open gates, what's
   next, learned items for this phase (accepted improvements and user preferences) and
   warnings. Do what it says.
2. Read `.studioigor/STATE.md`: "Now", open questions, session log.
3. What is recorded in `.studioigor/` (especially DECISIONS.md) is already decided: do not
   ask again and do not rebuild it. Trust the disk more than context memory.
4. `status` says "AUTOPILOT ACTIVE" and you are not an autopilot tick — do not edit the
   project: independent development is running. To intervene — `/home/nusip/.claude/skills/studioigor/scripts/autopilot.py stop`.

If you are a subagent with a task from the lead — do not run `status`: it ticks gates and
changes files. Work on the task and return the contract from your role.

## Game first
- Effort and tokens go into the game itself: gameplay, feel, assets, levels, sound, story.
- Tests — only where a bug silently breaks the game: economy and damage formulas, saves,
  determinism, a bug that came back a second time. No UI tests, no coverage for coverage's sake.
- Documents — short one-line decision records. No README essays and no code docs.
- The main check is a CAPTURE frame you looked at yourself, and the user's playtest.
- Several steps in a row went into tests, docs or tools — return to the game.
- This is a game, not a tech demo: a goal, menu, progression, sound, story, win and loss. At
  a playtest the first question is "is it fun?".

## Questions to the user
- Ask only for decisions (taste, scale, direction, money, accounts, publishing) and
  only via `AskUserQuestion`: 1–4 questions, 2–4 options, the recommended one first
  marked "(Recommended)", "you decide" is always available.
- Find out facts (files, installed software, APIs, documentation) yourself.
- Record the decision: `studio.py decide "…" --by user|agent --why "…"`. The user
  stepped away (`/loop`) — don't ask, decide yourself `--by agent`. Autopilot (independent
  development) — taste decisions `--by proxy`, user gates `tick "…" --proxy`, rules in
  `/home/nusip/.claude/skills/studioigor/references/autopilot.md`.
- "Always do it this way" / "I don't like X" — right away
  `python3 /home/nusip/.claude/skills/studioigor/scripts/learn.py prefer "…"`: it stays in this game
  (`.studioigor/PREFERENCES.md`). `--global` only for a rule about how to work with the user
  in any game, never for this game's taste.

## Git
- Commit at every step (a decision, closed gates, a locked mechanic):
  `python3 /home/nusip/.claude/skills/studioigor/scripts/studio.py commit "…"`.
- Never push or publish unless the user asked for it.
- Subagents that change files work in a git worktree (`isolation: "worktree"`).
  The lead reviews the diff, merges one at a time and checks the game after each merge.
  `.claude/worktrees/` must be in .gitignore. The procedure is in `/home/nusip/.claude/skills/studioigor/references/team.md`.
- Live Blender (blender-mcp) and the engine editor via MCP — main session only.
  Subagents work headless.

## Commands and code
- Verified commands live in `.studioigor/ENV.md`: RUN (the user sees the game), TEST,
  BUILD, CAPTURE (frames in `.studioigor/captures/`), EDITOR. Take them from there, don't make them up.
  A command changed — update ENV.md and run `studio.py verify`.
- The code standard is `/home/nusip/.claude/skills/studioigor/references/code.md`: tuning parameters in data files, simple
  architecture with nothing "for the future".
- Studio roles live in `.claude/agents/`: game-designer, art-director, tech-artist,
  gameplay-programmer, level-designer, sound-designer, narrative-designer, qa-tester.

## Do not touch
- Never edit the skill's files (`/home/nusip/.claude/skills/studioigor/SKILL.md`, `references/`, `scripts/`).
  Found a better way — `learn.py propose`; `status` will show what was accepted.
- Change the `.studioigor/` state through `studio.py` (decide, tick, note, commit) when
  there is a command for it.
