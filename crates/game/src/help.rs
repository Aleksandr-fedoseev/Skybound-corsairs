//! Справка: латунная кнопка «?» у каждого раздела и окно с короткими пунктами поверх экрана.
//! Тексты — `data/help.json`, по разделу на ключ. Пояснения живут здесь, а не на самих экранах.

use std::collections::HashMap;

use bevy::prelude::*;
use serde::Deserialize;

use crate::Screen;
use crate::menu::{BRASS, MUTED, PARCHMENT, pressable, text};
use crate::style::{self, Look, Skin};
use crate::view::Fonts;

#[derive(Deserialize)]
struct Entry {
    title: String,
    lines: Vec<String>,
}

#[derive(Resource, Deserialize)]
pub struct HelpData(HashMap<String, Entry>);

#[derive(Component)]
struct HelpOverlay;

pub fn plugin(app: &mut App) {
    app.insert_resource(
        serde_json::from_str::<HelpData>(include_str!("../../../data/help.json"))
            .expect("data/help.json корректен"),
    );
}

/// Кнопка «?» в углу экрана: `top`/`right` — от края экрана.
pub fn corner(root: &mut ChildSpawnerCommands, fonts: &Fonts, id: &str, top: f32, right: f32) {
    let mut b = root.spawn(Node {
        position_type: PositionType::Absolute,
        top: px(top),
        right: px(right),
        ..default()
    });
    b.with_children(|c| button(c, fonts, id));
}

/// Латунный кружок с «?»: тап — справка раздела `id`.
pub fn button(parent: &mut ChildSpawnerCommands, fonts: &Fonts, id: &str) {
    let id = id.to_string();
    let mut b = parent.spawn((
        Node {
            width: px(38),
            height: px(38),
            flex_shrink: 0.0,
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            border: UiRect::all(px(3)),
            border_radius: BorderRadius::MAX,
            ..default()
        },
        BackgroundColor(Color::srgb(0.2, 0.13, 0.08)),
        BorderColor::all(BRASS),
        BoxShadow::new(Color::srgba(0.0, 0.0, 0.0, 0.5), px(0), px(2), px(0), px(4)),
    ));
    b.with_child(style::heading(
        fonts,
        "?",
        22.0,
        Color::srgb(1.0, 0.85, 0.45),
    ));
    pressable(&mut b);
    b.observe(
        move |_: On<PointerClick>,
              mut commands: Commands,
              data: Res<HelpData>,
              fonts: Res<Fonts>,
              state: Res<State<Screen>>| {
            if let Some(entry) = data.0.get(&id) {
                open(&mut commands, &fonts, entry, *state.get());
            }
        },
    );
}

/// Окно справки поверх всего: заголовок-доска и пункты с латунными метками. Тап — закрыть.
fn open(commands: &mut Commands, fonts: &Fonts, entry: &Entry, screen: Screen) {
    let mut overlay = commands.spawn((
        HelpOverlay,
        DespawnOnExit(screen),
        Node {
            position_type: PositionType::Absolute,
            width: percent(100),
            height: percent(100),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            ..default()
        },
        BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.55)),
        GlobalZIndex(40),
    ));
    overlay.with_children(|o| {
        o.spawn((
            Node {
                width: px(460),
                padding: UiRect::new(px(22), px(22), px(16), px(18)),
                flex_direction: FlexDirection::Column,
                row_gap: px(12),
                border: UiRect::all(px(2)),
                border_radius: BorderRadius::all(px(16)),
                ..default()
            },
            Skin(Look::Tile),
            BoxShadow::new(
                Color::srgba(0.0, 0.0, 0.0, 0.6),
                px(0),
                px(6),
                px(0),
                px(14),
            ),
            Pickable::IGNORE,
        ))
        .with_children(|p| {
            p.spawn(Node {
                justify_content: JustifyContent::Center,
                margin: UiRect::bottom(px(4)),
                ..default()
            })
            .with_children(|row| {
                row.spawn((
                    Node {
                        height: px(46),
                        padding: UiRect::horizontal(px(26)),
                        align_items: AlignItems::Center,
                        ..default()
                    },
                    Skin(Look::Plank),
                ))
                .with_child(style::heading(
                    fonts,
                    entry.title.clone(),
                    22.0,
                    Color::srgb(1.0, 0.93, 0.78),
                ));
            });
            for line in &entry.lines {
                p.spawn(Node {
                    column_gap: px(12),
                    align_items: AlignItems::FlexStart,
                    ..default()
                })
                .with_children(|row| {
                    row.spawn((
                        Node {
                            width: px(10),
                            height: px(10),
                            flex_shrink: 0.0,
                            margin: UiRect::top(px(6)),
                            border_radius: BorderRadius::MAX,
                            ..default()
                        },
                        BackgroundColor(BRASS),
                    ));
                    row.spawn((
                        Node {
                            flex_shrink: 1.0,
                            ..default()
                        },
                        text(fonts, line.clone(), false, 15.0, PARCHMENT),
                    ));
                });
            }
            p.spawn((
                Node {
                    align_self: AlignSelf::Center,
                    margin: UiRect::top(px(4)),
                    ..default()
                },
                text(fonts, "нажмите, чтобы закрыть", false, 12.0, MUTED),
            ));
        });
    });
    overlay.observe(
        |_: On<PointerClick>,
         mut commands: Commands,
         overlays: Query<Entity, With<HelpOverlay>>| {
            for o in &overlays {
                commands.entity(o).despawn();
            }
        },
    );
}
