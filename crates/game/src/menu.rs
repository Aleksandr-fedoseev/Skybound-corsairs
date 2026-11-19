//! Главное меню — палуба «Ласточки» у родного острова.
//!
//! Корабль в центре, ресурсы сверху, отсеки и кнопка «В поход» снизу. Прогресс по этапам и
//! ресурсы пока живут только в памяти: сохранения и экономика на сервере будут позже.

use battle_core::Outcome;
use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use bevy::ecs::system::SystemParam;

use crate::Screen;
use crate::abyss::{Abyss, AbyssData};
use crate::audio::Sfx;
use crate::battle_screen::{BattleScreen, CustomFight, FightKind};
use crate::contracts::{BoardOpen, ContractData, Contracts, Deed};
use crate::convoys::Convoys;
use crate::ship::{Compartment, OpenCompartment, Ship, ShipData};
use crate::story::{StoryQueue, VoyageRequest};
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
const TOAST_LONG_SECONDS: f32 = 6.0;
/// Корабль покачивается на ветру: период (с) и размах (px).
const BOB_PERIOD: f32 = 4.0;
const BOB_PX: f32 = 6.0;
const PRESSED_SCALE: f32 = 0.94;

/// Данные меню: `data/menu.json`.
#[derive(Resource, Deserialize, Clone)]
pub struct MenuData {
    pub ship_name: String,
    pub start: Start,
    pub victory_reward: Reward,
    /// Дублоны за первое прохождение этапа.
    pub first_clear_doubloons: u32,
}

#[derive(Deserialize, Clone)]
pub struct Start {
    pub captain_level: u32,
    pub silver: u32,
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

impl MenuData {
    pub fn bundled() -> Self {
        serde_json::from_str(include_str!("../../../data/menu.json"))
            .expect("data/menu.json корректен")
    }
}

/// Ресурсы капитана. Пока на клиенте; сохраняются в файл (`save.rs`).
#[derive(Resource, Clone, Serialize, Deserialize)]
pub struct Profile {
    pub captain_level: u32,
    pub silver: u32,
    pub planks: u32,
    pub ether: u32,
    pub doubloons: u32,
    pub journals: u32,
    /// Лоции — книги умений (из Бездны, контрактов, ивентов).
    #[serde(default)]
    pub charts: u32,
    /// Носовые фигуры — заменяют корм при слиянии.
    #[serde(default)]
    pub figureheads: u32,
    /// Детали старого мира — создание и усиление снаряжения.
    #[serde(default)]
    pub details: u32,
    /// Эфир, который накопили Ветроловы и который ещё не собран.
    pub ether_pending: f32,
}

impl Profile {
    pub(crate) fn new(start: &Start) -> Self {
        Self {
            captain_level: start.captain_level,
            silver: start.silver,
            planks: start.planks,
            ether: start.ether,
            doubloons: start.doubloons,
            journals: start.journals,
            charts: 0,
            figureheads: 0,
            details: 0,
            ether_pending: start.ether_waiting,
        }
    }
}

/// Сколько этапов пройдено подряд с начала главы.
#[derive(Resource, Default, Clone, Serialize, Deserialize)]
pub struct Progress {
    pub cleared: usize,
    /// Просмотренные сцены сюжета.
    #[serde(default)]
    pub seen: Vec<String>,
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

    /// Длинная подсказка — например, отчёт «пока вас не было».
    pub(crate) fn show_long(&mut self, text: &str) {
        self.text = text.to_string();
        self.left = TOAST_LONG_SECONDS;
    }
}

/// Меню надо собрать заново (например, отсек достроился).
#[derive(Resource, Default)]
pub(crate) struct MenuRebuild(pub bool);

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
    /// Свой бой (конвой, Бездна) вместо этапа.
    pub custom: Option<CustomFight>,
    pub from: Screen,
}

impl Default for Voyage {
    fn default() -> Self {
        Self {
            stage: None,
            custom: None,
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
        .init_resource::<TilePage>()
        .add_systems(OnEnter(Screen::Menu), spawn_menu)
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
                    .or_else(in_state(Screen::Tavern))
                    .or_else(in_state(Screen::Activities)),
            ),
        )
        .add_systems(Update, count_victory.run_if(in_state(Screen::Battle)));
    #[cfg(feature = "dev")]
    app.add_systems(Update, dev_hull_key.run_if(in_state(Screen::Menu)));
}

/// Всё, из чего собирается экран корабля.
#[derive(SystemParam)]
struct MenuView<'w> {
    assets: Res<'w, AssetServer>,
    fonts: Res<'w, Fonts>,
    data: Res<'w, MenuData>,
    profile: Res<'w, Profile>,
    progress: Res<'w, Progress>,
    screen: Res<'w, BattleScreen>,
    ship_data: Res<'w, ShipData>,
    ship: Res<'w, Ship>,
    open: Res<'w, OpenCompartment>,
    contract_data: Res<'w, ContractData>,
    contracts: Res<'w, Contracts>,
    board_open: Res<'w, BoardOpen>,
    gear_data: Res<'w, crate::gear::GearData>,
    gear: Res<'w, crate::gear::Gear>,
    page: Res<'w, TilePage>,
}

fn spawn_menu(mut commands: Commands, view: MenuView) {
    build(&mut commands, &view);
}

fn rebuild_menu(
    mut commands: Commands,
    mut rebuild: ResMut<MenuRebuild>,
    roots: Query<Entity, With<MenuRoot>>,
    view: MenuView,
) {
    if !rebuild.0 {
        return;
    }
    rebuild.0 = false;
    for root in &roots {
        commands.entity(root).despawn();
    }
    build(&mut commands, &view);
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
        .observe(
            |ev: On<PointerPress>,
             mut nodes: Query<&mut UiTransform>,
             mut sfx: MessageWriter<Sfx>| {
                if let Ok(mut t) = nodes.get_mut(ev.event_target()) {
                    t.scale = Vec2::splat(PRESSED_SCALE);
                }
                sfx.write(Sfx("click"));
            },
        )
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

fn build(commands: &mut Commands, view: &MenuView) {
    let (assets, fonts, data, profile, progress, screen) = (
        &*view.assets,
        &*view.fonts,
        &*view.data,
        &*view.profile,
        &*view.progress,
        &*view.screen,
    );
    let (ship_data, ship) = (&*view.ship_data, &*view.ship);
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
            side_buttons(root, fonts, view.contracts.claimable(&view.contract_data));

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
                    format!("Корпус · ур. {}", ship.hull()),
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
                ImageNode::new(assets.load(ship_data.ship_image(ship.hull()).to_string())),
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
                |_: On<PointerClick>,
                 mut profile: ResMut<Profile>,
                 mut toast: ResMut<Toast>,
                 ship: Res<Ship>,
                 ship_data: Res<ShipData>,
                 mut sfx: MessageWriter<Sfx>,
                 mut deeds: MessageWriter<Deed>| {
                    let room = ship
                        .capacity(&ship_data)
                        .ether
                        .saturating_sub(profile.ether);
                    let got = (profile.ether_pending.floor() as u32).min(room);
                    if got == 0 && profile.ether_pending >= 1.0 {
                        toast.show("Трюм полон: эфир некуда класть — улучшите Трюм");
                    }
                    if got > 0 {
                        profile.ether += got;
                        profile.ether_pending -= got as f32;
                        sfx.write(Sfx("coin"));
                        deeds.write(Deed("ether"));
                        toast.show(&format!("Ветроловы: +{got} эфира"));
                    }
                },
            );

            compartments(root, assets, fonts, ship_data, ship, view.page.0);
            voyage(root, fonts, next, done);
            bottom_nav(root, assets, fonts, Screen::Menu);
            if view.board_open.0 {
                crate::contracts::board(root, fonts, &view.contract_data, &view.contracts);
            } else if let Some(id) = &view.open.0 {
                let workshop = (id == "workshop")
                    .then(|| ship.workshop_rarity(ship_data))
                    .flatten();
                crate::ship::panel(root, fonts, ship_data, ship, profile, id, |p| {
                    if let Some(max) = workshop {
                        crate::gear::workshop_section(
                            p,
                            fonts,
                            &view.gear_data,
                            &view.gear,
                            max,
                            profile,
                        );
                    }
                });
            }
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

fn side_buttons(root: &mut ChildSpawnerCommands, fonts: &Fonts, contracts_ready: bool) {
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
        ("Контракты", "задания", BRASS, contracts_ready, ""),
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
                if title == "Контракты" {
                    pressable(&mut button);
                    button.observe(
                        |_: On<PointerClick>,
                         mut open: ResMut<BoardOpen>,
                         mut panel: ResMut<OpenCompartment>,
                         mut rebuild: ResMut<MenuRebuild>| {
                            open.0 = true;
                            panel.0 = None;
                            rebuild.0 = true;
                        },
                    );
                } else {
                    later(&mut button, message);
                }
            }
        });
    }
}

fn compartments(
    root: &mut ChildSpawnerCommands,
    assets: &AssetServer,
    fonts: &Fonts,
    data: &ShipData,
    ship: &Ship,
    page: usize,
) {
    // Сначала открытые и полезные отсеки, потом открытые «на будущее», потом закрытые.
    let hull = ship.hull();
    let mut all: Vec<&Compartment> = data.compartments.iter().collect();
    all.sort_by_key(|c| (c.unlock_hull > hull, c.effect.is_none(), c.unlock_hull));
    // Не влезают все — пять плиток на страницу и шестая «Ещё».
    let per_page = if all.len() > COMPARTMENTS_SHOWN {
        COMPARTMENTS_SHOWN - 1
    } else {
        COMPARTMENTS_SHOWN
    };
    let pages = all.len().div_ceil(per_page);
    let page = page % pages.max(1);
    let shown: Vec<&Compartment> = all
        .iter()
        .skip(page * per_page)
        .take(per_page)
        .copied()
        .collect();

    root.spawn((
        Node {
            position_type: PositionType::Absolute,
            top: px(694),
            left: px(10),
            width: px(520),
            height: px(96),
            padding: UiRect::axes(px(6), px(4)),
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
            let open = compartment.unlock_hull <= hull;
            let level = ship.level(&compartment.id);
            let building = ship.build.as_ref().is_some_and(|b| b.id == compartment.id);
            let mut tile = row.spawn(Node {
                width: px(82),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                ..default()
            });
            tile.with_children(|t| {
                t.spawn((
                    Node {
                        width: px(66),
                        height: px(50),
                        ..default()
                    },
                    ImageNode {
                        color: if open && level > 0 {
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
                let (status, color) = if building {
                    ("строится".to_string(), GOLD)
                } else if !open {
                    (format!("Корпус {}", compartment.unlock_hull), GOLD)
                } else if level == 0 {
                    ("построить".to_string(), GOLD)
                } else {
                    (format!("ур. {level}"), MUTED)
                };
                t.spawn(text(fonts, status, building || level == 0, 12.0, color));
            });
            let id = compartment.id.clone();
            pressable(&mut tile);
            tile.observe(
                move |_: On<PointerClick>,
                      mut opened: ResMut<OpenCompartment>,
                      mut board: ResMut<BoardOpen>,
                      mut rebuild: ResMut<MenuRebuild>| {
                    board.0 = false;
                    opened.0 = Some(id.clone());
                    rebuild.0 = true;
                },
            );
        }
        if pages > 1 {
            let mut more = row.spawn(Node {
                width: px(82),
                flex_direction: FlexDirection::Column,
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            });
            more.with_children(|t| {
                t.spawn(text(fonts, "Ещё", true, 16.0, GOLD));
                t.spawn(text(
                    fonts,
                    format!("{}/{pages}", page + 1),
                    false,
                    12.0,
                    MUTED,
                ));
            });
            pressable(&mut more);
            more.observe(
                move |_: On<PointerClick>,
                      mut p: ResMut<TilePage>,
                      mut rebuild: ResMut<MenuRebuild>| {
                    p.0 = (page + 1) % pages;
                    rebuild.0 = true;
                },
            );
        }
    });
}

/// Страница в ряду отсеков.
#[derive(Resource, Default)]
pub(crate) struct TilePage(pub usize);

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
                    format!(
                        "{} · рекомендуемый уровень {}",
                        stage_title(&next.id),
                        next.recommended_level
                    )
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
             mut requests: MessageWriter<VoyageRequest>| {
                *voyage = Voyage::default();
                requests.write(VoyageRequest);
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
            Ok(Screen::Activities),
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

fn refresh_values(
    profile: Res<Profile>,
    ship: Res<Ship>,
    ship_data: Res<ShipData>,
    mut texts: Query<(&Value, &mut Text)>,
) {
    let cap = ship.capacity(&ship_data);
    for (value, mut text) in &mut texts {
        let shown = match value {
            Value::Silver => format!("{} / {}", profile.silver, cap.silver),
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

/// Добыча в запас — с потолком Трюма.
#[derive(SystemParam)]
pub(crate) struct Spoils<'w> {
    pub profile: ResMut<'w, Profile>,
    ship: Res<'w, Ship>,
    ship_data: Res<'w, ShipData>,
}

impl Spoils<'_> {
    pub fn give(&mut self, silver: u32, planks: u32, journals: u32) {
        let cap = self.ship.capacity(&self.ship_data);
        let p = &mut self.profile;
        p.silver = (p.silver + silver).min(cap.silver);
        p.planks = (p.planks + planks).min(cap.planks);
        p.journals += journals;
    }
}

/// Победа приносит добычу; на этапе главы — открывает следующий этап и сюжет.
#[allow(clippy::too_many_arguments)]
fn count_victory(
    screen: Res<BattleScreen>,
    data: Res<MenuData>,
    mut progress: ResMut<Progress>,
    mut spoils: Spoils,
    mut convoys: ResMut<Convoys>,
    mut counted: Local<Option<(usize, u64)>>,
    mut story: ResMut<StoryQueue>,
    mut notes: Query<&mut Text, With<ResultNote>>,
    mut deeds: MessageWriter<Deed>,
    abyss: (Res<AbyssData>, ResMut<Abyss>),
) {
    let (abyss_data, mut abyss) = abyss;
    let Some(outcome) = screen.shown_outcome else {
        return;
    };
    let battle = (screen.stage, screen.seed);
    if *counted == Some(battle) {
        return;
    }
    *counted = Some(battle);
    let note = match (outcome, &screen.custom) {
        (Outcome::Defeat, _) => "Усильте отряд или смените тактику".to_string(),
        (Outcome::Victory, Some(fight)) if matches!(fight.kind, FightKind::Abyss(_)) => {
            let FightKind::Abyss(floor) = fight.kind else {
                unreachable!()
            };
            deeds.write(Deed("win"));
            if floor == abyss.best + 1 {
                abyss.best = floor;
                let r = abyss_data.reward(floor);
                spoils.give(r.silver, 0, r.journals);
                let p = &mut spoils.profile;
                p.charts += r.charts;
                p.doubloons += r.doubloons;
                p.figureheads += r.figureheads;
                p.details += r.details;
                format!("Этаж {floor} пройден! {}", r.line())
            } else {
                "Этаж уже пройден — награда выдаётся один раз".to_string()
            }
        }
        (Outcome::Victory, Some(fight))
            if fight.kind == FightKind::Convoy && convoys.ready == 0 =>
        {
            "Конвоев в очереди нет — добычи нет. Новое судно скоро появится".to_string()
        }
        (Outcome::Victory, Some(fight)) => {
            spoils.give(fight.silver, fight.planks, fight.journals);
            deeds.write(Deed("win"));
            if fight.kind == FightKind::Convoy {
                convoys.ready -= 1;
                deeds.write(Deed("convoy"));
            }
            let mut parts = vec![format!("+{} серебра", fight.silver)];
            if fight.planks > 0 {
                parts.push(format!("+{} досок", fight.planks));
            }
            if fight.journals > 0 {
                parts.push(format!("+{} журналов", fight.journals));
            }
            parts.join(" · ")
        }
        (Outcome::Victory, None) => {
            let reward = &data.victory_reward;
            spoils.give(reward.silver, reward.planks, reward.journals);
            deeds.write(Deed("win"));
            let first = screen.stage == progress.cleared;
            if first {
                story.0.push(screen.data.stages[screen.stage].id.clone());
                progress.cleared += 1;
                spoils.profile.doubloons += data.first_clear_doubloons;
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
    };
    for mut text in &mut notes {
        text.0 = note.clone();
    }
}

/// H — следующий уровень Корпуса (1–7): другой корабль и открытые отсеки.
#[cfg(feature = "dev")]
fn dev_hull_key(
    keys: Res<ButtonInput<KeyCode>>,
    mut ship: ResMut<Ship>,
    mut rebuild: ResMut<MenuRebuild>,
) {
    if keys.just_pressed(KeyCode::KeyH) {
        ship.dev_next_hull();
        rebuild.0 = true;
    }
}
