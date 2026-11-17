# Environment — Skybound Corsairs

<!-- Phase 4 (references/environment.md). Commands verified by a real run.
VERIFIED is switched only by `studio.py verify`. CAPTURE writes images to
.studioigor/captures/. -->
VERIFIED: yes

## Installed
| tool | version | why |
|---|---|---|
| rustc / cargo (rustup stable) | 1.99.1 | сборка |
| clippy | 1.99.1 | линтер |
| Bevy | 0.20.1 (зафиксирована) | движок |
| системные библиотеки: alsa, udev, wayland, x11, xkbcommon, vulkan | есть | окно, звук, ввод, рендер |
| GPU RX 570, Mesa radv, Vulkan | 26.2.5 | рендер |
| clang + lld | системные | можно ускорить линковку |
| rust-analyzer (компонент rustup) | 1.99.1 | LSP |
| bevy_brp_mcp | 0.23.0 (~/.cargo/bin) | MCP для живой игры |
| bevy_brp_extras | 0.23.0, фича dev клиента | скриншоты, клавиши, мышь через BRP |
| node / npm | системные | будущие Cloud Functions |

## Agent integrations
- Плагин ассистента rust-analyzer-lsp (scope user).
- MCP bevy-brp: `mcp add bevy-brp -- /home/nusip/.cargo/bin/bevy_brp_mcp` (local, этот проект); `mcp list` — Connected.
- Проверено curl на порт 15702: rpc.discover отдаёт методы brp_extras, screenshot сохранил PNG, send_keys и shutdown ответили success.
- Инструменты MCP появятся в новой сессии ассистента.

## Commands
RUN: cargo run -p game
TEST: cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings
BUILD: cargo build --release -p game --no-default-features
CAPTURE: mkdir -p .studioigor/captures/hello && SKYBOUND_SCREENSHOT=.studioigor/captures/hello/battle.png cargo run -p game
EDITOR: none — у Bevy нет редактора, код в репозитории

## Pitfalls
- niri (тайловый Wayland) растягивал окно — теперь окно всегда 540×960 и неизменяемого размера (main.rs).
- Игру запускать через cargo run: бинарник, запущенный напрямую, ищет assets/ рядом с exe и рисует экран без текста.
- MCP-сервер не в PATH ассистента — регистрировать по абсолютному пути ~/.cargo/bin/bevy_brp_mcp.
- Не убивать cargo посреди сборки: битые .rlib («memory map must have a non-zero length»), лечится пересборкой.
- Фича dev включена по умолчанию; релиз — только с --no-default-features (BRP открывает порт 15702).
- BRP-клик: сначала brp_extras/move_mouse с position, потом click_mouse (у click_mouse нет координат). Позиции карточек — world.query ComputedNode + UiGlobalTransform.
- Под niri невидимое окно почти не получает кадров: shutdown и таймеры игры идут медленно.
- Нет Android SDK/NDK и target aarch64-linux-android — до сборки под телефон.
- Стандартный шрифт Bevy без кириллицы — только Noto Sans из crates/game/assets/fonts.
