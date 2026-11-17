# Окружение для Bevy 0.20.1 — 2026-11-18

Источники: docs.rs/bevy_brp_mcp (таблица совместимости), envcheck.py, локальная проверка.

## Выводы
- bevy_brp_mcp 0.23.0 — версия для Bevy 0.20 (0.22.x — 0.19). Репозиторий natepiano/bevy_brp_mcp
  архивирован, MCP теперь в workspace natepiano/bevy_brp (каталог mcp). Версию bevy_brp_extras
  для 0.20 уточнить на crates.io при установке.
- На Bevy 0.20 rpc_discover отдаёт имена методов без описаний и схем — инструментам это не мешает.
- BRP даёт агенту скриншоты, чтение/запись компонентов и ввод в живой игре. Подключать в клиент
  только под cargo-фичей dev (не в релиз).
- rust-analyzer: компонент rustup не установлен (`rustup component add rust-analyzer`), плюс
  плагин ассистента rust-analyzer-lsp.
- bevy_cli (alpha) — по желанию, сейчас не нужен: cargo run хватает.
- brew в envcheck — шаблон под macOS; на CachyOS ставим через pacman (git-lfs, yt-dlp; нужен sudo).
- cargo search из песочницы падает с TLS-ошибкой к crates.io — сеть для cargo проверить при установке.

## Что меняет
- Список установки: rust-analyzer (компонент + плагин), bevy_brp_mcp 0.23 + extras в клиент под фичей dev,
  git-lfs (позже, когда появятся спрайты), yt-dlp (для референсов, по желанию).
