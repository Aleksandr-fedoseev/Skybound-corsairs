# UI-кит: что сгенерировать в Gemini

Генерирует всё `python3 tools/gemini_ui.py` (Gemini API, ключ — вне репозитория, в
`~/.config/skybound-corsairs/gemini.key`), промпты — в самом скрипте. Вручную: рисованная версия заменяет временную
без правок кода: положить PNG в `assets/ui_src/<имя>` и запустить `python3 tools/make_ui.py`.

## Общие требования (добавлять к каждому промпту)

> 2D game UI asset for a mobile anime sky-pirate gacha game, painted style matching a
> fantasy airship over a sea of clouds: warm dark wood, polished brass rivets and trims,
> aged parchment, soft painterly shading, light from top-left. Orthographic front view,
> no perspective, no text, no letters, no characters. Isolated on a plain flat magenta
> background (#FF00FF) for cutting out. High resolution, clean edges.

- Фон — сплошной пурпурный (#FF00FF): его вырезаем. Без теней на фон.
- Рамки и углы — одинаковой толщины со всех сторон: деталь режется на 9 частей,
  середина и стороны тянутся, углы — нет. Узоры только в углах, стороны — ровные.
- Размеры ниже — итоговые после обрезки; генерировать можно крупнее в той же пропорции.

## Детали

| Файл | Размер | Рамка (не тянется) | Промпт (к общим требованиям) |
|---|---|---|---|
| panel.png | 192×192 | 26 px | Square panel made of dark vertical wooden planks with a thick polished brass frame, brass rivets in the four corners, slightly rounded corners. Plain even sides. |
| card.png | 128×128 | 16 px | Square piece of aged cream parchment with a thin brown inked border and slightly darker edges, rounded corners, very subtle texture, flat and readable. |
| plank.png | 512×64 | 20 px | Long horizontal wooden signboard plank with brass bolts at both ends and a thin brass edge, rounded corners. |
| button.png | 192×64 | 16 px | Wide rounded button of glossy amber-orange enamel in a polished brass bezel, gentle top highlight. |
| button_pressed.png | 192×64 | 16 px | Same amber button pressed in: darker, highlight at the bottom. |
| button_off.png | 192×64 | 16 px | Same button shape, desaturated grey enamel, dull brass bezel. |
| button_dark.png | 192×64 | 16 px | Same button shape, deep navy-blue enamel in a brass bezel. |
| tab_on.png | 192×64 | 16 px | Rounded tab of warm amber leather with brass stitching edge, active. |
| tab_off.png | 192×64 | 16 px | Same tab, dark brown leather, inactive, muted. |
| icon_silver.png | 64×64 | — | Single silver coin with an engraved airship, slight tilt. |
| icon_doubloons.png | 64×64 | — | Single gold doubloon coin with a skull-and-wings emblem. |
| icon_planks.png | 64×64 | — | Small stack of three wooden planks tied with rope. |
| icon_ether.png | 64×64 | — | Glowing teal ether crystal shard. |
| icon_journals.png | 64×64 | — | Red leather ship's logbook with brass corners. |
| icon_charts.png | 64×64 | — | Rolled navigation chart with a red route line, partly open. |
| icon_details.png | 64×64 | — | Brass gear cog with a small bolt. |

## Фоны экранов (540×960, вертикальные, без пурпурного фона)

Общая часть: *Vertical 9:16 painted background for a mobile anime sky-pirate game, same
style as a fantasy airship above a sea of clouds, warm light, soft depth of field, no
characters, no text, calm area in the middle for UI.*

| Файл | Экран | Промпт |
|---|---|---|
| assets/backgrounds/crew.png | Экипаж | Cozy crew quarters below deck of a wooden airship: hammocks, lanterns, maps on the walls, round portholes with clouds outside. |
| assets/backgrounds/tavern.png | Таверна | Lively sky-port tavern interior: wooden bar, hanging lanterns, notice board with wanted posters, window to the clouds. |
| assets/backgrounds/activities.png | Активности | View from the crow's nest over a vast stormy cloud abyss, distant floating islands and an imperial warship silhouette. |

## Борг

У Борга пока нет рисованных поз: нужны такие же, как у остальных героев (конвейер
напарника, `tools/sprites/keypose.py`, файл `crates/game/assets/units/borg.png` и запись в
`data/animated.json`). Медальон и фигура в Экипаже возьмутся из них сами при запуске
`tools/make_ui.py`.
