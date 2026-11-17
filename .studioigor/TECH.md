# Technology — Skybound Corsairs

<!-- Phase 3 (references/stack.md). ENGINE — one of: godot, godot-mono, unity,
unreal, web, defold, bevy (read by studio.py and envcheck.py). DIMENSION — 2D or 3D
(2.5D with 3D models is 3D); envcheck.py recommends Blender for 3D. -->
ENGINE: bevy
ENGINE VERSION: 0.20.1
DIMENSION: 2D
LANGUAGE: Rust (edition 2024); сервер — TypeScript (Firebase Cloud Functions)
PLATFORMS: Android (первая), iOS после мягкого запуска; Linux — для разработки

## Decision
- context: мобильная 2D-гача, вертикальный экран, детерминированный бой нужен и клиенту, и серверу для проверки.
- options: решено командой до studioigor (концепт, раздел «Технологии»); альтернативы не сравнивались.
- choice and why: Rust + Bevy, бой — отдельный крейт без движка; сервер Firebase (Supabase и Nakama — открытый вопрос концепта).
- consequences: API Bevy ломается между 0.x — версия зафиксирована; покупки и реклама через свой мост Kotlin ↔ Rust (JNI); клиент ↔ Firebase через REST; корабль и отсеки — Blender → 2D-спрайты.

## Budgets
60 fps на бюджетном Android в бою с 4+5 бойцами и эффектами ульт; размер APK — уточнить в фазе 10.

## Project structure
- crates/battle-core — логика боя, без Bevy, тесты
- crates/game-data — загрузка data/*.json → бойцы
- crates/game — клиент на Bevy (бинарник skybound), assets/fonts
- data/ — баланс в JSON
