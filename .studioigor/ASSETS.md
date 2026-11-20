# Assets — Skybound Corsairs

<!-- Phase 5 (references/assets.md). -->

## Inventory
<!-- one line per asset card (references/assets.md): name · category · style · budget · source · status -->
| asset | category | style | budget | source | status |
|---|---|---|---|---|---|

## Strategy by category
| category | source (pack / Blender / procedural / generation / user) | why | status |
|---|---|---|---|
| бой (прототип) | заглушки: фигуры и текст в коде | стиль ещё не выбран (решение #10) | plan |

## Conventions
Отложено до фазы 5. Шрифты: Noto Sans (OFL) в crates/game/assets/fonts.

## Requests to the user
- none

## License registry
| asset / pack | source (URL) | license | author | path | attribution |
|---|---|---|---|---|---|
| позы Мэй, Соры, Лиры (по 4 рисунка) | Gemini (nano-banana), аккаунт пользователя, по разворотам из art/references/nano-banana | по условиям Google на сгенерированный контент — проверить перед релизом | команда | art/2d/{mei,sora,lira}, crates/game/assets/units/ | — |
| позы Каи (4 рисунка) | Gemini (nano-banana), аккаунт пользователя, по развороту из art/references/nano-banana | по условиям Google на сгенерированный контент — проверить перед релизом | команда | art/2d/kaya, crates/game/assets/units/kaya.png | — |
| фоны боя главы 1 (1-1, 1-2, 1-3, 1-boss) | Gemini (nano-banana), аккаунт пользователя | по условиям Google на сгенерированный контент — проверить перед релизом | команда | art/2d/bg, crates/game/assets/battle/1-*.jpg | — |
| позы врагов: морпех, стрелок, скат (по 3), левиафан (4) | Gemini (nano-banana), аккаунт пользователя, по разворотам из art/references/nano-banana/enemies | по условиям Google на сгенерированный контент — проверить перед релизом | команда | art/2d/{imperial_marine,imperial_gunner,storm_ray,storm_leviathan}, crates/game/assets/units/ | — |
| позы героинь: ульта, победа, поражение (Кая, Мэй, Сора, Лира), новый удар Каи | Gemini (nano-banana), аккаунт пользователя, по стойке героини | по условиям Google на сгенерированный контент — проверить перед релизом | команда | art/2d/{kaya,mei,sora,lira}, crates/game/assets/units/ | — |
| эффекты Соры: смерч, серп порыва | Gemini (nano-banana), аккаунты пользователя, рисунок на чёрном → прозрачность (tools/sprites/fx_textures.py) | по условиям Google на сгенерированный контент — проверить перед релизом | команда | art/2d/fx, crates/game/assets/fx/{tornado,gust}.png | — |
| позы Рэна: спокойная стойка, выпад (замах и укол) | Gemini (nano-banana), аккаунт пользователя, по его прежнему рисунку | по условиям Google на сгенерированный контент — проверить перед релизом | команда | art/2d/ren, crates/game/assets/units/ren.png | — |
| круговой удар Рэна (8 кадров оборота) | движение Double_Blade_Spin из библиотеки Meshy на его модели, кадры перерисованы в Gemini (tools/sprites/painted_frames.py) | Meshy — по подписке команды; Gemini — по условиям Google, проверить перед релизом | команда | art/meshy/ren, art/2d/ren/spin, crates/game/assets/units/ren.png | — |
| позы Рэна: ульта, удар, поражение, победа | Gemini (nano-banana), аккаунт пользователя, по развороту из art/references/nano-banana/heroes/ren.jpeg | по условиям Google на сгенерированный контент — проверить перед релизом | команда | art/2d/ren, crates/game/assets/units/ren.png | — |
| разрез и укол клинка | свои (tools/sprites/fx_textures.py) | свои | команда | crates/game/assets/fx/{slash,thrust}.png | — |
| кольцо кругового взмаха (fx/whirl.png) | сделано нами: tools/sprites/fx_textures.py | своё | команда | crates/game/assets/fx/whirl.png | — |
| грозовая туча, свечение | свои (шум и градиент в tools/sprites/fx_textures.py) | свои | команда | crates/game/assets/fx/{storm_cloud,glow}.png | — |
| спрайт-листы Каи и Мэй | Meshy (image-to-3D, авториг, библиотека анимаций) по референсам nano-banana | по условиям подписки Meshy пользователя — проверить перед релизом | команда | crates/game/assets/units/ | — |
