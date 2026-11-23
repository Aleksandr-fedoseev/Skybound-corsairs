//! Карта островов: этапы главы на островах вдоль пунктиров с картинки.
//!
//! Пройденные этапы можно перепроходить, следующий подсвечен, остальные закрыты. Тап по этапу
//! открывает карточку с врагами и кнопкой «В бой»; после боя игрок возвращается на карту.

use bevy::prelude::*;
use serde::Deserialize;

use crate::Screen;
use crate::battle_screen::BattleScreen;
use crate::convoys::{ConvoyData, Convoys};
use crate::crew::Roster;
use crate::menu::{
    BRASS, DARK_WOOD, GO, GO_EDGE, GOLD, MUTED, MenuData, PARCHMENT, Progress, Toast, Voyage,
    bottom_nav, pressable, spawn_toast, text,
};
use crate::ship::clock;
use crate::story::VoyageRequest;
use crate::style::{Look, Skin};
use crate::view::Fonts;

const CLEARED: Color = Color::srgb(0.30, 0.62, 0.38);
const CONVOY: Color = Color::srgb(0.20, 0.45, 0.62);
const READY: Color = Color::srgb(0.55, 0.88, 0.55);
const WEAK: Color = Color::srgb(1.0, 0.55, 0.45);
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

/// В карточке — конвой (а не этап).
#[derive(Resource, Default)]
struct ConvoyPick(bool);

/// Подпись конвоя на карте: сколько судов.
#[derive(Component)]
struct ConvoyLabel;

#[derive(Component)]
struct Pulse;

#[derive(Component)]
struct Card;

#[derive(Component, Clone, Copy)]
enum CardText {
    Title,
    Enemies,
    Level,
    Status,
}

pub fn plugin(app: &mut App) {
    app.insert_resource(MapData::bundled())
        .init_resource::<Selected>()
        .init_resource::<ConvoyPick>()
        .add_systems(OnEnter(Screen::Map), spawn_map)
        .add_systems(
            Update,
            (refresh_card, refresh_convoy_label, pulse).run_if(in_state(Screen::Map)),
        );
}

#[allow(clippy::too_many_arguments)]
fn spawn_map(
    mut commands: Commands,
    assets: Res<AssetServer>,
    fonts: Res<Fonts>,
    map: Res<MapData>,
    progress: Res<Progress>,
    screen: Res<BattleScreen>,
    convoy: Res<ConvoyData>,
    mut selected: ResMut<Selected>,
    mut pick: ResMut<ConvoyPick>,
    profile: Res<crate::menu::Profile>,
) {
    let stages = &screen.data.stages;
    let next = progress.next_stage(stages.len());
    selected.0 = Some(next);
    pick.0 = false;

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
            crate::style::top_bar(
                root,
                &assets,
                &fonts,
                &[
                    ("doubloons", profile.doubloons),
                    ("silver", profile.silver),
                    ("planks", profile.planks),
                ],
            );
            crate::style::title(
                root,
                &fonts,
                &format!(
                    "{} · {} из {}",
                    map.chapter,
                    progress.cleared.min(stages.len()),
                    stages.len()
                ),
                56.0,
            );
            home(root, &assets, &fonts, &map.home);
            for spot in &map.locked {
                locked(root, &fonts, spot);
            }
            convoy_spot(root, &assets, &fonts, &convoy.spot);
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

/// Подпись под точкой на карте.
fn plate(parent: &mut ChildSpawnerCommands, fonts: &Fonts, title: &str, sub: &str, color: Color) {
    parent
        .spawn((
            Node {
                margin: UiRect::top(px(4)),
                padding: UiRect::axes(px(10), px(3)),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                border: UiRect::all(px(1)),
                border_radius: BorderRadius::all(px(8)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.07, 0.05, 0.03, 0.92)),
            BorderColor::all(BRASS),
            Pickable::IGNORE,
        ))
        .with_children(|p| {
            p.spawn(crate::style::heading(fonts, title, 14.0, color));
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
                    width: px(54),
                    height: px(54),
                    ..default()
                },
                ImageNode::new(assets.load("ui/nav_ship.png")),
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
        (CLEARED, BRASS)
    } else if open {
        (GO, GOLD)
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
                Outline::new(px(2), px(0), Color::srgb(0.30, 0.20, 0.08)),
            ));
            node.with_child(crate::style::heading(
                fonts,
                label,
                if boss { 16.0 } else { 18.0 },
                Color::WHITE,
            ));
            if index == next && open {
                node.insert(Pulse);
            }
            pressable(&mut node);
            node.observe(
                move |_: On<PointerClick>,
                      mut selected: ResMut<Selected>,
                      mut pick: ResMut<ConvoyPick>,
                      mut toast: ResMut<Toast>,
                      screen: Res<BattleScreen>| {
                    if open {
                        selected.0 = Some(index);
                        pick.0 = false;
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
            padding: UiRect::axes(px(18), px(14)),
            column_gap: px(12),
            align_items: AlignItems::Center,
            ..default()
        },
        Skin(Look::Tile),
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
            info.spawn((
                CardText::Title,
                crate::style::heading(fonts, "", 22.0, Color::srgb(1.0, 0.93, 0.78)),
            ));
            info.spawn((CardText::Enemies, text(fonts, "", false, 13.0, PARCHMENT)));
            info.spawn((CardText::Level, text(fonts, "", true, 13.0, PARCHMENT)));
            info.spawn((CardText::Status, text(fonts, "", false, 12.0, MUTED)));
        });
        let mut go = card.spawn((
            Node {
                width: px(130),
                height: px(58),
                flex_shrink: 0.0,
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            Skin(Look::Go),
        ));
        go.with_child(crate::style::heading(fonts, "В бой", 24.0, Color::WHITE));
        pressable(&mut go);
        go.observe(
            |_: On<PointerClick>,
             selected: Res<Selected>,
             pick: Res<ConvoyPick>,
             convoy: (Res<ConvoyData>, Res<Convoys>, Res<Progress>),
             mut toast: ResMut<Toast>,
             mut voyage: ResMut<Voyage>,
             mut requests: MessageWriter<VoyageRequest>| {
                let (data, convoys, progress) = convoy;
                if pick.0 {
                    if convoys.ready == 0 {
                        toast.show(&format!(
                            "Конвоев нет — следующий через {}",
                            clock(convoys.next_in(&data))
                        ));
                        return;
                    }
                    *voyage = Voyage {
                        stage: None,
                        custom: Some(data.fight(&progress)),
                        from: Screen::Map,
                    };
                    requests.write(VoyageRequest);
                } else if let Some(stage) = selected.0 {
                    *voyage = Voyage {
                        stage: Some(stage),
                        custom: None,
                        from: Screen::Map,
                    };
                    requests.write(VoyageRequest);
                }
            },
        );
    });
}

/// Враги этапа одной строкой: «Имперский морпех ×2 · Имперский стрелок ×1».
fn enemies_line(screen: &BattleScreen, stage: usize) -> String {
    let ids: Vec<&str> = screen.data.stages[stage]
        .enemies
        .iter()
        .map(|e| e.enemy.as_str())
        .collect();
    enemies_text(screen, &ids)
}

fn enemies_text(screen: &BattleScreen, ids: &[&str]) -> String {
    let data = &screen.data;
    let mut groups: Vec<(&str, usize)> = Vec::new();
    for id in ids {
        let name = data.enemy(id).map_or(*id, |e| e.name.as_str());
        match groups.iter_mut().find(|(n, _)| *n == name) {
            Some((_, count)) => *count += 1,
            None => groups.push((name, 1)),
        }
    }
    let names: Vec<String> = groups.iter().map(|(n, c)| format!("{n} ×{c}")).collect();
    names.join(" · ")
}

#[allow(clippy::too_many_arguments)]
fn refresh_card(
    selected: Res<Selected>,
    pick: Res<ConvoyPick>,
    convoy: (Res<ConvoyData>, Res<Convoys>),
    events: Res<crate::events::EventData>,
    screen: Res<BattleScreen>,
    progress: Res<Progress>,
    menu: Res<MenuData>,
    roster: Res<Roster>,
    mut cards: Query<&mut Visibility, With<Card>>,
    mut texts: Query<(&CardText, &mut Text, &mut TextColor)>,
) {
    for mut vis in &mut cards {
        vis.set_if_neq(if selected.0.is_some() || pick.0 {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        });
    }
    if pick.0 {
        let (data, convoys) = convoy;
        let tier = data.tier(&progress);
        let ids: Vec<&str> = tier.enemies.iter().map(|(id, _)| id.as_str()).collect();
        let level = tier.enemies.iter().map(|(_, l)| *l).max().unwrap_or(1);
        for (kind, mut text, mut color) in &mut texts {
            let value = match kind {
                CardText::Title => format!("Конвой · {}", tier.name),
                CardText::Enemies => format!("{} · ур. {level}", enemies_text(&screen, &ids)),
                CardText::Level => {
                    color.0 = if convoys.ready > 0 { READY } else { WEAK };
                    if convoys.ready >= data.max_ready {
                        format!(
                            "Судов: {} из {} — очередь полна",
                            convoys.ready, data.max_ready
                        )
                    } else {
                        format!(
                            "Судов: {} из {} · следующее через {}",
                            convoys.ready,
                            data.max_ready,
                            clock(convoys.next_in(&data))
                        )
                    }
                }
                CardText::Status => {
                    let k = events.convoy_multiplier();
                    let wind = if k > 1 {
                        format!(" · попутный ветер ×{k}")
                    } else {
                        String::new()
                    };
                    format!(
                        "Добыча: +{} досок · +{} серебра{wind}",
                        tier.planks * k,
                        tier.silver * k
                    )
                }
            };
            if text.0 != value {
                text.0 = value;
            }
        }
        return;
    }
    let Some(stage) = selected.0 else {
        return;
    };
    let def = &screen.data.stages[stage];
    let reward = &menu.victory_reward;
    // Средний уровень отряда против рекомендуемого: ниже — этап, скорее всего, не пройти.
    let team = roster.team_levels();
    let team_level = team.iter().map(|(_, l)| l).sum::<u32>() / team.len().max(1) as u32;
    let ready = team_level >= def.recommended_level;
    for (kind, mut text, mut color) in &mut texts {
        let value = match kind {
            CardText::Title => {
                if def.id.ends_with("boss") {
                    format!("Босс главы · {}", def.name)
                } else {
                    format!("{} · {}", def.id, def.name)
                }
            }
            CardText::Enemies => enemies_line(&screen, stage),
            CardText::Level => {
                color.0 = if ready { READY } else { WEAK };
                format!(
                    "Рекомендуемый уровень отряда: {} · у вас {team_level}",
                    def.recommended_level
                )
            }
            CardText::Status => {
                let state = if stage < progress.cleared {
                    "Пройден"
                } else {
                    "Новый"
                };
                format!(
                    "{state} · +{} серебра · +{} досок · +{} журналов",
                    reward.silver, reward.planks, reward.journals
                )
            }
        };
        if text.0 != value {
            text.0 = value;
        }
    }
}

/// Точка конвоя на карте: шлюпка и подпись с числом судов.
fn convoy_spot(
    root: &mut ChildSpawnerCommands,
    assets: &AssetServer,
    fonts: &Fonts,
    spot: &crate::convoys::Spot,
) {
    root.spawn((spot_column(spot.x, spot.y, NODE), Pickable::IGNORE))
        .with_children(|col| {
            let mut node = col.spawn((
                Node {
                    width: px(NODE),
                    height: px(NODE),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    border: UiRect::all(px(3)),
                    border_radius: BorderRadius::MAX,
                    ..default()
                },
                BackgroundColor(CONVOY),
                BorderColor::all(GO_EDGE),
            ));
            node.with_child((
                Node {
                    width: px(44),
                    height: px(36),
                    ..default()
                },
                ImageNode::new(assets.load("menu/module_boats.png")),
                Pickable::IGNORE,
            ));
            pressable(&mut node);
            node.observe(
                |_: On<PointerClick>,
                 mut selected: ResMut<Selected>,
                 mut pick: ResMut<ConvoyPick>| {
                    selected.0 = None;
                    pick.0 = true;
                },
            );
            col.spawn((
                Node {
                    margin: UiRect::top(px(4)),
                    padding: UiRect::axes(px(10), px(3)),
                    flex_direction: FlexDirection::Column,
                    align_items: AlignItems::Center,
                    border: UiRect::all(px(1)),
                    border_radius: BorderRadius::all(px(8)),
                    ..default()
                },
                BackgroundColor(Color::srgba(0.07, 0.05, 0.03, 0.92)),
                BorderColor::all(BRASS),
                Pickable::IGNORE,
            ))
            .with_children(|p| {
                p.spawn(crate::style::heading(
                    fonts,
                    spot.name.clone(),
                    14.0,
                    PARCHMENT,
                ));
                p.spawn((ConvoyLabel, text(fonts, "", false, 11.0, GOLD)));
            });
        });
}

fn refresh_convoy_label(convoys: Res<Convoys>, mut labels: Query<&mut Text, With<ConvoyLabel>>) {
    for mut text in &mut labels {
        let value = format!("судов: {}", convoys.ready);
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
