//! Общий вид экранов: рисованные детали интерфейса из `assets/ui/` (дерево с латунью,
//! пергамент, латунные кнопки, вкладки, иконки ресурсов, медальоны героев).
//!
//! Детали тянутся на любой размер нарезкой на 9 частей: углы не искажаются. Картинки
//! готовит `tools/make_ui.py`; промпты для рисованных версий — `.studioigor/UI_KIT.md`.

use bevy::prelude::*;
use bevy::sprite::{BorderRect, SliceScaleMode, TextureSlicer};

use crate::view::Fonts;

/// Тёмные чернила — текст на пергаменте.
pub const INK: Color = Color::srgb(0.27, 0.18, 0.11);
/// Приглушённые чернила — подписи на пергаменте.
pub const INK_SOFT: Color = Color::srgb(0.46, 0.35, 0.24);
/// Зелёные чернила — «станет лучше» на пергаменте.
pub const INK_GOOD: Color = Color::srgb(0.16, 0.45, 0.20);

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Look {
    /// Тёмное дерево в тонкой латунной кайме — панели и ленты (как ряд отсеков).
    Tile,
    /// Пергамент — карточки внутри панелей.
    Parchment,
    /// Латунная кнопка главного действия.
    Go,
    /// Тёмная кнопка второстепенного действия.
    Dark,
    /// Недоступная кнопка.
    Off,
    TabOn,
    TabOff,
    /// Доска-заголовок.
    Plank,
}

impl Look {
    fn file(self) -> &'static str {
        match self {
            Look::Tile => "",
            Look::Parchment => "ui/card.png",
            Look::Go => "ui/button.png",
            Look::Dark => "ui/button_dark.png",
            Look::Off => "ui/button_off.png",
            Look::TabOn => "ui/tab_on.png",
            Look::TabOff => "ui/tab_off.png",
            Look::Plank => "ui/plank.png",
        }
    }

    /// Ширина неизменяемой рамки картинки в пикселях.
    /// Неизменяемая рамка картинки (по горизонтали, по вертикали) в пикселях.
    fn border(self) -> BorderRect {
        let (x, y) = match self {
            Look::Tile => (0.0, 0.0),
            Look::Parchment => (16.0, 16.0),
            // Болты на концах доски не растягиваются.
            Look::Plank => (58.0, 16.0),
            _ => (18.0, 18.0),
        };
        BorderRect {
            min_inset: Vec2::new(x, y),
            max_inset: Vec2::new(x, y),
        }
    }
}

/// Подложка узла: картинку подставит `apply_skins` (не нужно протаскивать AssetServer).
#[derive(Component, Clone, Copy)]
pub struct Skin(pub Look);

pub fn plugin(app: &mut App) {
    // После Update: узлы, собранные за кадр, получают картинку до отрисовки.
    app.add_systems(PostUpdate, apply_skins);
}

fn apply_skins(
    mut commands: Commands,
    assets: Res<AssetServer>,
    mut nodes: Query<(Entity, &Skin, &mut Node), Changed<Skin>>,
) {
    for (entity, look, mut node) in &mut nodes {
        if look.0 == Look::Tile {
            // Без картинки: заливка, тонкая латунная кайма и скругление.
            node.border = UiRect::all(px(2));
            node.border_radius = BorderRadius::all(px(14));
            commands.entity(entity).insert((
                BackgroundColor(Color::srgb(0.07, 0.05, 0.03)),
                BorderColor::all(Color::srgb(0.79, 0.59, 0.25)),
            ));
        } else {
            commands.entity(entity).insert(skin(&assets, look.0));
        }
    }
}

/// Картинка-подложка узла, нарезанная на 9 частей.
pub fn skin(assets: &AssetServer, look: Look) -> ImageNode {
    ImageNode {
        image: assets.load(look.file()),
        image_mode: NodeImageMode::Sliced(TextureSlicer {
            border: look.border(),
            center_scale_mode: SliceScaleMode::Stretch,
            sides_scale_mode: SliceScaleMode::Stretch,
            max_corner_scale: 1.0,
        }),
        // Подложка — на весь узел, а не только внутри отступов.
        visual_box: VisualBox::BorderBox,
        ..default()
    }
}

/// Иконка ресурса: silver, planks, ether, journals, doubloons, charts, details.
pub fn icon(assets: &AssetServer, kind: &str, size: f32) -> impl Bundle {
    (
        Node {
            width: px(size),
            height: px(size),
            flex_shrink: 0.0,
            ..default()
        },
        ImageNode::new(assets.load(format!("ui/icon_{kind}.png"))),
        Pickable::IGNORE,
    )
}

/// Медальон героя: портрет в кольце редкости. `dim` — героя ещё нет.
pub fn medallion(assets: &AssetServer, hero: &str, size: f32, dim: bool) -> impl Bundle {
    (
        Node {
            width: px(size),
            height: px(size),
            flex_shrink: 0.0,
            ..default()
        },
        ImageNode {
            image: assets.load(format!("ui/hero_{hero}.png")),
            color: if dim {
                Color::srgba(0.45, 0.45, 0.5, 0.75)
            } else {
                Color::WHITE
            },
            ..default()
        },
        Pickable::IGNORE,
    )
}

/// Строка «иконка число · иконка число».
pub fn costs(
    parent: &mut ChildSpawnerCommands,
    assets: &AssetServer,
    fonts: &Fonts,
    items: &[(&str, u32)],
    size: f32,
    color: Color,
) {
    parent
        .spawn((
            Node {
                column_gap: px(4),
                align_items: AlignItems::Center,
                ..default()
            },
            Pickable::IGNORE,
        ))
        .with_children(|row| {
            for (i, (kind, n)) in items.iter().enumerate() {
                row.spawn(icon(assets, kind, size + 4.0));
                row.spawn((
                    Node {
                        margin: UiRect::right(px(if i + 1 < items.len() { 8 } else { 0 })),
                        ..default()
                    },
                    Text::new(n.to_string()),
                    fonts.serif(size + 3.0),
                    TextColor(color),
                    Pickable::IGNORE,
                ));
            }
        });
}

/// Полоса ресурсов вверху экрана: тёмные плашки с иконкой слева.
/// Число для плашки ресурса: до 10 000 — как есть, дальше — «12,5к», «125к», «1,2м».
pub fn compact(n: u32) -> String {
    let short = |v: f64, unit: &str| {
        if v < 100.0 {
            format!("{:.1}{unit}", (v * 10.0).floor() / 10.0).replace('.', ",")
        } else {
            format!("{}{unit}", v.floor())
        }
    };
    match n {
        0..10_000 => n.to_string(),
        10_000..1_000_000 => short(n as f64 / 1000.0, "к"),
        _ => short(n as f64 / 1_000_000.0, "м"),
    }
}

pub fn top_bar(
    root: &mut ChildSpawnerCommands,
    assets: &AssetServer,
    fonts: &Fonts,
    resources: &[(&str, u32)],
) {
    bar(root).with_children(|bar| {
        for (kind, n) in resources {
            resource_slot(bar, assets, fonts, kind, compact(*n), 18.0, ());
        }
    });
}

/// Пустая полоса ресурсов: тёмное дерево с латунной кромкой снизу.
pub fn bar<'a>(root: &'a mut ChildSpawnerCommands) -> EntityCommands<'a> {
    root.spawn((
        Node {
            position_type: PositionType::Absolute,
            top: px(0),
            left: px(0),
            right: px(0),
            height: px(54),
            padding: UiRect::axes(px(12), px(7)),
            column_gap: px(12),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            border: UiRect::bottom(px(2)),
            ..default()
        },
        BackgroundColor(Color::srgba(0.09, 0.06, 0.04, 0.92)),
        BorderColor::all(Color::srgb(0.62, 0.46, 0.20)),
    ))
}

/// Плашка ресурса: крупная иконка слева, число засечкой. `extra` — на текст числа
/// (например, метка, по которой число обновляется).
pub fn resource_slot<'a>(
    bar: &'a mut ChildSpawnerCommands,
    assets: &AssetServer,
    fonts: &Fonts,
    kind: &str,
    value: String,
    size: f32,
    extra: impl Bundle,
) -> EntityCommands<'a> {
    let mut slot = bar.spawn(Node {
        flex_grow: 1.0,
        flex_basis: px(0),
        height: px(40),
        align_items: AlignItems::Center,
        ..default()
    });
    slot.with_children(|slot| {
        slot.spawn((
            Node {
                position_type: PositionType::Absolute,
                left: px(16),
                right: px(0),
                top: px(4),
                bottom: px(4),
                padding: UiRect::left(px(22)),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            ImageNode {
                image: assets.load("ui/pill.png"),
                image_mode: NodeImageMode::Sliced(TextureSlicer {
                    border: BorderRect::all(20.0),
                    ..default()
                }),
                ..default()
            },
        ))
        .with_child((
            heading(fonts, value, size, Color::srgb(1.0, 0.93, 0.78)),
            extra,
        ));
        slot.spawn(icon(assets, kind, 38.0));
    });
    slot
}

/// Доска-заголовок экрана по центру.
pub fn title(root: &mut ChildSpawnerCommands, fonts: &Fonts, label: &str, top: f32) {
    title_in(root, fonts, label, top, 0.0);
}

/// Заголовок на экране с кнопкой «Назад» слева: доска по центру места справа от кнопки,
/// чтобы между ними оставался зазор.
pub fn title_beside_back(root: &mut ChildSpawnerCommands, fonts: &Fonts, label: &str, top: f32) {
    title_in(root, fonts, label, top, BACK_WIDTH);
}

/// Сколько места слева занимает кнопка «Назад» вместе с зазором до заголовка.
const BACK_WIDTH: f32 = 132.0;

fn title_in(root: &mut ChildSpawnerCommands, fonts: &Fonts, label: &str, top: f32, left: f32) {
    root.spawn(Node {
        position_type: PositionType::Absolute,
        top: px(top),
        left: px(left),
        right: px(if left > 0.0 { 12.0 } else { 0.0 }),
        justify_content: JustifyContent::Center,
        ..default()
    })
    .with_children(|row| {
        row.spawn((
            Node {
                height: px(48),
                padding: UiRect::horizontal(px(44)),
                align_items: AlignItems::Center,
                ..default()
            },
            Skin(Look::Plank),
        ))
        .with_child(heading(fonts, label, 24.0, Color::srgb(1.0, 0.93, 0.78)));
    });
}

/// Заголовок, надпись на кнопке, крупная цифра: засечка с тенью.
pub fn heading(fonts: &Fonts, value: impl Into<String>, size: f32, color: Color) -> impl Bundle {
    (
        Text::new(value),
        fonts.display(size),
        TextColor(color),
        TextShadow {
            offset: Vec2::new(0.0, 2.0),
            color: Color::srgba(0.0, 0.0, 0.0, 0.75),
        },
        Pickable::IGNORE,
    )
}

/// Надпись чернилами на пергаменте: засечка без тени.
pub fn ink(fonts: &Fonts, value: impl Into<String>, size: f32, color: Color) -> impl Bundle {
    (
        Text::new(value),
        fonts.serif(size),
        TextColor(color),
        Pickable::IGNORE,
    )
}

/// Награда иконками на тёмной плашке: подпись слева, дальше «[иконка] 120 [иконка] 40».
pub fn reward_row(
    parent: &mut ChildSpawnerCommands,
    assets: &AssetServer,
    fonts: &Fonts,
    label: &str,
    items: &[(&str, u32)],
) {
    parent
        .spawn((
            Node {
                padding: UiRect::axes(px(10), px(5)),
                column_gap: px(8),
                align_items: AlignItems::Center,
                align_self: AlignSelf::FlexStart,
                border_radius: BorderRadius::all(px(12)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.35)),
        ))
        .with_children(|row| {
            if !label.is_empty() {
                row.spawn(heading(fonts, label, 13.0, Color::srgb(0.8, 0.72, 0.58)));
            }
            costs(row, assets, fonts, items, 16.0, Color::WHITE);
        });
}

/// Кружки этапов с номерами: пройденные — золотые, текущий — светлый ободок, дальше — тёмные.
pub fn pips(
    parent: &mut ChildSpawnerCommands,
    fonts: &Fonts,
    total: usize,
    done: usize,
    current: Option<usize>,
) {
    parent
        .spawn(Node {
            column_gap: px(6),
            align_items: AlignItems::Center,
            ..default()
        })
        .with_children(|row| {
            for i in 0..total {
                let (bg, border, color) = if i < done {
                    (
                        Color::srgb(0.78, 0.58, 0.18),
                        Color::srgb(1.0, 0.86, 0.5),
                        Color::WHITE,
                    )
                } else if Some(i) == current {
                    (
                        Color::srgb(0.25, 0.2, 0.16),
                        Color::srgb(1.0, 0.93, 0.78),
                        Color::WHITE,
                    )
                } else {
                    (
                        Color::srgb(0.14, 0.11, 0.1),
                        Color::srgb(0.35, 0.3, 0.26),
                        Color::srgb(0.55, 0.5, 0.45),
                    )
                };
                row.spawn((
                    Node {
                        width: px(34),
                        height: px(34),
                        justify_content: JustifyContent::Center,
                        align_items: AlignItems::Center,
                        border: UiRect::all(px(if Some(i) == current { 3 } else { 2 })),
                        border_radius: BorderRadius::all(px(17)),
                        ..default()
                    },
                    BackgroundColor(bg),
                    BorderColor::all(border),
                ))
                .with_child(heading(fonts, (i + 1).to_string(), 15.0, color));
            }
        });
}

/// Небольшая плашка с текстом: время, статус, условие.
pub fn pill(parent: &mut ChildSpawnerCommands, fonts: &Fonts, label: impl Into<String>, bg: Color) {
    parent
        .spawn((
            Node {
                padding: UiRect::axes(px(10), px(4)),
                flex_shrink: 0.0,
                border_radius: BorderRadius::all(px(12)),
                ..default()
            },
            BackgroundColor(bg),
        ))
        .with_child(heading(fonts, label, 13.0, Color::WHITE));
}

/// Полоса прогресса с подписью поверх: «12 400 / 20 000».
pub fn meter(
    parent: &mut ChildSpawnerCommands,
    fonts: &Fonts,
    frac: f32,
    label: impl Into<String>,
    fill: Color,
) {
    parent
        .spawn((
            Node {
                height: px(26),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                border: UiRect::all(px(2)),
                border_radius: BorderRadius::all(px(13)),
                overflow: Overflow::clip(),
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.5)),
            BorderColor::all(Color::srgb(0.45, 0.36, 0.25)),
        ))
        .with_children(|bar| {
            bar.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    left: px(0),
                    top: px(0),
                    bottom: px(0),
                    width: percent(frac.clamp(0.0, 1.0) * 100.0),
                    ..default()
                },
                BackgroundColor(fill),
            ));
            bar.spawn(heading(fonts, label, 14.0, Color::WHITE));
        });
}
