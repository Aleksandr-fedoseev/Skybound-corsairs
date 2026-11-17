# Decisions

One line = one decision. `by`: user — the user decided; agent — the agent decided on its own
(the user chose "you decide" or the question wasn't worth their time). A cancelled
decision is not deleted: a new line says "replaces #N".

| # | date | phase | decision | options | by | why |
|---|---|---|---|---|---|---|
| 1 | 2026-11-18 | 3 | платформы: Android первой, iOS после мягкого запуска; RU+EN | — | user | концепт игры, раздел «Аудитория и платформы» |
| 2 | 2026-11-18 | 3 | движок: Rust + Bevy 0.20.1 (зафиксирован), 2D; сервер Firebase | — | user | концепт игры, раздел «Технологии»; проект уже на Bevy |
| 3 | 2026-11-18 | 0 | существующий проект: работа в ветке studioigor, слияние в main только с согласия | — | agent | правило скилла для существующей игры |
| 4 | 2026-11-18 | 4 | аудит кода: исправить стыки — 1) константы боя и ИИ в коде, а не в data/; 2) сила залпа задана дважды; 3) клиент нельзя запустить на нужном этапе/seed для gym. Архитектура (бой без движка, детерминизм, данные в JSON) хорошая | — | agent | references/code.md §12 |
