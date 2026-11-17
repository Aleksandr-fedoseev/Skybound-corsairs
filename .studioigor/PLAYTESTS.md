# Playtests

One session — one section. The user's feedback — verbatim. The first question is always "is it fun?".

<!--
## PT-01 — 2026-09-28 — M01 Drift (gym)
- build: <commit> · run: `<command>`
- what we tried: …
- is it fun? … · feedback (verbatim): …
- conclusions → actions: …
-->

## PT-01 — 2026-11-18 — M06 Автобой «Живая палуба» (gym)
- build: 9075457 · run: `cargo run -p game`
- what we tried: бой на x1/x3, ручные ульты без АВТО, босс (клавиша 4), уровень героев, перечитывание data/
- is it fun? «Да, затягивает» · feedback (verbatim): «Экран и раскладка, Темп, Удар не чувствуется»
- conclusions → actions: оставляем «Живую палубу»; правим раскладку, темп и ощущение удара; баланс (100% побед) — отдельно

## PT-02 — 2026-11-18 — M06 Автобой «Живая палуба» v2 (gym)
- build: 4c48fdb · run: `cargo run -p game`
- what we tried: босс без АВТО, ручные ульты; новый ритм, эффекты удара, раскладка
- is it fun? «Заметно лучше» · feedback (verbatim): «Заметно лучше»
- conclusions → actions: M06 locked; дальше — баланс (сейчас 100% побед без потерь), чтобы ручные ульты решали

## PT-03 — 2026-11-18 — M06 v3: поле боя с моделями (gym)
- build: 2cc6113 · run: `cargo run -p game`
- what we tried: рывки, отброс, смерть, ручные ульты кнопками, босс, переключение раскладок V
- is it fun? не спрашивали отдельно · feedback (verbatim): раскладка — «Пока не решил»; доработать — «Интерфейс, Баланс»
- conclusions → actions: обе раскладки остаются до референсов и первых спрайтов; дальше — интерфейс, затем баланс
