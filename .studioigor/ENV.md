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
| node / npm | системные | будущие Cloud Functions |

## Agent integrations
Пока нет: MCP для Bevy (BRP), LSP-плагин rust-analyzer — решить в фазе 4.

## Commands
RUN: cargo run -p game
TEST: cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings
BUILD: cargo build --release -p game
CAPTURE: mkdir -p .studioigor/captures/hello && SKYBOUND_SCREENSHOT=.studioigor/captures/hello/battle.png cargo run -p game
EDITOR: none — у Bevy нет редактора, код в репозитории

## Pitfalls
- niri (тайловый Wayland) растягивает окно; в режиме SKYBOUND_SCREENSHOT размер окна зафиксирован (dev::fix_window_size), кадр 540×960.
- Нет Android SDK/NDK и target aarch64-linux-android — до сборки под телефон.
- Стандартный шрифт Bevy без кириллицы — только Noto Sans из crates/game/assets/fonts.
