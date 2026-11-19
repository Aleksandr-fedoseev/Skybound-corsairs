//! Карта островов: этапы главы на островах вдоль пунктиров с картинки.
//!
//! Пройденные этапы можно перепроходить, следующий подсвечен, остальные закрыты. Тап по этапу
//! открывает карточку с врагами и кнопкой «В бой»; после боя игрок возвращается на карту.

use bevy::prelude::*;
use serde::Deserialize;

use crate::Screen;
use crate::battle_screen::BattleScreen;
use crate::menu::{
    BRASS, DARK_WOOD, GO, GO_EDGE, GOLD, MUTED, MenuData, PARCHMENT, Progress, Toast, Voyage, WOOD,
    bottom_nav, pressable, spawn_toast, text,
};
use crate::view::Fonts;

const CLEARED: Color = Color::srgb(0.30, 0.62, 0.38);
const LOCKED: Color = Color::srgb(0.36, 0.38, 0.43);
const NODE: f32 = 54.0;
const BOSS_NODE: f32 = 66.0;
const HOME_NODE: f32 = 64.0;
/// Следующий этап пульсирует: период (с) и размах (доля размера).
const PULSE_PERIOD: f32 = 1.4;
const PULSE: f32 = 0.08;

/// Карта: `data/map.json`. Координаты — центр точки на экране 540×960.
#[derive(Resource, Deserialize)]
pub struct MapData {
    pub background: String,
    pub chapter: String,
    pub home: Spot,
    pub stages: Vec<StageSpot>,
    pub locked: Vec<LockedSpot>,
}

#[derive(Deserialize)]
pub struct Spot {
    pub name: String,
    pub x: f32,
    pub y: f32,
}

#[derive(Deserialize)]
pub struct StageSpot {
    pub id: String,
    pub x: f32,
    pub y: f32,
}

/// Остров, который пока закрыт (следующая глава, ивент).
#[derive(Deserialize)]
pub struct LockedSpot {
    pub name: String,
    pub note: String,
    pub x: f32,
    pub y: f32,
}

impl MapData {
    pub fn bundled() -> Self {
        serde_json::from_str(include_str!("../../../data/map.json"))
            .expect("data/map.json корректен")
    }
}

/// Этап, открытый в карточке (индекс в `stages.json`).
#[derive(Resource, Default)]
struct Selected(Option<usize>);

#[derive(Component)]
struct Pulse;

#[derive(Component)]
struct Card;

#[derive(Component, Clone, Copy)]
enum CardText {
    Title,
    Enemies,
    Status,
}

pub fn plugin(app: &mut App) {
    app.insert_resource(MapData::bundled())
        .init_resource::<Selected>()
        .add_systems(OnEnter(Screen::Map), spawn_map)
        .add_systems(Update, (refresh_card, pulse).run_if(in_state(Screen::Map)));
}

fn spawn_map(
    mut commands: Commands,
    assets: Res<AssetServer>,
    fonts: Res<Fonts>,
    map: Res<MapData>,
    progress: Res<Progress>,
    screen: Res<BattleScreen>,
    mut selected: ResMut<Selected>,
) {
    let stages = &screen.data.stages;
    let next = progress.next_stage(stages.len());
    selected.0 = Some(next);

    commands
        .spawn((
            DespawnOnExit(Screen::Map),
            Node {
                width: percent(100),
                height: percent(100),
                ..default()
            },
            ImageNode {
                image_mode: NodeImageMode::Stretch,
                ..ImageNode::new(assets.load(map.background.clone()))
            },
        ))
        .with_children(|root| {
            header(
                root,
                &fonts,
                &map.chapter,
                progress.cleared.min(stages.len()),
                stages.len(),
            );
            home(root, &assets, &fonts, &map.home);
            for spot in &map.locked {
                locked(root, &fonts, spot);
            }
            for spot in &map.stages {
                let Some(index) = stages.iter().position(|s| s.id == spot.id) else {
                    continue;
                };
                stage_node(
                    root,
                    &fonts,
                    spot,
                    index,
                    &stages[index].name,
                    progress.cleared,
                    next,
                );
            }
            card(root, &fonts);
            bottom_nav(root, &assets, &fonts, Screen::Map);
            spawn_toast(root, &fonts);
        });
}

fn header(
    root: &mut ChildSpawnerCommands,
    fonts: &Fonts,
    chapter: &str,
    cleared: usize,
    total: usize,
) {
    root.spawn((
        Node {
            position_type: PositionType::Absolute,
            top: px(0),
            left: px(0),
            width: percent(100),
            height: px(56),
            padding: UiRect::horizontal(px(16)),
            justify_content: JustifyContent::SpaceBetween,
            align_items: AlignItems::Center,
            border: UiRect::bottom(px(2)),
            ..default()
        },
        BackgroundColor(WOOD),
        BorderColor::all(BRASS),
    ))
    .with_children(|bar| {
        bar.spawn(text(
            fonts,
            format!("Карта · {chapter}"),
            true,
            20.0,
            PARCHMENT,
        ));
        bar.spawn(text(
            fonts,
            format!("Пройдено {cleared} из {total}"),
            false,
            14.0,
            MUTED,
        ));
    });
}

/// Подпись под точкой на карте.
fn plate(parent: &mut ChildSpawnerCommands, fonts: &Fonts, title: &str, sub: &str, color: Color) {
    parent
        .spawn((
            Node {
                margin: UiRect::top(px(4)),
                padding: UiRect::axes(px(8), px(2)),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                border_radius: BorderRadius::all(px(8)),
                ..default()
            },
            BackgroundColor(WOOD),
            Pickable::IGNORE,
        ))
        .with_children(|p| {
            p.spawn(text(fonts, title, true, 12.0, color));
            if !sub.is_empty() {
                p.spawn(text(fonts, sub, false, 11.0, MUTED));
            }
        });
}

/// Колонка «круглая точка + подпись» с центром точки в (x, y).
fn spot_column(x: f32, y: f32, size: f32) -> Node {
    // Колонка шире точки, чтобы подпись не переносилась по словам.
    const LABEL_ROOM: f32 = 80.0;
    Node {
        position_type: PositionType::Absolute,
        left: px(x - (size + LABEL_ROOM) / 2.0),
        top: px(y - size / 2.0),
        width: px(size + LABEL_ROOM),
        flex_direction: FlexDirection::Column,
        align_items: AlignItems::Center,
        ..default()
    }
}

fn home(root: &mut ChildSpawnerCommands, assets: &AssetServer, fonts: &Fonts, spot: &Spot) {
    root.spawn((spot_column(spot.x, spot.y, HOME_NODE), Pickable::IGNORE))
        .with_children(|col| {
            let mut node = col.spawn((
                Node {
                    width: px(HOME_NODE),
                    height: px(HOME_NODE),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    border: UiRect::all(px(3)),
                    border_radius: BorderRadius::MAX,
                    ..default()
                },
                BackgroundColor(DARK_WOOD),
                BorderColor::all(GOLD),
            ));
            node.with_child((
                Node {
                    width: px(50),
                    height: px(50),
                    ..default()
                },
                ImageNode::new(assets.load("menu/ship_lvl1.png")),
                Pickable::IGNORE,
            ));
            pressable(&mut node);
            node.observe(|_: On<PointerClick>, mut next: ResMut<NextState<Screen>>| {
                next.set(Screen::Menu);
            });
            plate(col, fonts, &spot.name, "на корабль", GOLD);
        });
}

fn locked(root: &mut ChildSpawnerCommands, fonts: &Fonts, spot: &LockedSpot) {
    let message = format!("{} — {}, пока закрыто", spot.name, spot.note);
    let mut column = root.spawn(spot_column(spot.x, spot.y, NODE));
    column.with_children(|col| {
        plate(col, fonts, &spot.name, &spot.note, MUTED);
    });
    column.observe(move |_: On<PointerClick>, mut toast: ResMut<Toast>| {
        toast.show(&message);
    });
}

fn stage_node(
    root: &mut ChildSpawnerCommands,
    fonts: &Fonts,
    spot: &StageSpot,
    index: usize,
    name: &str,
    cleared: usize,
    next: usize,
) {
    let boss = spot.id.ends_with("boss");
    let size = if boss { BOSS_NODE } else { NODE };
    let open = index <= cleared;
    let (fill, edge) = if index < cleared {
        (CLEARED, Color::WHITE)
    } else if open {
        (GO, GO_EDGE)
    } else {
        (LOCKED, Color::srgb(0.6, 0.6, 0.62))
    };
    let label = if boss {
        "БОСС".to_string()
    } else {
        spot.id.clone()
    };
    let sub = if index < cleared {
        "пройден"
    } else if index == next && open {
        "новый"
    } else {
        ""
    };

    root.spawn((spot_column(spot.x, spot.y, size), Pickable::IGNORE))
        .with_children(|col| {
            let mut node = col.spawn((
                Node {
                    width: px(size),
                    height: px(size),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    border: UiRect::all(px(3)),
                    border_radius: BorderRadius::MAX,
                    ..default()
                },
                BackgroundColor(fill),
                BorderColor::all(edge),
            ));
            node.with_child(text(
                fonts,
                label,
                true,
                if boss { 14.0 } else { 16.0 },
                Color::WHITE,
            ));
            if index == next && open {
                node.insert(Pulse);
            }
            pressable(&mut node);
            node.observe(
                move |_: On<PointerClick>,
                      mut selected: ResMut<Selected>,
                      mut toast: ResMut<Toast>,
                      screen: Res<BattleScreen>| {
                    if open {
                        selected.0 = Some(index);
                    } else {
                        let before = &screen.data.stages[cleared.min(index)].name;
                        toast.show(&format!("Сначала пройдите «{before}»"));
                    }
                },
            );
            plate(col, fonts, name, sub, if open { PARCHMENT } else { MUTED });
        });
}

fn card(root: &mut ChildSpawnerCommands, fonts: &Fonts) {
    root.spawn((
        Card,
        Node {
            position_type: PositionType::Absolute,
            left: px(10),
            bottom: px(92),
            width: px(520),
            padding: UiRect::all(px(14)),
            column_gap: px(12),
            align_items: AlignItems::Center,
            border: UiRect::all(px(2)),
            border_radius: BorderRadius::all(px(14)),
            ..default()
        },
        BackgroundColor(Color::srgba(0.16, 0.11, 0.07, 0.96)),
        BorderColor::all(BRASS),
        Visibility::Hidden,
    ))
    .with_children(|card| {
        card.spawn(Node {
            flex_grow: 1.0,
            flex_shrink: 1.0,
            flex_direction: FlexDirection::Column,
            row_gap: px(3),
            ..default()
        })
        .with_children(|info| {
            info.spawn((CardText::Title, text(fonts, "", true, 18.0, PARCHMENT)));
            info.spawn((CardText::Enemies, text(fonts, "", false, 13.0, PARCHMENT)));
            info.spawn((CardText::Status, text(fonts, "", false, 12.0, MUTED)));
        });
        let mut go = card.spawn((
            Node {
                width: px(130),
                height: px(54),
                flex_shrink: 0.0,
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                border: UiRect::all(px(2)),
                border_radius: BorderRadius::all(px(12)),
                ..default()
            },
            BackgroundColor(GO),
            BorderColor::all(GO_EDGE),
        ));
        go.with_child(text(fonts, "В бой", true, 22.0, Color::WHITE));
        pressable(&mut go);
        go.observe(
            |_: On<PointerClick>,
             selected: Res<Selected>,
             mut voyage: ResMut<Voyage>,
             mut next: ResMut<NextState<Screen>>| {
                if let Some(stage) = selected.0 {
                    *voyage = Voyage {
                        stage: Some(stage),
                        from: Screen::Map,
                    };
                    next.set(Screen::Battle);
                }
            },
        );
    });
}

/// Враги этапа одной строкой: «Имперский морпех ×2 · Имперский стрелок ×1 · ур. 1».
fn enemies_line(screen: &BattleScreen, stage: usize) -> String {
    let data = &screen.data;
    let def = &data.stages[stage];
    let mut groups: Vec<(&str, usize)> = Vec::new();
    for enemy in &def.enemies {
        let name = data
            .enemy(&enemy.enemy)
            .map_or(enemy.enemy.as_str(), |e| e.name.as_str());
        match groups.iter_mut().find(|(n, _)| *n == name) {
            Some((_, count)) => *count += 1,
            None => groups.push((name, 1)),
        }
    }
    let levels = def.enemies.iter().map(|e| e.level);
    let (low, high) = (levels.clone().min().unwrap_or(1), levels.max().unwrap_or(1));
    let level = if low == high {
        format!("ур. {low}")
    } else {
        format!("ур. {low}–{high}")
    };
    let names: Vec<String> = groups.iter().map(|(n, c)| format!("{n} ×{c}")).collect();
    format!("{} · {level}", names.join(" · "))
}

fn refresh_card(
    selected: Res<Selected>,
    screen: Res<BattleScreen>,
    progress: Res<Progress>,
    menu: Res<MenuData>,
    mut cards: Query<&mut Visibility, With<Card>>,
    mut texts: Query<(&CardText, &mut Text)>,
) {
    for mut vis in &mut cards {
        vis.set_if_neq(if selected.0.is_some() {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        });
    }
    let Some(stage) = selected.0 else {
        return;
    };
    let def = &screen.data.stages[stage];
    let reward = &menu.victory_reward;
    for (kind, mut text) in &mut texts {
        let value = match kind {
            CardText::Title => {
                if def.id.ends_with("boss") {
                    format!("Босс главы · {}", def.name)
                } else {
                    format!("{} · {}", def.id, def.name)
                }
            }
            CardText::Enemies => enemies_line(&screen, stage),
            CardText::Status => {
                let state = if stage < progress.cleared {
                    "Пройден, можно перепройти"
                } else {
                    "Новый этап"
                };
                format!(
                    "{state} · награда +{} серебра, +{} досок",
                    reward.silver, reward.planks
                )
            }
        };
        if text.0 != value {
            text.0 = value;
        }
    }
}

fn pulse(time: Res<Time>, mut nodes: Query<&mut UiTransform, With<Pulse>>) {
    let t = time.elapsed_secs() / PULSE_PERIOD * std::f32::consts::TAU;
    for mut transform in &mut nodes {
        // Пока палец на кнопке, она сжата — пульс её не разжимает.
        if transform.scale.x > 0.95 {
            transform.scale = Vec2::splat(1.0 + PULSE * (0.5 + 0.5 * t.sin()));
        }
    }
}
