//! Главное меню — палуба «Ласточки» у родного острова.
//!
//! Корабль в центре, ресурсы сверху, отсеки и кнопка «В поход» снизу. Прогресс по этапам и
//! ресурсы пока живут только в памяти: сохранения и экономика на сервере будут позже.

use battle_core::Outcome;
use bevy::prelude::*;
use serde::Deserialize;

use crate::Screen;
use crate::battle_screen::BattleScreen;
use crate::view::{Fonts, ResultNote};

pub(crate) const WOOD: Color = Color::srgba(0.11, 0.075, 0.05, 0.90);
pub(crate) const DARK_WOOD: Color = Color::srgb(0.09, 0.06, 0.04);
pub(crate) const BRASS: Color = Color::srgb(0.79, 0.59, 0.25);
pub(crate) const PARCHMENT: Color = Color::srgb(0.965, 0.906, 0.784);
pub(crate) const MUTED: Color = Color::srgb(0.80, 0.72, 0.58);
pub(crate) const GOLD: Color = Color::srgb(0.95, 0.76, 0.31);
pub(crate) const GO: Color = Color::srgb(0.85, 0.51, 0.17);
pub(crate) const GO_EDGE: Color = Color::srgb(1.0, 0.89, 0.63);
const STORM: Color = Color::srgb(0.56, 0.82, 1.0);
const ETHER: Color = Color::srgb(0.18, 0.61, 0.58);
pub(crate) const RED_DOT: Color = Color::srgb(0.88, 0.29, 0.23);
const PILL: Color = Color::srgba(0.0, 0.0, 0.0, 0.35);
/// Сколько отсеков помещается в ряд под кораблём.
const COMPARTMENTS_SHOWN: usize = 6;
const TOAST_SECONDS: f32 = 2.2;
/// Корабль покачивается на ветру: период (с) и размах (px).
const BOB_PERIOD: f32 = 4.0;
const BOB_PX: f32 = 6.0;
const PRESSED_SCALE: f32 = 0.94;

/// Данные меню: `data/menu.json`.
#[derive(Resource, Deserialize, Clone)]
pub struct MenuData {
    pub ship_name: String,
    pub start: Start,
    pub ether_per_minute: f32,
    pub victory_reward: Reward,
    /// Дублоны за первое прохождение этапа.
    pub first_clear_doubloons: u32,
    /// Картинка корабля по уровню Корпуса: берётся последняя, у которой `from_hull` ≤ уровня.
    pub ship_art: Vec<ShipArt>,
    pub compartments: Vec<Compartment>,
}

#[derive(Deserialize, Clone)]
pub struct Start {
    pub captain_level: u32,
    pub hull_level: u32,
    pub silver: u32,
    pub hold_capacity: u32,
    pub planks: u32,
    pub ether: u32,
    pub doubloons: u32,
    /// Судовые журналы — опыт для уровней героев.
    pub journals: u32,
    /// Эфир, который Ветроловы накопили, пока игрока не было.
    pub ether_waiting: f32,
}

#[derive(Deserialize, Clone)]
pub struct Reward {
    pub silver: u32,
    pub planks: u32,
    pub journals: u32,
}

#[derive(Deserialize, Clone)]
pub struct ShipArt {
    pub from_hull: u32,
    pub image: String,
}

#[derive(Deserialize, Clone)]
pub struct Compartment {
    pub name: String,
    /// Подпись под иконкой, если полное имя не влезает.
    #[serde(default)]
    pub short: Option<String>,
    pub icon: String,
    /// Экран, который открывает тап по отсеку (`"tavern"`); без него — подсказка.
    #[serde(default)]
    pub opens: Option<String>,
    /// С какого уровня Корпуса открывается.
    pub hull: u32,
}

impl MenuData {
    pub fn bundled() -> Self {
        serde_json::from_str(include_str!("../../../data/menu.json"))
            .expect("data/menu.json корректен")
    }

    fn ship_image(&self, hull: u32) -> &str {
        self.ship_art
            .iter()
            .rfind(|art| art.from_hull <= hull)
            .or(self.ship_art.first())
            .map_or("", |art| art.image.as_str())
    }
}

/// Ресурсы капитана. Пока только на клиенте и без сохранения.
#[derive(Resource)]
pub struct Profile {
    pub captain_level: u32,
    pub hull_level: u32,
    pub silver: u32,
    pub hold_capacity: u32,
    pub planks: u32,
    pub ether: u32,
    pub doubloons: u32,
    pub journals: u32,
    /// Эфир, который накопили Ветроловы и который ещё не собран.
    pub ether_pending: f32,
}

impl Profile {
    fn new(start: &Start) -> Self {
        Self {
            captain_level: start.captain_level,
            hull_level: start.hull_level,
            silver: start.silver,
            hold_capacity: start.hold_capacity,
            planks: start.planks,
            ether: start.ether,
            doubloons: start.doubloons,
            journals: start.journals,
            ether_pending: start.ether_waiting,
        }
    }
}

/// Сколько этапов пройдено подряд с начала главы.
#[derive(Resource, Default)]
pub struct Progress {
    pub cleared: usize,
}

impl Progress {
    /// Следующий этап; когда всё пройдено — последний (босс), его можно перепроходить.
    pub fn next_stage(&self, stages: usize) -> usize {
        self.cleared.min(stages.saturating_sub(1))
    }

    fn all_cleared(&self, stages: usize) -> bool {
        self.cleared >= stages
    }
}

#[derive(Resource, Default)]
pub(crate) struct Toast {
    text: String,
    left: f32,
}

impl Toast {
    pub(crate) fn show(&mut self, text: &str) {
        self.text = text.to_string();
        self.left = TOAST_SECONDS;
    }
}

/// Меню надо собрать заново (например, сменился уровень Корпуса).
#[derive(Resource, Default)]
struct MenuRebuild(bool);

#[derive(Component)]
struct MenuRoot;

#[derive(Component, Clone, Copy)]
enum Value {
    Silver,
    Planks,
    Ether,
    Doubloons,
}

#[derive(Component)]
struct EtherBubble;

#[derive(Component)]
struct ToastNode;

/// Куда идём в бой и куда вернуться после него.
#[derive(Resource)]
pub struct Voyage {
    /// Этап, выбранный на карте; `None` — следующий непройденный.
    pub stage: Option<usize>,
    pub from: Screen,
}

impl Default for Voyage {
    fn default() -> Self {
        Self {
            stage: None,
            from: Screen::Menu,
        }
    }
}

#[derive(Component)]
struct ShipSprite;

pub fn plugin(app: &mut App) {
    let data = MenuData::bundled();
    app.insert_resource(Profile::new(&data.start))
        .insert_resource(data)
        .init_resource::<Progress>()
        .init_resource::<Toast>()
        .init_resource::<Voyage>()
        .init_resource::<MenuRebuild>()
        .add_systems(OnEnter(Screen::Menu), spawn_menu)
        .add_systems(Update, accumulate_ether)
        .add_systems(
            Update,
            (rebuild_menu, refresh_values, refresh_ether_bubble, bob_ship)
                .run_if(in_state(Screen::Menu)),
        )
        .add_systems(
            Update,
            refresh_toast.run_if(
                in_state(Screen::Menu)
                    .or_else(in_state(Screen::Map))
                    .or_else(in_state(Screen::Crew))
                    .or_else(in_state(Screen::Tavern)),
            ),
        )
        .add_systems(Update, count_victory.run_if(in_state(Screen::Battle)));
    #[cfg(feature = "dev")]
    app.add_systems(Update, dev_hull_key.run_if(in_state(Screen::Menu)));
}

fn spawn_menu(
    mut commands: Commands,
    assets: Res<AssetServer>,
    fonts: Res<Fonts>,
    data: Res<MenuData>,
    profile: Res<Profile>,
    progress: Res<Progress>,
    screen: Res<BattleScreen>,
) {
    build(
        &mut commands,
        &assets,
        &fonts,
        &data,
        &profile,
        &progress,
        &screen,
    );
}

#[allow(clippy::too_many_arguments)]
fn rebuild_menu(
    mut commands: Commands,
    mut rebuild: ResMut<MenuRebuild>,
    roots: Query<Entity, With<MenuRoot>>,
    assets: Res<AssetServer>,
    fonts: Res<Fonts>,
    data: Res<MenuData>,
    profile: Res<Profile>,
    progress: Res<Progress>,
    screen: Res<BattleScreen>,
) {
    if !rebuild.0 {
        return;
    }
    rebuild.0 = false;
    for root in &roots {
        commands.entity(root).despawn();
    }
    build(
        &mut commands,
        &assets,
        &fonts,
        &data,
        &profile,
        &progress,
        &screen,
    );
}

pub(crate) fn text(
    fonts: &Fonts,
    value: impl Into<String>,
    bold: bool,
    size: f32,
    color: Color,
) -> impl Bundle {
    (
        Text::new(value),
        fonts.text(bold, size),
        TextColor(color),
        TextShadow {
            offset: Vec2::new(1.0, 1.0),
            color: Color::srgba(0.0, 0.0, 0.0, 0.6),
        },
        Pickable::IGNORE,
    )
}

/// Кнопка чуть сжимается под пальцем.
pub(crate) fn pressable(button: &mut EntityCommands) {
    button
        .observe(|ev: On<PointerPress>, mut nodes: Query<&mut UiTransform>| {
            if let Ok(mut t) = nodes.get_mut(ev.event_target()) {
                t.scale = Vec2::splat(PRESSED_SCALE);
            }
        })
        .observe(
            |ev: On<PointerRelease>, mut nodes: Query<&mut UiTransform>| {
                if let Ok(mut t) = nodes.get_mut(ev.event_target()) {
                    t.scale = Vec2::ONE;
                }
            },
        )
        .observe(|ev: On<PointerOut>, mut nodes: Query<&mut UiTransform>| {
            if let Ok(mut t) = nodes.get_mut(ev.event_target()) {
                t.scale = Vec2::ONE;
            }
        });
}

/// Кнопка того, чего в игре ещё нет: показывает подсказку.
pub(crate) fn later(button: &mut EntityCommands, message: &'static str) {
    pressable(button);
    button.observe(move |_: On<PointerClick>, mut toast: ResMut<Toast>| {
        toast.show(message);
    });
}

pub(crate) fn stage_title(id: &str) -> String {
    let mut parts = id.splitn(2, '-');
    let chapter = parts.next().unwrap_or("1");
    match parts.next() {
        Some("boss") => format!("Глава {chapter} · босс главы"),
        _ => format!("Глава {chapter} · этап {id}"),
    }
}

#[allow(clippy::too_many_arguments)]
fn build(
    commands: &mut Commands,
    assets: &AssetServer,
    fonts: &Fonts,
    data: &MenuData,
    profile: &Profile,
    progress: &Progress,
    screen: &BattleScreen,
) {
    let stages = &screen.data.stages;
    let next = &stages[progress.next_stage(stages.len())];
    let done = progress.all_cleared(stages.len());

    commands
        .spawn((
            MenuRoot,
            DespawnOnExit(Screen::Menu),
            Node {
                width: percent(100),
                height: percent(100),
                ..default()
            },
            ImageNode {
                image_mode: NodeImageMode::Stretch,
                ..ImageNode::new(assets.load("menu/island.jpg"))
            },
        ))
        .with_children(|root| {
            top_bar(root, fonts, profile);
            side_buttons(root, fonts);

            // Имя корабля над ним.
            root.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    top: px(76),
                    left: px(140),
                    width: px(260),
                    flex_direction: FlexDirection::Column,
                    align_items: AlignItems::Center,
                    ..default()
                },
                Pickable::IGNORE,
            ))
            .with_children(|title| {
                title.spawn(text(
                    fonts,
                    format!("«{}»", data.ship_name),
                    true,
                    26.0,
                    Color::WHITE,
                ));
                title.spawn(text(
                    fonts,
                    format!("Корпус · ур. {}", profile.hull_level),
                    false,
                    13.0,
                    PARCHMENT,
                ));
            });

            // Корабль стоит на одной «ватерлинии», какого бы размера ни была картинка.
            root.spawn((
                ShipSprite,
                Node {
                    position_type: PositionType::Absolute,
                    left: px(80),
                    bottom: px(268),
                    width: px(380),
                    ..default()
                },
                ImageNode::new(assets.load(data.ship_image(profile.hull_level).to_string())),
                Pickable::IGNORE,
            ));

            // Ветроловы накопили эфир — тап собирает.
            let mut bubble = root.spawn((
                EtherBubble,
                Node {
                    position_type: PositionType::Absolute,
                    top: px(318),
                    left: px(300),
                    height: px(34),
                    padding: UiRect::horizontal(px(14)),
                    align_items: AlignItems::Center,
                    border: UiRect::all(px(2)),
                    border_radius: BorderRadius::all(px(17)),
                    ..default()
                },
                BackgroundColor(ETHER),
                BorderColor::all(Color::WHITE),
                Visibility::Hidden,
            ));
            bubble.with_child(text(fonts, "", true, 14.0, Color::WHITE));
            pressable(&mut bubble);
            bubble.observe(
                |_: On<PointerClick>, mut profile: ResMut<Profile>, mut toast: ResMut<Toast>| {
                    let got = profile.ether_pending.floor() as u32;
                    if got > 0 {
                        profile.ether += got;
                        profile.ether_pending -= got as f32;
                        toast.show(&format!("Ветроловы: +{got} эфира"));
                    }
                },
            );

            compartments(root, assets, fonts, data, profile);
            voyage(root, fonts, next, done);
            bottom_nav(root, assets, fonts, Screen::Menu);
            spawn_toast(root, fonts);
        });
}

/// Подсказка поверх всего экрана.
pub(crate) fn spawn_toast(root: &mut ChildSpawnerCommands, fonts: &Fonts) {
    root.spawn((
        ToastNode,
        Node {
            position_type: PositionType::Absolute,
            top: px(250),
            left: px(40),
            width: px(460),
            padding: UiRect::all(px(12)),
            justify_content: JustifyContent::Center,
            border_radius: BorderRadius::all(px(10)),
            ..default()
        },
        BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.78)),
        Visibility::Hidden,
        GlobalZIndex(20),
        Pickable::IGNORE,
    ))
    .with_child((
        text(fonts, "", false, 15.0, Color::WHITE),
        TextLayout::justify(Justify::Center),
    ));
}

fn top_bar(root: &mut ChildSpawnerCommands, fonts: &Fonts, profile: &Profile) {
    root.spawn((
        Node {
            position_type: PositionType::Absolute,
            top: px(0),
            left: px(0),
            width: percent(100),
            height: px(64),
            padding: UiRect::axes(px(10), px(6)),
            align_items: AlignItems::Center,
            column_gap: px(10),
            border: UiRect::bottom(px(2)),
            ..default()
        },
        BackgroundColor(WOOD),
        BorderColor::all(BRASS),
    ))
    .with_children(|bar| {
        let mut avatar = bar.spawn((
            Node {
                width: px(50),
                height: px(50),
                flex_shrink: 0.0,
                flex_direction: FlexDirection::Column,
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                border: UiRect::all(px(2)),
                border_radius: BorderRadius::MAX,
                ..default()
            },
            BackgroundColor(DARK_WOOD),
            BorderColor::all(BRASS),
        ));
        avatar.with_children(|a| {
            a.spawn(text(fonts, "ур.", false, 11.0, MUTED));
            a.spawn(text(
                fonts,
                profile.captain_level.to_string(),
                true,
                18.0,
                PARCHMENT,
            ));
        });
        later(&mut avatar, "Профиль капитана появится позже");

        bar.spawn(Node {
            flex_grow: 1.0,
            flex_direction: FlexDirection::Column,
            row_gap: px(4),
            ..default()
        })
        .with_children(|grid| {
            for row in [
                [
                    (Value::Silver, Color::srgb(0.84, 0.87, 0.90), "серебро"),
                    (Value::Planks, Color::srgb(0.69, 0.48, 0.29), "доски"),
                ],
                [
                    (Value::Ether, Color::srgb(0.37, 0.83, 0.79), "эфир"),
                    (Value::Doubloons, GOLD, ""),
                ],
            ] {
                grid.spawn(Node {
                    column_gap: px(6),
                    ..default()
                })
                .with_children(|line| {
                    for (value, color, name) in row {
                        resource_pill(line, fonts, value, color, name);
                    }
                });
            }
        });
    });
}

fn resource_pill(
    line: &mut ChildSpawnerCommands,
    fonts: &Fonts,
    value: Value,
    color: Color,
    name: &str,
) {
    line.spawn((
        Node {
            flex_grow: 1.0,
            flex_basis: px(0),
            height: px(24),
            padding: UiRect::left(px(8)),
            align_items: AlignItems::Center,
            column_gap: px(6),
            border_radius: BorderRadius::all(px(12)),
            ..default()
        },
        BackgroundColor(PILL),
    ))
    .with_children(|pill| {
        pill.spawn((
            Node {
                width: px(12),
                height: px(12),
                border_radius: BorderRadius::MAX,
                ..default()
            },
            BackgroundColor(color),
        ));
        pill.spawn((value, text(fonts, "", true, 13.0, PARCHMENT)));
        pill.spawn((
            Node {
                flex_grow: 1.0,
                ..default()
            },
            text(fonts, name, false, 12.0, MUTED),
        ));
        if matches!(value, Value::Doubloons) {
            let mut plus = pill.spawn((
                Node {
                    width: px(22),
                    height: px(22),
                    margin: UiRect::right(px(1)),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    border_radius: BorderRadius::all(px(11)),
                    ..default()
                },
                BackgroundColor(Color::srgb(0.30, 0.69, 0.42)),
            ));
            plus.with_child(text(fonts, "+", true, 16.0, Color::WHITE));
            later(&mut plus, "Магазин дублонов появится позже");
        }
    });
}

fn side_buttons(root: &mut ChildSpawnerCommands, fonts: &Fonts) {
    let column = |left: bool| Node {
        position_type: PositionType::Absolute,
        top: px(76),
        left: if left { px(10) } else { auto() },
        right: if left { auto() } else { px(10) },
        flex_direction: FlexDirection::Column,
        row_gap: px(10),
        ..default()
    };
    let items: [(&str, &str, Color, bool, &'static str); 4] = [
        ("Буря", "ивент", STORM, false, "Ивенты появятся позже"),
        (
            "Пропуск",
            "сезон",
            BRASS,
            false,
            "Боевой пропуск появится позже",
        ),
        (
            "Контракты",
            "задания",
            BRASS,
            true,
            "Доска контрактов появится позже",
        ),
        ("Почта", "", BRASS, false, "Почта появится позже"),
    ];
    for (left, chunk) in [(true, &items[..2]), (false, &items[2..])] {
        root.spawn(column(left)).with_children(|col| {
            for &(title, sub, edge, dot, message) in chunk {
                let mut button = col.spawn((
                    Node {
                        width: px(76),
                        height: px(64),
                        flex_direction: FlexDirection::Column,
                        justify_content: JustifyContent::Center,
                        align_items: AlignItems::Center,
                        border: UiRect::all(px(2)),
                        border_radius: BorderRadius::all(px(12)),
                        ..default()
                    },
                    BackgroundColor(WOOD),
                    BorderColor::all(edge),
                ));
                button.with_children(|b| {
                    b.spawn(text(fonts, title, true, 14.0, PARCHMENT));
                    if !sub.is_empty() {
                        b.spawn(text(fonts, sub, false, 12.0, MUTED));
                    }
                    if dot {
                        b.spawn((
                            Node {
                                position_type: PositionType::Absolute,
                                top: px(-5),
                                right: px(-5),
                                width: px(14),
                                height: px(14),
                                border_radius: BorderRadius::MAX,
                                ..default()
                            },
                            BackgroundColor(RED_DOT),
                            Pickable::IGNORE,
                        ));
                    }
                });
                later(&mut button, message);
            }
        });
    }
}

fn compartments(
    root: &mut ChildSpawnerCommands,
    assets: &AssetServer,
    fonts: &Fonts,
    data: &MenuData,
    profile: &Profile,
) {
    // Сначала открытые отсеки, потом ближайшие закрытые.
    let mut shown: Vec<&Compartment> = data.compartments.iter().collect();
    shown.sort_by_key(|c| (c.hull > profile.hull_level, c.hull));
    shown.truncate(COMPARTMENTS_SHOWN);

    root.spawn((
        Node {
            position_type: PositionType::Absolute,
            top: px(694),
            left: px(10),
            width: px(520),
            height: px(96),
            padding: UiRect::axes(px(6), px(6)),
            justify_content: JustifyContent::SpaceBetween,
            border: UiRect::all(px(1)),
            border_radius: BorderRadius::all(px(14)),
            ..default()
        },
        BackgroundColor(WOOD),
        BorderColor::all(BRASS),
    ))
    .with_children(|row| {
        for compartment in shown {
            let open = compartment.hull <= profile.hull_level;
            let mut tile = row.spawn(Node {
                width: px(82),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                ..default()
            });
            tile.with_children(|t| {
                t.spawn((
                    Node {
                        width: px(70),
                        height: px(54),
                        ..default()
                    },
                    ImageNode {
                        color: if open {
                            Color::WHITE
                        } else {
                            Color::srgba(0.35, 0.35, 0.38, 0.9)
                        },
                        ..ImageNode::new(assets.load(compartment.icon.clone()))
                    },
                    Pickable::IGNORE,
                ));
                t.spawn((
                    text(
                        fonts,
                        compartment
                            .short
                            .clone()
                            .unwrap_or_else(|| compartment.name.clone()),
                        false,
                        12.0,
                        if open { PARCHMENT } else { MUTED },
                    ),
                    TextLayout::justify(Justify::Center),
                ));
                if !open {
                    t.spawn((
                        Node {
                            position_type: PositionType::Absolute,
                            top: px(16),
                            ..default()
                        },
                        text(
                            fonts,
                            format!("Корпус {}", compartment.hull),
                            true,
                            12.0,
                            GOLD,
                        ),
                    ));
                }
            });
            let message = if open {
                format!("{} — экран отсека появится позже", compartment.name)
            } else {
                format!(
                    "{} откроется на Корпусе {}",
                    compartment.name, compartment.hull
                )
            };
            let opens = match compartment.opens.as_deref() {
                Some("tavern") if open => Some(Screen::Tavern),
                _ => None,
            };
            pressable(&mut tile);
            tile.observe(
                move |_: On<PointerClick>,
                      mut toast: ResMut<Toast>,
                      mut next: ResMut<NextState<Screen>>| {
                    match opens {
                        Some(screen) => next.set(screen),
                        None => toast.show(&message),
                    }
                },
            );
        }
    });
}

fn voyage(root: &mut ChildSpawnerCommands, fonts: &Fonts, next: &game_data::StageDef, done: bool) {
    root.spawn((
        Node {
            position_type: PositionType::Absolute,
            top: px(798),
            left: px(10),
            width: px(520),
            height: px(72),
            padding: UiRect::new(px(16), px(9), px(9), px(9)),
            align_items: AlignItems::Center,
            column_gap: px(12),
            border: UiRect::all(px(2)),
            border_radius: BorderRadius::all(px(14)),
            ..default()
        },
        BackgroundColor(Color::srgba(0.16, 0.11, 0.07, 0.95)),
        BorderColor::all(BRASS),
    ))
    .with_children(|panel| {
        panel
            .spawn(Node {
                flex_grow: 1.0,
                flex_direction: FlexDirection::Column,
                ..default()
            })
            .with_children(|info| {
                let caption = if done {
                    "Глава 1 пройдена · перепройти босса".to_string()
                } else {
                    stage_title(&next.id)
                };
                info.spawn(text(fonts, caption, false, 12.0, MUTED));
                info.spawn(text(fonts, next.name.clone(), true, 20.0, PARCHMENT));
            });
        let mut go = panel.spawn((
            Node {
                width: px(150),
                height: px(54),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                border: UiRect::all(px(2)),
                border_radius: BorderRadius::all(px(12)),
                ..default()
            },
            BackgroundColor(GO),
            BorderColor::all(GO_EDGE),
        ));
        go.with_child(text(fonts, "В поход", true, 22.0, Color::WHITE));
        pressable(&mut go);
        go.observe(
            |_: On<PointerClick>,
             mut voyage: ResMut<Voyage>,
             mut next: ResMut<NextState<Screen>>| {
                *voyage = Voyage::default();
                next.set(Screen::Battle);
            },
        );
    });
}

/// Нижняя панель вкладок: общая для корабля и карты.
pub(crate) fn bottom_nav(
    root: &mut ChildSpawnerCommands,
    assets: &AssetServer,
    fonts: &Fonts,
    active: Screen,
) {
    // Вкладка ведёт на экран или пока только показывает подсказку.
    let tabs: [(&str, &str, Result<Screen, &'static str>); 5] = [
        ("Корабль", "menu/ship_lvl1.png", Ok(Screen::Menu)),
        ("Экипаж", "menu/module_training_deck.png", Ok(Screen::Crew)),
        ("Карта", "menu/module_navigation.png", Ok(Screen::Map)),
        ("Таверна", "menu/module_tavern.png", Ok(Screen::Tavern)),
        (
            "Активности",
            "menu/module_lookout.png",
            Err("Активности появятся позже"),
        ),
    ];
    root.spawn((
        Node {
            position_type: PositionType::Absolute,
            bottom: px(0),
            left: px(0),
            width: percent(100),
            height: px(82),
            padding: UiRect::vertical(px(6)),
            border: UiRect::top(px(2)),
            ..default()
        },
        BackgroundColor(DARK_WOOD),
        BorderColor::all(BRASS),
        GlobalZIndex(5),
    ))
    .with_children(|nav| {
        for (name, icon, target) in tabs {
            let here = target == Ok(active);
            let mut tab = nav.spawn(Node {
                flex_grow: 1.0,
                flex_basis: px(0),
                flex_direction: FlexDirection::Column,
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                row_gap: px(2),
                ..default()
            });
            tab.with_children(|t| {
                t.spawn((
                    Node {
                        width: px(52),
                        height: px(44),
                        ..default()
                    },
                    ImageNode::new(assets.load(icon)),
                    Pickable::IGNORE,
                ));
                t.spawn(text(
                    fonts,
                    name,
                    here,
                    12.0,
                    if here { GOLD } else { PARCHMENT },
                ));
            });
            match target {
                Ok(screen) if !here => {
                    pressable(&mut tab);
                    tab.observe(
                        move |_: On<PointerClick>, mut next: ResMut<NextState<Screen>>| {
                            next.set(screen);
                        },
                    );
                }
                Ok(_) => {}
                Err(message) => later(&mut tab, message),
            }
        }
    });
}

/// Ветроловы копят эфир всегда, даже во время боя.
fn accumulate_ether(time: Res<Time>, data: Res<MenuData>, mut profile: ResMut<Profile>) {
    profile.ether_pending += time.delta_secs() * data.ether_per_minute / 60.0;
}

fn refresh_values(profile: Res<Profile>, mut texts: Query<(&Value, &mut Text)>) {
    for (value, mut text) in &mut texts {
        let shown = match value {
            Value::Silver => format!("{} / {}", profile.silver, profile.hold_capacity),
            Value::Planks => profile.planks.to_string(),
            Value::Ether => profile.ether.to_string(),
            Value::Doubloons => profile.doubloons.to_string(),
        };
        if text.0 != shown {
            text.0 = shown;
        }
    }
}

fn refresh_ether_bubble(
    profile: Res<Profile>,
    mut bubbles: Query<(&mut Visibility, &Children), With<EtherBubble>>,
    mut texts: Query<&mut Text>,
) {
    let ready = profile.ether_pending.floor() as u32;
    for (mut vis, children) in &mut bubbles {
        vis.set_if_neq(if ready > 0 {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        });
        for child in children {
            if let Ok(mut text) = texts.get_mut(*child) {
                text.0 = format!("+{ready} эфира");
            }
        }
    }
}

fn refresh_toast(
    time: Res<Time>,
    mut toast: ResMut<Toast>,
    mut nodes: Query<(&mut Visibility, &Children), With<ToastNode>>,
    mut texts: Query<&mut Text>,
) {
    toast.left = (toast.left - time.delta_secs()).max(0.0);
    for (mut vis, children) in &mut nodes {
        vis.set_if_neq(if toast.left > 0.0 {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        });
        for child in children {
            if let Ok(mut text) = texts.get_mut(*child)
                && text.0 != toast.text
            {
                text.0 = toast.text.clone();
            }
        }
    }
}

fn bob_ship(time: Res<Time>, mut ships: Query<&mut UiTransform, With<ShipSprite>>) {
    let t = time.elapsed_secs() / BOB_PERIOD * std::f32::consts::TAU;
    for mut transform in &mut ships {
        transform.translation = Val2::px(0.0, BOB_PX * t.sin());
    }
}

/// Победа открывает следующий этап и приносит добычу.
fn count_victory(
    screen: Res<BattleScreen>,
    data: Res<MenuData>,
    mut progress: ResMut<Progress>,
    mut profile: ResMut<Profile>,
    mut counted: Local<Option<(usize, u64)>>,
    mut notes: Query<&mut Text, With<ResultNote>>,
) {
    let Some(outcome) = screen.shown_outcome else {
        return;
    };
    let battle = (screen.stage, screen.seed);
    if *counted == Some(battle) {
        return;
    }
    *counted = Some(battle);
    let note = match outcome {
        Outcome::Victory => {
            let reward = &data.victory_reward;
            profile.silver = (profile.silver + reward.silver).min(profile.hold_capacity);
            profile.planks += reward.planks;
            profile.journals += reward.journals;
            let first = screen.stage == progress.cleared;
            if first {
                progress.cleared += 1;
                profile.doubloons += data.first_clear_doubloons;
            }
            let stages = &screen.data.stages;
            let opened = match stages.get(progress.cleared) {
                Some(stage) if first => format!("\nОткрыт этап «{}»", stage.name),
                None if first => "\nГлава 1 пройдена!".to_string(),
                _ => String::new(),
            };
            let doubloons = if first {
                format!(" · +{} дублонов", data.first_clear_doubloons)
            } else {
                String::new()
            };
            format!(
                "+{} серебра · +{} досок · +{} журналов{doubloons}{opened}",
                reward.silver, reward.planks, reward.journals
            )
        }
        Outcome::Defeat => "Усильте отряд или смените тактику".to_string(),
    };
    for mut text in &mut notes {
        text.0 = note.clone();
    }
}

/// H — следующий уровень Корпуса (1–7): другой корабль и открытые отсеки.
#[cfg(feature = "dev")]
fn dev_hull_key(
    keys: Res<ButtonInput<KeyCode>>,
    mut profile: ResMut<Profile>,
    mut rebuild: ResMut<MenuRebuild>,
) {
    if keys.just_pressed(KeyCode::KeyH) {
        profile.hull_level = profile.hull_level % 7 + 1;
        rebuild.0 = true;
    }
}
