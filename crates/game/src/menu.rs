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
use crate::events::{EventData, StormEvent, WeekEvents};
use crate::ship::{Compartment, OpenCompartment, RoomTimer, Ship, ShipData};
use crate::story::{StoryQueue, VoyageRequest};
use crate::view::{Fonts, ResultNote};

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
/// Сколько отсеков помещается в ряд под кораблём.
const COMPARTMENTS_SHOWN: usize = 6;
const TOAST_SECONDS: f32 = 2.2;
const TOAST_LONG_SECONDS: f32 = 6.0;
/// Корабль покачивается на ветру: период (с) и размах (px).
const BOB_PERIOD: f32 = 4.0;
const BOB_PX: f32 = 6.0;
const BOB_TILT_DEG: f32 = 1.2;
/// Корабль-разрез на главной: квадрат рисунка корпуса (логические px экрана 540×960).
const SHIP_LEFT: f32 = 10.0;
/// Ширина экрана в логических px.
const SCREEN_W: f32 = 540.0;
const SHIP_TOP: f32 = 190.0;
const SHIP_SIDE: f32 = 520.0;
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
    pub supplies: u32,
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
    /// Буревые ядра — из ивентов; для отсеков 8–10 и легендарного снаряжения.
    #[serde(default)]
    pub storm_cores: u32,
    /// Эфир, который накопили Ветроловы и который ещё не собран.
    pub ether_pending: f32,
    /// Припасы: победа на этапе главы их тратит (`supplies.rs`).
    #[serde(default = "start_supplies")]
    pub supplies: u32,
    /// Доля следующей единицы восстановления.
    #[serde(default)]
    pub supplies_regen: f32,
    /// Сваренное Камбузом, ещё не собранное.
    #[serde(default)]
    pub supplies_pending: f32,
}

/// Старые сохранения без припасов получают стартовый запас.
fn start_supplies() -> u32 {
    MenuData::bundled().start.supplies
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
            storm_cores: 0,
            ether_pending: start.ether_waiting,
            supplies: start.supplies,
            supplies_regen: 0.0,
            supplies_pending: 0.0,
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
    /// День (UTC), к которому относится `wins`.
    #[serde(default)]
    pub day: u64,
    /// Побед за этот день по этапам — для убывающей награды и дневного лимита.
    #[serde(default)]
    pub wins: Vec<u32>,
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
        .init_resource::<ShipOpen>()
        .add_systems(OnEnter(Screen::Menu), spawn_menu)
        .add_systems(OnEnter(Screen::Crew), spawn_sky)
        .add_systems(OnEnter(Screen::Tavern), spawn_sky)
        .add_systems(OnEnter(Screen::Activities), spawn_sky)
        // Небо на остальных экранах едет так же, как на главной (там — в общей цепочке ниже).
        .add_systems(
            Update,
            (drift_clouds, scroll_backdrop).run_if(not(in_state(Screen::Menu))),
        )
        .add_systems(
            Update,
            // После пересборки — сразу качка: иначе новый корабль кадр стоит на месте и дёргается.
            (
                rebuild_menu,
                (
                    refresh_values,
                    refresh_ether_bubble,
                    bob_ship,
                    spin_and_pulse,
                    drift_clouds,
                    scroll_backdrop,
                    animate_sails,
                ),
            )
                .chain()
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
    ship_open: Res<'w, ShipOpen>,
    event_data: Res<'w, EventData>,
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
            BackgroundColor(Color::srgb_u8(62, 104, 178)),
        ))
        .with_children(|root| {
            let open_ship = view.ship_open.0;
            backdrop(root, assets, &ship_data.view);
            clouds(root, assets, &ship_data.view, false);
            top_bar(root, assets, fonts, profile);
            if !open_ship {
                side_buttons(
                    root,
                    fonts,
                    view.event_data.current_short(),
                    view.contracts.claimable(&view.contract_data),
                );
            }

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
                title.spawn(crate::style::heading(
                    fonts,
                    format!("«{}»", data.ship_name),
                    32.0,
                    Color::WHITE,
                ));
                title.spawn(text(
                    fonts,
                    if open_ship {
                        "Нажмите на отсек — улучшить"
                    } else {
                        "Нажмите на корабль — осмотреть"
                    },
                    false,
                    13.0,
                    PARCHMENT,
                ));
            });

            if open_ship {
                // Полный вид: разрез со всеми комнатами, назад — к главной.
                ship_rooms(root, assets, fonts, ship_data, ship, view.open.0.as_deref());
                back_button(root, fonts);
            } else {
                ship_outside(root, assets, fonts, ship_data, ship);
            }
            if !open_ship {
                // Передние облака — только на главной: в полном виде закрывали бы комнаты.
                clouds(root, assets, &ship_data.view, true);
                compartments(root, assets, fonts, ship_data, ship, view.page.0);
                voyage(root, fonts, next, done);
            }
            bottom_nav(root, assets, fonts, Screen::Menu);
            if view.board_open.0 {
                crate::contracts::board(root, fonts, &view.contract_data, &view.contracts);
            } else if let Some(id) = &view.open.0 {
                let workshop = (id == "workshop")
                    .then(|| ship.workshop_rarity(ship_data))
                    .flatten();
                crate::ship::panel(root, assets, fonts, ship_data, ship, profile, id, |p| {
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

/// Корабль снаружи (главная) — в той же рамке, что и разрез: тап открывает полный вид с комнатами.
fn ship_outside(
    root: &mut ChildSpawnerCommands,
    assets: &AssetServer,
    fonts: &Fonts,
    ship_data: &ShipData,
    ship: &Ship,
) {
    let view = &ship_data.view;
    let pct = |v: f32| percent(v / view.size * 100.0);
    let mut outside = root.spawn((
        ShipSprite,
        Node {
            position_type: PositionType::Absolute,
            left: px(SHIP_LEFT),
            top: px(SHIP_TOP),
            width: px(SHIP_SIDE),
            height: px(SHIP_SIDE),
            ..default()
        },
        UiTransform::default(),
    ));
    outside.with_children(|deck| {
        deck.spawn((
            Node {
                position_type: PositionType::Absolute,
                width: percent(100),
                height: percent(100),
                ..default()
            },
            ImageNode::new(assets.load(view.exterior.clone())),
        ));
        // Улучшения над палубой (ветроловы) видны и снаружи.
        for room in view.rooms.iter().filter(|r| r.outside) {
            let Some(image) = room.image(ship.level(&room.id)) else {
                continue;
            };
            let [x, y, w, h] = room.rect;
            deck.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    left: pct(x),
                    top: pct(y),
                    width: pct(w),
                    height: pct(h),
                    ..default()
                },
                ImageNode::new(assets.load(image.to_string())),
            ));
        }
        if let Some(sails) = &view.sails {
            for (index, sail) in sails.layers.iter().enumerate() {
                let [x, y, w, h] = sail.rect;
                deck.spawn((
                    Node {
                        position_type: PositionType::Absolute,
                        left: pct(x),
                        top: pct(y),
                        width: pct(w),
                        height: pct(h),
                        ..default()
                    },
                    Pickable::IGNORE,
                ))
                .with_children(|frame| {
                    frame.spawn((
                        SailFurled(index),
                        Node {
                            position_type: PositionType::Absolute,
                            width: percent(100),
                            height: percent(100),
                            ..default()
                        },
                        ImageNode::new(assets.load(sail.furled.clone())),
                        Pickable::IGNORE,
                    ));
                    // Раскрытый парус показывается сверху вниз — раскатывается от реи.
                    frame
                        .spawn((
                            SailOpen(index),
                            Node {
                                position_type: PositionType::Absolute,
                                width: percent(100),
                                height: percent(0),
                                overflow: Overflow::clip(),
                                ..default()
                            },
                            Pickable::IGNORE,
                        ))
                        .with_children(|clip| {
                            clip.spawn((
                                Node {
                                    position_type: PositionType::Absolute,
                                    width: percent(100),
                                    height: px(SHIP_SIDE * h / view.size),
                                    ..default()
                                },
                                ImageNode::new(assets.load(sail.open.clone())),
                                Pickable::IGNORE,
                            ));
                        });
                });
            }
        }
        propeller(deck, assets, view);
    });
    outside.observe(
        |_: On<PointerClick>,
         mut open: ResMut<ShipOpen>,
         mut sfx: MessageWriter<Sfx>,
         mut rebuild: ResMut<MenuRebuild>| {
            open.0 = true;
            sfx.write(Sfx("click"));
            rebuild.0 = true;
        },
    );
    ether_bubble(root, fonts, px(60), px(300));
    crate::supplies::galley_bubble(root, fonts, px(330), px(300));
}

/// Пропеллер на корме, поверх руля: крутится всегда.
fn propeller(ship: &mut ChildSpawnerCommands, assets: &AssetServer, view: &crate::ship::ShipView) {
    let Some(prop) = &view.propeller else {
        return;
    };
    let pct = |v: f32| percent(v / view.size * 100.0);
    ship.spawn((
        Spinner(prop.turns_per_second),
        Node {
            position_type: PositionType::Absolute,
            left: pct(prop.at[0] - prop.size / 2.0),
            top: pct(prop.at[1] - prop.size / 2.0),
            width: pct(prop.size),
            height: pct(prop.size),
            ..default()
        },
        ImageNode::new(assets.load(prop.image.clone())),
        UiTransform::default(),
        Pickable::IGNORE,
    ));
}

/// Назад из полного вида корабля.
fn back_button(root: &mut ChildSpawnerCommands, fonts: &Fonts) {
    let mut back = root.spawn((
        Node {
            position_type: PositionType::Absolute,
            top: px(76),
            left: px(10),
            height: px(46),
            padding: UiRect::horizontal(px(16)),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            ..default()
        },
        crate::style::Skin(crate::style::Look::Dark),
        UiTransform::default(),
    ));
    back.with_child(crate::style::heading(fonts, "« Назад", 17.0, PARCHMENT));
    pressable(&mut back);
    back.observe(
        |_: On<PointerClick>,
         mut ship_open: ResMut<ShipOpen>,
         mut opened: ResMut<OpenCompartment>,
         mut rebuild: ResMut<MenuRebuild>| {
            ship_open.0 = false;
            opened.0 = None;
            rebuild.0 = true;
        },
    );
}

/// Открыт полный вид корабля (разрез с комнатами).
#[derive(Resource, Default)]
pub(crate) struct ShipOpen(pub bool);

/// Корабль-разрез: корпус, поверх — комнаты отсеков. Комната показывает уровень (свой рисунок
/// с нужного уровня), не построенная — заколочена, на стройке — леса и таймер. Тап — панель отсека.
fn ship_rooms(
    root: &mut ChildSpawnerCommands,
    assets: &AssetServer,
    fonts: &Fonts,
    ship_data: &ShipData,
    ship: &Ship,
    selected: Option<&str>,
) {
    let view = &ship_data.view;
    let pct = |v: f32| percent(v / view.size * 100.0);
    root.spawn((
        ShipSprite,
        Node {
            position_type: PositionType::Absolute,
            left: px(SHIP_LEFT),
            top: px(SHIP_TOP),
            width: px(SHIP_SIDE),
            height: px(SHIP_SIDE),
            ..default()
        },
        Pickable::IGNORE,
    ))
    .with_children(|deck| {
        deck.spawn((
            Node {
                position_type: PositionType::Absolute,
                width: percent(100),
                height: percent(100),
                ..default()
            },
            ImageNode::new(assets.load(view.hull.clone())),
            Pickable::IGNORE,
        ));
        propeller(deck, assets, view);
        for room in &view.rooms {
            let Some(compartment) = ship_data.compartment(&room.id) else {
                continue;
            };
            let [x, y, w, h] = room.rect;
            let level = ship.level(&room.id);
            let building = ship.build.as_ref().is_some_and(|b| b.id == room.id);
            let mut cell = deck.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    left: pct(x),
                    top: pct(y),
                    width: pct(w),
                    height: pct(h),
                    ..default()
                },
                UiTransform::default(),
            ));
            // Открытый в панели отсек — в пульсирующей золотой рамке.
            if selected == Some(room.id.as_str()) {
                cell.insert((SelectedRoom, Outline::new(px(3), px(1), GOLD)));
            }
            cell.with_children(|c| {
                let fill = || Node {
                    position_type: PositionType::Absolute,
                    width: percent(100),
                    height: percent(100),
                    ..default()
                };
                if let Some(image) = room.image(level) {
                    c.spawn((
                        fill(),
                        ImageNode::new(assets.load(image.to_string())),
                        Pickable::IGNORE,
                    ));
                }
                if building {
                    c.spawn((
                        fill(),
                        ImageNode::new(assets.load(view.scaffold.clone())),
                        Pickable::IGNORE,
                    ));
                    c.spawn((room_tag(room.tag), Pickable::IGNORE))
                        .with_child((RoomTimer, text(fonts, "", true, 12.0, GOLD)));
                } else if level == 0 {
                    // Не построен: доски с замком и что нужно, чтобы открыть.
                    c.spawn((
                        fill(),
                        ImageNode::new(assets.load(view.locked.clone())),
                        Pickable::IGNORE,
                    ));
                    let need = if ship.hull() < compartment.unlock_hull {
                        format!("Корпус {}", compartment.unlock_hull)
                    } else {
                        "Построить".to_string()
                    };
                    c.spawn((room_tag(room.tag), Pickable::IGNORE))
                        .with_child(crate::style::heading(fonts, need, 13.0, GOLD));
                } else {
                    c.spawn((room_tag(room.tag), Pickable::IGNORE)).with_child(
                        crate::style::heading(fonts, format!("ур. {level}"), 12.0, PARCHMENT),
                    );
                }
            });
            let id = room.id.clone();
            pressable(&mut cell);
            cell.observe(
                move |_: On<PointerClick>,
                      mut opened: ResMut<OpenCompartment>,
                      mut board: ResMut<BoardOpen>,
                      mut rebuild: ResMut<MenuRebuild>| {
                    board.0 = false;
                    opened.0 = Some(id.clone());
                    rebuild.0 = true;
                },
            );
            // Ветроловы накопили эфир — пузырь над ними, тап собирает.
            if room.id == "windcatchers" {
                ether_bubble(deck, fonts, pct(x + w * 0.3), pct(y - 10.0));
            }
        }
    });
}

/// Пузырь накопленного эфира: тап собирает в запас (с потолком Трюма).
fn ether_bubble(parent: &mut ChildSpawnerCommands, fonts: &Fonts, left: Val, top: Val) {
    let mut bubble = parent.spawn((
        EtherBubble,
        Node {
            position_type: PositionType::Absolute,
            left,
            top,
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
}

/// Облака плывут влево — корабль летит вправо. Положение — от часов, поэтому после
/// пересборки меню облака не прыгают.
fn clouds(
    root: &mut ChildSpawnerCommands,
    assets: &AssetServer,
    view: &crate::ship::ShipView,
    front: bool,
) {
    for cloud in view.clouds.iter().filter(|c| c.front == front) {
        root.spawn((
            Cloud {
                speed: cloud.speed,
                phase: cloud.phase,
                width: cloud.width,
            },
            Node {
                position_type: PositionType::Absolute,
                left: px(0),
                top: px(cloud.top),
                width: px(cloud.width),
                ..default()
            },
            ImageNode {
                color: Color::WHITE.with_alpha(cloud.alpha),
                ..ImageNode::new(assets.load(cloud.image.clone()))
            },
            UiTransform::default(),
            Pickable::IGNORE,
        ));
    }
}

/// Живое небо главной — общий фон Экипажа, Таверны и Активностей: отдельный слой позади
/// экрана, поэтому при пересборке экрана небо не дёргается.
fn spawn_sky(
    mut commands: Commands,
    state: Res<State<Screen>>,
    assets: Res<AssetServer>,
    data: Res<ShipData>,
) {
    commands
        .spawn((
            DespawnOnExit(*state.get()),
            Node {
                position_type: PositionType::Absolute,
                width: percent(100),
                height: percent(100),
                overflow: Overflow::clip(),
                ..default()
            },
            BackgroundColor(Color::srgb_u8(62, 104, 178)),
            GlobalZIndex(-10),
            Pickable::IGNORE,
        ))
        .with_children(|root| {
            backdrop(root, &assets, &data.view);
            clouds(root, &assets, &data.view, false);
        });
}

/// Панорама неба за кораблём (два куска встык едут влево) и стайка птиц.
fn backdrop(root: &mut ChildSpawnerCommands, assets: &AssetServer, view: &crate::ship::ShipView) {
    let sky = &view.backdrop;
    for copy in 0..2 {
        root.spawn((
            Backdrop {
                copy: copy as f32,
                width: sky.width,
                speed: sky.speed,
            },
            Node {
                position_type: PositionType::Absolute,
                left: px(0),
                top: px(0),
                width: px(sky.width),
                height: percent(100),
                ..default()
            },
            ImageNode::new(assets.load(sky.image.clone())),
            UiTransform::default(),
            Pickable::IGNORE,
        ));
    }
    if let Some(birds) = &view.birds {
        for i in 0..birds.count {
            // Клин: первая впереди, остальные позади и попеременно выше-ниже.
            let back = i.div_ceil(2) as f32;
            let side = if i % 2 == 0 { 1.0 } else { -1.0 };
            let size = 22.0 - back * 2.0;
            root.spawn((
                Bird {
                    offset: Vec2::new(-back * 34.0, side * back * 16.0),
                    phase: i as f32 * 1.7,
                },
                Node {
                    position_type: PositionType::Absolute,
                    left: px(0),
                    top: px(birds.top),
                    width: px(size),
                    height: px(size / 2.0),
                    ..default()
                },
                ImageNode::new(assets.load(birds.image.clone())),
                UiTransform::default(),
                Pickable::IGNORE,
            ));
        }
    }
}

#[derive(Component)]
struct Backdrop {
    copy: f32,
    width: f32,
    speed: f32,
}

#[derive(Component)]
struct Bird {
    offset: Vec2,
    phase: f32,
}

/// Фон едет от часов: после пересборки меню не прыгает.
fn scroll_backdrop(
    time: Res<Time>,
    data: Res<ShipData>,
    mut skies: Query<(&Backdrop, &mut UiTransform), Without<Bird>>,
    mut birds: Query<(&Bird, &mut UiTransform), Without<Backdrop>>,
) {
    let t = time.elapsed_secs();
    for (sky, mut transform) in &mut skies {
        let x = -(t * sky.speed).rem_euclid(sky.width) + sky.copy * sky.width;
        transform.translation = Val2::px(x, 0.0);
    }
    let Some(flock) = &data.view.birds else {
        return;
    };
    // Стайка обгоняет корабль слева направо раз в `every` секунд, остальное время — за экраном.
    let x = (t % flock.every) * flock.speed - 60.0;
    let drift = (t * 0.7).sin() * 10.0;
    for (bird, mut transform) in &mut birds {
        let flap = (t * 7.0 + bird.phase).sin();
        transform.translation = Val2::px(x + bird.offset.x, bird.offset.y + drift);
        transform.scale = Vec2::new(1.0, 0.35 + 0.65 * flap.abs()) * Vec2::new(1.0, flap.signum());
    }
}

/// Облако фона главной.
#[derive(Component)]
struct Cloud {
    speed: f32,
    phase: f32,
    width: f32,
}

fn drift_clouds(time: Res<Time>, mut clouds: Query<(&Cloud, &mut UiTransform)>) {
    let t = time.elapsed_secs();
    for (cloud, mut transform) in &mut clouds {
        // Путь — ширина экрана и самого облака; облако уходит за левый край и выходит справа.
        let path = SCREEN_W + cloud.width;
        let x = SCREEN_W - (cloud.phase * path + t * cloud.speed).rem_euclid(path);
        transform.translation = Val2::px(x, 0.0);
    }
}

/// Плашка комнаты: уровень, таймер стройки или что нужно для постройки. `at` — левый нижний
/// угол в долях комнаты (по умолчанию — внизу слева).
fn room_tag(at: [f32; 2]) -> impl Bundle {
    (
        Node {
            position_type: PositionType::Absolute,
            left: percent(at[0] * 100.0),
            bottom: percent(at[1] * 100.0),
            margin: UiRect::all(px(4)),
            padding: UiRect::axes(px(6), px(1)),
            border_radius: BorderRadius::all(px(6)),
            ..default()
        },
        BackgroundColor(Color::srgba(0.05, 0.04, 0.08, 0.7)),
    )
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

/// Полоса ресурсов главной — как на остальных экранах, плюс уровень капитана, вместимость
/// трюма у серебра и «+» у дублонов. Числа обновляет `refresh_values`.
fn top_bar(
    root: &mut ChildSpawnerCommands,
    assets: &AssetServer,
    fonts: &Fonts,
    profile: &Profile,
) {
    crate::style::bar(root).with_children(|bar| {
        let mut avatar = bar.spawn((
            Node {
                width: px(44),
                height: px(44),
                flex_shrink: 0.0,
                flex_direction: FlexDirection::Column,
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                border: UiRect::all(px(3)),
                border_radius: BorderRadius::MAX,
                ..default()
            },
            BackgroundColor(Color::srgb(0.16, 0.10, 0.06)),
            BorderColor::all(BRASS),
        ));
        avatar.with_children(|a| {
            a.spawn(text(fonts, "ур.", false, 10.0, MUTED));
            a.spawn(crate::style::heading(
                fonts,
                profile.captain_level.to_string(),
                18.0,
                PARCHMENT,
            ));
        });
        later(&mut avatar, "Профиль капитана появится позже");

        for (value, kind, size) in [
            (Value::Silver, "silver", 16.0),
            (Value::Planks, "planks", 16.0),
            (Value::Ether, "ether", 16.0),
            (Value::Doubloons, "doubloons", 16.0),
        ] {
            let mut slot =
                crate::style::resource_slot(bar, assets, fonts, kind, String::new(), size, value);
            if !matches!(value, Value::Doubloons) {
                // Тап — сколько есть и сколько вмещает Трюм.
                slot.observe(
                    move |_: On<PointerClick>,
                          profile: Res<Profile>,
                          ship: Res<Ship>,
                          ship_data: Res<ShipData>,
                          mut toast: ResMut<Toast>| {
                        let cap = ship.capacity(&ship_data);
                        let (name, n, max) = match value {
                            Value::Silver => ("Серебро", profile.silver, cap.silver),
                            Value::Planks => ("Доски", profile.planks, cap.planks),
                            _ => ("Эфир", profile.ether, cap.ether),
                        };
                        toast.show(&format!(
                            "{name}: {n} из {max} — больше Трюм не вмещает, улучшайте его"
                        ));
                    },
                );
            }
            if matches!(value, Value::Doubloons) {
                slot.with_children(|s| {
                    let mut plus = s.spawn((
                        Node {
                            position_type: PositionType::Absolute,
                            right: px(-6),
                            width: px(24),
                            height: px(24),
                            justify_content: JustifyContent::Center,
                            align_items: AlignItems::Center,
                            border: UiRect::all(px(2)),
                            border_radius: BorderRadius::MAX,
                            ..default()
                        },
                        BackgroundColor(Color::srgb(0.30, 0.62, 0.36)),
                        BorderColor::all(Color::srgb(0.85, 0.72, 0.40)),
                    ));
                    plus.with_child(crate::style::heading(fonts, "+", 17.0, Color::WHITE));
                    later(&mut plus, "Магазин дублонов появится позже");
                });
            }
        }
    });
}

/// Кнопки по бокам главной: слева — ивент недели, справа — контракты.
/// Боевой пропуск и почта появятся здесь, когда будут готовы (сейчас кнопок нет).
fn side_buttons(
    root: &mut ChildSpawnerCommands,
    fonts: &Fonts,
    event: &str,
    contracts_ready: bool,
) {
    let column = |left: bool| Node {
        position_type: PositionType::Absolute,
        top: px(76),
        left: if left { px(10) } else { auto() },
        right: if left { auto() } else { px(10) },
        flex_direction: FlexDirection::Column,
        row_gap: px(10),
        ..default()
    };
    let items: [(&str, &str, Color, bool); 2] = [
        (event, "ивент", STORM, false),
        ("Контракты", "задания", BRASS, contracts_ready),
    ];
    for (left, chunk) in [(true, &items[..1]), (false, &items[1..])] {
        root.spawn(column(left)).with_children(|col| {
            for &(title, sub, edge, dot) in chunk {
                let mut button = col.spawn((
                    Node {
                        width: px(84),
                        height: px(62),
                        flex_direction: FlexDirection::Column,
                        justify_content: JustifyContent::Center,
                        align_items: AlignItems::Center,
                        ..default()
                    },
                    crate::style::Skin(if edge == STORM {
                        crate::style::Look::Dark
                    } else {
                        crate::style::Look::TabOff
                    }),
                ));
                button.with_children(|b| {
                    b.spawn(crate::style::heading(fonts, title, 15.0, PARCHMENT));
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
                if edge == STORM {
                    // Сразу на страницу ивента недели, а не в список Активностей.
                    pressable(&mut button);
                    button.observe(
                        |_: On<PointerClick>,
                         mut page: ResMut<crate::abyss::ActivitiesPage>,
                         mut next: ResMut<NextState<Screen>>| {
                            page.0 = Some(crate::abyss::Page::Event);
                            next.set(Screen::Activities);
                        },
                    );
                } else {
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
            top: px(682),
            left: px(10),
            width: px(520),
            height: px(108),
            padding: UiRect::all(px(6)),
            column_gap: px(5),
            border: UiRect::all(px(2)),
            border_radius: BorderRadius::all(px(14)),
            ..default()
        },
        BackgroundColor(Color::srgba(0.07, 0.05, 0.03, 0.88)),
        BorderColor::all(BRASS),
    ))
    .with_children(|row| {
        for compartment in shown {
            let open = compartment.unlock_hull <= hull;
            let level = ship.level(&compartment.id);
            let building = ship.build.as_ref().is_some_and(|b| b.id == compartment.id);
            // Плитка отсека: тёмное дерево в латунной кайме; стройка — янтарная кайма.
            let mut tile = row.spawn((
                Node {
                    flex_grow: 1.0,
                    flex_basis: px(0),
                    padding: UiRect::top(px(4)),
                    flex_direction: FlexDirection::Column,
                    align_items: AlignItems::Center,
                    border: UiRect::all(px(if building { 2.0 } else { 1.0 })),
                    border_radius: BorderRadius::all(px(10)),
                    ..default()
                },
                BackgroundColor(Color::srgba(0.16, 0.10, 0.06, 0.92)),
                BorderColor::all(if building {
                    GOLD
                } else {
                    Color::srgb(0.72, 0.55, 0.26)
                }),
            ));
            tile.with_children(|t| {
                t.spawn((
                    Node {
                        width: px(64),
                        height: px(48),
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
                    crate::style::heading(
                        fonts,
                        compartment
                            .short
                            .clone()
                            .unwrap_or_else(|| compartment.name.clone()),
                        14.0,
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
                t.spawn(text(fonts, status, building || level == 0, 11.0, color));
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
            let mut more = row.spawn((
                Node {
                    flex_grow: 1.0,
                    flex_basis: px(0),
                    flex_direction: FlexDirection::Column,
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    ..default()
                },
                crate::style::Skin(crate::style::Look::Dark),
            ));
            more.with_children(|t| {
                t.spawn(crate::style::heading(fonts, "Ещё »", 17.0, GOLD));
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
            top: px(796),
            left: px(10),
            width: px(520),
            height: px(76),
            padding: UiRect::new(px(18), px(10), px(8), px(8)),
            align_items: AlignItems::Center,
            column_gap: px(12),
            ..default()
        },
        crate::style::Skin(crate::style::Look::Tile),
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
                info.spawn(crate::style::heading(
                    fonts,
                    next.name.clone(),
                    22.0,
                    Color::srgb(1.0, 0.93, 0.78),
                ));
                info.spawn(crate::supplies::label(fonts, 12.0, PARCHMENT));
            });
        let mut go = panel.spawn((
            Node {
                width: px(140),
                height: px(56),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            crate::style::Skin(crate::style::Look::Go),
        ));
        go.with_child(crate::style::heading(fonts, "В поход", 24.0, Color::WHITE));
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
    let tabs: [(&str, &str, Screen); 5] = [
        ("Корабль", "ship", Screen::Menu),
        ("Экипаж", "crew", Screen::Crew),
        ("Карта", "map", Screen::Map),
        ("Таверна", "tavern", Screen::Tavern),
        ("Активности", "activities", Screen::Activities),
    ];
    root.spawn((
        Node {
            position_type: PositionType::Absolute,
            bottom: px(0),
            left: px(0),
            width: percent(100),
            height: px(86),
            border: UiRect::top(px(3)),
            ..default()
        },
        BackgroundColor(DARK_WOOD),
        BorderColor::all(BRASS),
        GlobalZIndex(5),
    ))
    .with_children(|nav| {
        for (i, (name, icon, screen)) in tabs.into_iter().enumerate() {
            let here = screen == active;
            // Плитки из тёмного дерева с латунными разделителями; текущая — светлее,
            // с золотой полосой сверху.
            let mut tab = nav.spawn((
                Node {
                    flex_grow: 1.0,
                    flex_basis: px(0),
                    flex_direction: FlexDirection::Column,
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    border: UiRect::new(
                        px(if i == 0 { 0.0 } else { 1.0 }),
                        px(0),
                        px(if here { 3.0 } else { 0.0 }),
                        px(0),
                    ),
                    ..default()
                },
                BackgroundColor(if here {
                    Color::srgb(0.36, 0.25, 0.12)
                } else {
                    Color::srgb(0.16, 0.10, 0.06)
                }),
                BorderColor {
                    left: Color::srgb(0.47, 0.34, 0.16),
                    top: GOLD,
                    ..BorderColor::all(Color::NONE)
                },
            ));
            tab.with_children(|t| {
                t.spawn((
                    Node {
                        width: px(if here { 54.0 } else { 48.0 }),
                        height: px(if here { 54.0 } else { 48.0 }),
                        ..default()
                    },
                    ImageNode::new(assets.load(format!("ui/nav_{icon}.png"))),
                    Pickable::IGNORE,
                ));
                t.spawn(crate::style::heading(
                    fonts,
                    name,
                    15.0,
                    if here { GOLD } else { PARCHMENT },
                ));
            });
            if !here {
                pressable(&mut tab);
                tab.observe(
                    move |_: On<PointerClick>,
                          mut next: ResMut<NextState<Screen>>,
                          mut page: ResMut<crate::abyss::ActivitiesPage>| {
                        // Вкладка «Активности» открывает список, а не последний раздел.
                        page.0 = None;
                        next.set(screen);
                    },
                );
            }
        }
    });
}

fn refresh_values(
    profile: Res<Profile>,
    ship: Res<Ship>,
    ship_data: Res<ShipData>,
    mut texts: Query<(&Value, &mut Text, &mut TextColor)>,
) {
    // Потолок Трюма не пишем (не влезает) — полный Трюм видно по цвету числа.
    let cap = ship.capacity(&ship_data);
    for (value, mut text, mut color) in &mut texts {
        let (n, cap) = match value {
            Value::Silver => (profile.silver, cap.silver),
            Value::Planks => (profile.planks, cap.planks),
            Value::Ether => (profile.ether, cap.ether),
            Value::Doubloons => (profile.doubloons, u32::MAX),
        };
        let shown = crate::style::compact(n);
        if text.0 != shown {
            text.0 = shown;
        }
        color.0 = if n >= cap {
            Color::srgb(1.0, 0.55, 0.35)
        } else {
            Color::srgb(1.0, 0.93, 0.78)
        };
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

/// Пропеллер: оборотов в секунду.
#[derive(Component)]
struct Spinner(f32);

/// Скатка паруса и рамка раскрытого паруса (номер паруса в данных).
#[derive(Component)]
struct SailFurled(usize);

#[derive(Component)]
struct SailOpen(usize);

fn animate_sails(
    time: Res<Time>,
    data: Res<ShipData>,
    mut furled: Query<(&SailFurled, &mut ImageNode)>,
    mut open: Query<(&SailOpen, &mut Node)>,
) {
    let Some(sails) = &data.view.sails else {
        return;
    };
    let t = time.elapsed_secs();
    for (sail, mut image) in &mut furled {
        // Скатка гаснет в первой половине раскрытия: дальше её закрывает парус.
        let s = sails.openness(sail.0, t);
        image.color = Color::WHITE.with_alpha((1.0 - s * 2.0).clamp(0.0, 1.0));
    }
    for (sail, mut node) in &mut open {
        node.height = percent(sails.openness(sail.0, t) * 100.0);
    }
}

/// Комната отсека, открытого в панели.
#[derive(Component)]
struct SelectedRoom;

fn spin_and_pulse(
    time: Res<Time>,
    mut spinners: Query<(&Spinner, &mut UiTransform)>,
    mut selected: Query<&mut Outline, With<SelectedRoom>>,
) {
    let t = time.elapsed_secs();
    for (spinner, mut transform) in &mut spinners {
        transform.rotation = Rot2::radians(-t * spinner.0 * std::f32::consts::TAU);
    }
    let glow = 0.55 + 0.45 * (t * 5.0).sin().abs();
    for mut outline in &mut selected {
        outline.color = GOLD.with_alpha(glow);
    }
}

fn bob_ship(time: Res<Time>, mut ships: Query<&mut UiTransform, With<ShipSprite>>) {
    let t = time.elapsed_secs() / BOB_PERIOD * std::f32::consts::TAU;
    for mut transform in &mut ships {
        transform.translation = Val2::px(0.0, BOB_PX * t.sin());
        // Нос чуть клюёт вслед за качкой.
        transform.rotation = Rot2::degrees(BOB_TILT_DEG * t.cos());
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
    events: (Res<EventData>, ResMut<StormEvent>, ResMut<WeekEvents>),
    supply: Res<crate::supplies::SupplyData>,
) {
    let (abyss_data, mut abyss) = abyss;
    let (event_data, mut storm, mut week_events) = events;
    let Some(outcome) = screen.shown_outcome else {
        return;
    };
    let battle = (screen.stage, screen.seed);
    if *counted == Some(battle) {
        return;
    }
    *counted = Some(battle);
    let note = match (outcome, &screen.custom) {
        // Буря: считается урон в любом исходе.
        (_, Some(fight)) if fight.kind == FightKind::Storm => {
            let damage: u32 = screen
                .battle
                .units()
                .iter()
                .filter(|u| u.side == battle_core::Side::Enemy)
                .map(|u| u.def.stats.max_hp - u.hp)
                .sum();
            if outcome == Outcome::Victory {
                deeds.write(Deed("win"));
            }
            storm.record(damage, &event_data.storm, &mut spoils.profile)
        }
        (Outcome::Defeat, Some(fight)) if matches!(fight.kind, FightKind::Blockade(_)) => {
            let FightKind::Blockade(wave) = fight.kind else {
                unreachable!()
            };
            week_events.blockade_lose(wave)
        }
        (Outcome::Victory, Some(fight)) if matches!(fight.kind, FightKind::Blockade(_)) => {
            let FightKind::Blockade(wave) = fight.kind else {
                unreachable!()
            };
            deeds.write(Deed("win"));
            // Здоровье выживших — дальше, в следующую волну; павшие выбывают.
            let allies: Vec<&battle_core::Unit> = screen
                .battle
                .units()
                .iter()
                .filter(|u| u.side == battle_core::Side::Ally)
                .collect();
            let hp = screen
                .team
                .iter()
                .zip(allies)
                .map(|(m, u)| (m.id.clone(), u.hp as f32 / u.def.stats.max_hp as f32))
                .collect();
            week_events.blockade_win(wave, hp, &event_data.blockade, &mut spoils.profile)
        }
        (Outcome::Defeat, _) => "Усильте отряд или смените тактику".to_string(),
        (Outcome::Victory, Some(fight)) if matches!(fight.kind, FightKind::Regatta(_)) => {
            let FightKind::Regatta(leg) = fight.kind else {
                unreachable!()
            };
            deeds.write(Deed("win"));
            week_events.finish_leg(leg, &event_data.regatta, &mut spoils.profile)
        }
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
            // Попутный ветер по выходным: добыча с конвоя больше.
            let k = if fight.kind == FightKind::Convoy {
                event_data.convoy_multiplier()
            } else {
                1
            };
            let (silver, planks) = (fight.silver * k, fight.planks * k);
            spoils.give(silver, planks, fight.journals);
            deeds.write(Deed("win"));
            if fight.kind == FightKind::Convoy {
                convoys.ready -= 1;
                deeds.write(Deed("convoy"));
            }
            let mut parts = vec![format!("+{silver} серебра")];
            if planks > 0 {
                parts.push(format!("+{planks} досок"));
            }
            if k > 1 {
                parts.push(format!("попутный ветер ×{k}"));
            }
            if fight.journals > 0 {
                parts.push(format!("+{} журналов", fight.journals));
            }
            parts.join(" · ")
        }
        (Outcome::Victory, None) => {
            // Повторные победы за день приносят меньше (supplies.rs); первая — всегда целиком.
            let today = crate::cabins::today();
            let wins = progress.wins_today(screen.stage, today);
            let percent = supply.percent(wins).unwrap_or(0);
            progress.add_win(screen.stage, today);
            spoils.profile.supplies = spoils.profile.supplies.saturating_sub(supply.cost);
            let cut = |n: u32| n * percent / 100;
            let reward = &data.victory_reward;
            let reward = crate::menu::Reward {
                silver: cut(reward.silver),
                planks: cut(reward.planks),
                journals: cut(reward.journals),
            };
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
            let replay = if percent < 100 {
                format!(" · повтор: {percent}%")
            } else {
                String::new()
            };
            format!(
                "+{} серебра · +{} досок · +{} журналов{doubloons}{replay}\n−{} припасов (осталось {}) · побед сегодня {}/{}{opened}",
                reward.silver,
                reward.planks,
                reward.journals,
                supply.cost,
                spoils.profile.supplies,
                wins + 1,
                supply.daily_limit()
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
