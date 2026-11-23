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
use crate::ship::{Compartment, OpenCompartment, RoomTimer, SHIP_ART, SHIPYARD, Ship, ShipData};
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
/// На осмотре: палец проехал больше — это перетаскивание, а не тап по комнате; насколько
/// край корабля может отойти внутрь экрана (px).
/// Ширина экрана в логических px.
const SCREEN_W: f32 = 540.0;
const PRESSED_SCALE: f32 = 0.94;

/// Данные меню: `data/menu.json`.
#[derive(Resource, Deserialize, Clone)]
pub struct MenuData {
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
    /// Буревые ядра — из ивентов; для отсеков 8–10 и легендарного снаряжения.
    #[serde(default)]
    pub storm_cores: u32,
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
            storm_cores: 0,
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
        .init_resource::<ShipCam>()
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
                    (frame_ship, animate_sails).chain(),
                    spin_and_pulse,
                    drift_clouds,
                    scroll_backdrop,
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
        .add_systems(Update, count_victory.run_if(in_state(Screen::Battle)))
        .add_systems(Update, ship_mips);
    #[cfg(feature = "dev")]
    app.add_systems(Update, dev_hull_key.run_if(in_state(Screen::Menu)));
}

/// Всё, из чего собирается экран корабля.
#[derive(SystemParam)]
struct MenuView<'w> {
    assets: Res<'w, AssetServer>,
    fonts: Res<'w, Fonts>,
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
    cam: Res<'w, ShipCam>,
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
    let (assets, fonts, profile, progress, screen) = (
        &*view.assets,
        &*view.fonts,
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
            let open_ship = view.cam.open;
            backdrop(root, assets, &ship_data.scene);
            clouds(root, assets, &ship_data.scene, false);
            // Корабль раньше верхней полосы: на осмотре он крупный и уходит под неё.
            ship_view(root, assets, fonts, ship_data, ship, view.open.0.as_deref());
            // Передние облака на осмотре гаснут: закрывали бы комнаты.
            clouds(root, assets, &ship_data.scene, true);
            // Кнопки главной на осмотре уезжают за края, «Назад» выезжает.
            slide(root, Vec2::new(0.0, -260.0), false, |r| {
                side_buttons(r, fonts, view.contracts.claimable(&view.contract_data));
            });
            slide(root, Vec2::new(0.0, 290.0), false, |r| {
                compartments(r, assets, fonts, ship_data, ship, view.page.0);
                voyage(r, fonts, next, done);
            });
            slide(root, Vec2::new(-130.0, 0.0), true, |r| {
                back_button(r, fonts)
            });
            // Верхняя полоса поверх всего: кнопки уезжают под неё.
            top_bar(root, assets, fonts, profile);

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
                    format!("«{}»", ship.current(ship_data).name),
                    32.0,
                    Color::WHITE,
                ));
                title.spawn(text(
                    fonts,
                    if open_ship {
                        "Тап по отсеку — улучшить"
                    } else {
                        "Нажмите на корабль — осмотреть"
                    },
                    false,
                    13.0,
                    PARCHMENT,
                ));
            });
            bottom_nav(root, assets, fonts, Screen::Menu);
            if view.board_open.0 {
                crate::contracts::board(root, fonts, &view.contract_data, &view.contracts);
            } else if view.open.0.as_deref() == Some(SHIPYARD) {
                crate::ship::shipyard(
                    root,
                    assets,
                    fonts,
                    ship_data,
                    ship,
                    profile,
                    crate::ship::chapters_done(stages, progress.cleared),
                );
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

/// Корабль: снизу разрез с комнатами, поверх — он же снаружи (с парусами). Снаружи — на
/// главной, тап приближает к осмотру: наружный облик гаснет, открывая комнаты. Где корабль на
/// экране и насколько он крупный, задаёт `frame_ship` каждый кадр.
fn ship_view(
    root: &mut ChildSpawnerCommands,
    assets: &AssetServer,
    fonts: &Fonts,
    ship_data: &ShipData,
    ship: &Ship,
    selected: Option<&str>,
) {
    let view = ship.current(ship_data);
    let pct = |v: f32| percent(v / SHIP_ART * 100.0);
    let mut sprite = root.spawn((
        ShipSprite,
        Node {
            position_type: PositionType::Absolute,
            ..default()
        },
        UiTransform::default(),
    ));
    sprite.with_children(|deck| {
        ship_rooms(deck, assets, fonts, ship_data, ship, selected);
        deck.spawn((
            Exterior,
            Node {
                position_type: PositionType::Absolute,
                width: percent(100),
                height: percent(100),
                ..default()
            },
        ))
        .with_children(|outside| {
            outside.spawn((
                ExteriorImage,
                Node {
                    position_type: PositionType::Absolute,
                    width: percent(100),
                    height: percent(100),
                    ..default()
                },
                ImageNode::new(assets.load(view.exterior())),
                Pickable::IGNORE,
            ));
            // Улучшения над палубой (ветроловы) видны и снаружи.
            for room in view.rooms.iter().filter(|r| r.outside) {
                let Some(image) = room.image(ship.level(&room.id)) else {
                    continue;
                };
                let [x, y, w, h] = room.rect;
                outside.spawn((
                    ExteriorImage,
                    Node {
                        position_type: PositionType::Absolute,
                        left: pct(x),
                        top: pct(y),
                        width: pct(w),
                        height: pct(h),
                        ..default()
                    },
                    ImageNode::new(assets.load(image.to_string())),
                    Pickable::IGNORE,
                ));
            }
            if let Some(sails) = &view.sails {
                for (index, sail) in sails.layers.iter().enumerate() {
                    spawn_sail(outside, assets, SHIP_ART, sails.slices, index, sail);
                }
            }
        })
        .observe(
            |_: On<PointerClick>,
             mut cam: ResMut<ShipCam>,
             mut sfx: MessageWriter<Sfx>,
             mut rebuild: ResMut<MenuRebuild>| {
                if cam.open {
                    return;
                }
                cam.open = true;
                sfx.write(Sfx("click"));
                rebuild.0 = true;
            },
        );
        propeller(deck, assets, view);
        // Ветроловы накопили эфир — пузырь над кораблём, тап собирает.
        let [x, y] = view.bubble;
        ether_bubble(deck, fonts, pct(x), pct(y));
    });
}

/// Парус: скатка на рее и полотно, нарезанное на полосы (каждая колышется сама).
fn spawn_sail(
    parent: &mut ChildSpawnerCommands,
    assets: &AssetServer,
    size: f32,
    slices: usize,
    index: usize,
    sail: &crate::ship::SailLayer,
) {
    let pct = |v: f32| percent(v / size * 100.0);
    let [x, y, w, h] = sail.rect;
    parent
        .spawn((
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
                SailBundle(index),
                Node {
                    position_type: PositionType::Absolute,
                    width: percent(100),
                    height: percent(100),
                    ..default()
                },
                ImageNode::new(assets.load(sail.furled.clone())),
                Pickable::IGNORE,
            ));
            let cloth = assets.load(sail.open.clone());
            let n = slices.max(1) as f32;
            for k in 0..slices.max(1) {
                // Полосы чуть заходят друг на друга: при ряби между ними не видно щелей.
                let (top, bottom) = (k as f32 / n, ((k as f32 + SLICE_OVERLAP) / n).min(1.0));
                frame.spawn((
                    SailSlice { sail: index, k },
                    Node {
                        position_type: PositionType::Absolute,
                        top: percent(top * 100.0),
                        width: percent(100),
                        height: percent((bottom - top) * 100.0),
                        ..default()
                    },
                    ImageNode {
                        rect: Some(Rect::new(0.0, top * h, w, bottom * h)),
                        ..ImageNode::new(cloth.clone())
                    },
                    UiTransform::default(),
                    Pickable::IGNORE,
                ));
            }
        });
}

/// Пропеллер на корме, поверх руля: крутится всегда.
fn propeller(ship: &mut ChildSpawnerCommands, assets: &AssetServer, view: &crate::ship::ShipDef) {
    let Some(prop) = &view.propeller else {
        return;
    };
    let pct = |v: f32| percent(v / SHIP_ART * 100.0);
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

/// Назад из осмотра корабля.
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
         mut cam: ResMut<ShipCam>,
         mut opened: ResMut<OpenCompartment>,
         mut rebuild: ResMut<MenuRebuild>| {
            cam.open = false;
            opened.0 = None;
            rebuild.0 = true;
        },
    );
}

/// Слой, который на осмотре корабля уезжает на `by` (или, если `inside`, наоборот выезжает).
#[derive(Component)]
struct Slide {
    by: Vec2,
    inside: bool,
}

fn slide(
    root: &mut ChildSpawnerCommands,
    by: Vec2,
    inside: bool,
    spawn: impl FnOnce(&mut ChildSpawnerCommands),
) {
    root.spawn((
        Slide { by, inside },
        Node {
            position_type: PositionType::Absolute,
            width: percent(100),
            height: percent(100),
            ..default()
        },
        UiTransform::default(),
        Pickable::IGNORE,
    ))
    .with_children(spawn);
}

/// Осмотр корабля: открыт ли, насколько приближен (0–1, плавно) и ход парусов.
#[derive(Resource, Default)]
pub(crate) struct ShipCam {
    pub open: bool,
    zoom: f32,
    /// Ход парусов (с): 0 — все свёрнуты; при старте игры раскатываются.
    sails: f32,
}

impl ShipCam {
    /// Приближение со сглаживанием: плавно трогается и плавно встаёт.
    fn eased(&self) -> f32 {
        let z = self.zoom;
        z * z * z * (z * (z * 6.0 - 15.0) + 10.0)
    }
}

/// Наружный облик корабля (гаснет на осмотре) и его картинки.
#[derive(Component)]
struct Exterior;

#[derive(Component)]
struct ExteriorImage;

/// Корабль-разрез: корпус, поверх — комнаты отсеков. Комната показывает уровень (свой рисунок
/// с нужного уровня), не построенная — заколочена, на стройке — леса и таймер. Тап — панель отсека.
fn ship_rooms(
    deck: &mut ChildSpawnerCommands,
    assets: &AssetServer,
    fonts: &Fonts,
    ship_data: &ShipData,
    ship: &Ship,
    selected: Option<&str>,
) {
    let view = ship.current(ship_data);
    let pct = |v: f32| percent(v / SHIP_ART * 100.0);
    deck.spawn((
        Node {
            position_type: PositionType::Absolute,
            width: percent(100),
            height: percent(100),
            ..default()
        },
        ImageNode::new(assets.load(view.hull())),
    ));
    let locked = assets.load(view.locked());
    for room in &view.rooms {
        // Жилые комнаты без отсека — просто часть рисунка.
        let Some(compartment) = ship_data.compartment(&room.id) else {
            continue;
        };
        let [x, y, w, h] = room.rect;
        let cramped = compartment.rank > view.rank;
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
                    ImageNode::new(assets.load(ship_data.scene.scaffold.clone())),
                    Pickable::IGNORE,
                ));
                c.spawn((room_tag(room.tag), Pickable::IGNORE))
                    .with_child((RoomTimer, text(fonts, "", true, 12.0, GOLD)));
            } else if level == 0 || cramped {
                // Не построен или тесно на этом корабле: та же комната с досками и замком.
                c.spawn((
                    fill(),
                    ImageNode {
                        rect: Some(Rect::new(x, y, x + w, y + h)),
                        ..ImageNode::new(locked.clone())
                    },
                    Pickable::IGNORE,
                ));
                let need = if cramped {
                    "Нет места".to_string()
                } else {
                    "Построить".to_string()
                };
                c.spawn((room_tag(room.tag), Pickable::IGNORE))
                    .with_child(crate::style::heading(fonts, need, 13.0, GOLD));
            } else {
                c.spawn((room_tag(room.tag), Pickable::IGNORE))
                    .with_child(crate::style::heading(
                        fonts,
                        format!("ур. {level}"),
                        12.0,
                        PARCHMENT,
                    ));
            }
        });
        let id = room.id.clone();
        pressable(&mut cell);
        cell.observe(
            move |_: On<PointerClick>,
                  cam: Res<ShipCam>,
                  mut opened: ResMut<OpenCompartment>,
                  mut board: ResMut<BoardOpen>,
                  mut rebuild: ResMut<MenuRebuild>| {
                if !cam.open {
                    return;
                }
                board.0 = false;
                opened.0 = Some(id.clone());
                rebuild.0 = true;
            },
        );
    }
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
    view: &crate::ship::Scene,
    front: bool,
) {
    for cloud in view.clouds.iter().filter(|c| c.front == front) {
        root.spawn((
            Cloud {
                speed: cloud.speed,
                phase: cloud.phase,
                width: cloud.width,
                alpha: cloud.alpha,
                front,
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
            backdrop(root, &assets, &data.scene);
            clouds(root, &assets, &data.scene, false);
        });
}

/// Панорама неба за кораблём (два куска встык едут влево) и стайка птиц.
fn backdrop(root: &mut ChildSpawnerCommands, assets: &AssetServer, view: &crate::ship::Scene) {
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
                // На 2 px шире шага: копии заходят друг на друга, без светлой нити на стыке.
                width: px(sky.width + 2.0),
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
    let Some(flock) = &data.scene.birds else {
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
    alpha: f32,
    front: bool,
}

fn drift_clouds(
    time: Res<Time>,
    cam: Res<ShipCam>,
    mut clouds: Query<(&Cloud, &mut UiTransform, &mut ImageNode)>,
) {
    let t = time.elapsed_secs();
    let fade = 1.0 - cam.eased();
    for (cloud, mut transform, mut image) in &mut clouds {
        // Передние облака на осмотре гаснут: закрывали бы комнаты.
        let alpha = if cloud.front {
            cloud.alpha * fade
        } else {
            cloud.alpha
        };
        image.color = Color::WHITE.with_alpha(alpha);
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
            (Value::Silver, "silver", 15.0),
            (Value::Planks, "planks", 16.0),
            (Value::Ether, "ether", 16.0),
            (Value::Doubloons, "doubloons", 16.0),
        ] {
            let mut slot =
                crate::style::resource_slot(bar, assets, fonts, kind, String::new(), size, value);
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
                if title == "Буря" {
                    pressable(&mut button);
                    button.observe(|_: On<PointerClick>, mut next: ResMut<NextState<Screen>>| {
                        next.set(Screen::Activities);
                    });
                } else if title == "Контракты" {
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
    // Первая плитка — Верфь; потом отсеки: открытые и полезные, «на будущее», нет места на корабле.
    let current = ship.current(data);
    let mut list: Vec<&Compartment> = data.compartments.iter().collect();
    list.sort_by_key(|c| (c.rank > current.rank, c.effect.is_none(), c.rank));
    let mut all: Vec<Option<&Compartment>> = vec![None];
    all.extend(list.into_iter().map(Some));
    // Не влезают все — пять плиток на страницу и шестая «Ещё».
    let per_page = if all.len() > COMPARTMENTS_SHOWN {
        COMPARTMENTS_SHOWN - 1
    } else {
        COMPARTMENTS_SHOWN
    };
    let pages = all.len().div_ceil(per_page);
    let page = page % pages.max(1);
    let shown: Vec<Option<&Compartment>> = all
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
        for entry in shown {
            let Some(compartment) = entry else {
                shipyard_tile(row, assets, fonts, current);
                continue;
            };
            let open = compartment.rank <= current.rank;
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
                    ("нет места".to_string(), GOLD)
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

/// Плитка верфи в ряду отсеков: свой корабль, тап — купить другой.
fn shipyard_tile(
    row: &mut ChildSpawnerCommands,
    assets: &AssetServer,
    fonts: &Fonts,
    current: &crate::ship::ShipDef,
) {
    let mut tile = row.spawn((
        Node {
            flex_grow: 1.0,
            flex_basis: px(0),
            padding: UiRect::top(px(4)),
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::Center,
            border: UiRect::all(px(1)),
            border_radius: BorderRadius::all(px(10)),
            ..default()
        },
        BackgroundColor(Color::srgba(0.16, 0.10, 0.06, 0.92)),
        BorderColor::all(Color::srgb(0.72, 0.55, 0.26)),
    ));
    tile.with_children(|t| {
        t.spawn((
            Node {
                width: px(64),
                height: px(48),
                ..default()
            },
            ImageNode::new(assets.load(current.exterior())),
            Pickable::IGNORE,
        ));
        t.spawn(crate::style::heading(fonts, "Верфь", 14.0, PARCHMENT));
        t.spawn(text(fonts, "корабли", false, 11.0, GOLD));
    });
    pressable(&mut tile);
    tile.observe(
        |_: On<PointerClick>,
         mut opened: ResMut<OpenCompartment>,
         mut board: ResMut<BoardOpen>,
         mut rebuild: ResMut<MenuRebuild>| {
            board.0 = false;
            opened.0 = Some(SHIPYARD.to_string());
            rebuild.0 = true;
        },
    );
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
                    move |_: On<PointerClick>, mut next: ResMut<NextState<Screen>>| {
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
    mut texts: Query<(&Value, &mut Text)>,
) {
    let cap = ship.capacity(&ship_data);
    for (value, mut text) in &mut texts {
        let shown = match value {
            Value::Silver => format!("{}/{}", profile.silver, cap.silver),
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
    cam: Res<ShipCam>,
    mut bubbles: Query<(&mut Visibility, &Children), With<EtherBubble>>,
    mut texts: Query<&mut Text>,
) {
    let ready = profile.ether_pending.floor() as u32;
    for (mut vis, children) in &mut bubbles {
        // На осмотре пузырь прячется: корабль крупный, пузырь ушёл бы за край.
        vis.set_if_neq(if ready > 0 && !cam.open {
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

/// Скатка паруса на рее и полоса полотна (номер паруса в данных, номер полосы сверху).
#[derive(Component)]
struct SailBundle(usize);

#[derive(Component)]
struct SailSlice {
    sail: usize,
    k: usize,
}

/// Насколько полоса паруса длиннее своей доли (заходит на соседнюю).
const SLICE_OVERLAP: f32 = 1.06;

fn smoothstep(a: f32, b: f32, x: f32) -> f32 {
    let s = ((x - a) / (b - a)).clamp(0.0, 1.0);
    s * s * (3.0 - 2.0 * s)
}

/// Паруса: на главной раскатываются (полотно падает с реи под тяжестью, вздрагивает, когда
/// дотянуло до низа, и надувается ветром), потом колышутся; на осмотре сворачиваются.
fn animate_sails(
    time: Res<Time>,
    data: Res<ShipData>,
    ship: Res<Ship>,
    mut cam: ResMut<ShipCam>,
    mut bundles: Query<(&SailBundle, &mut ImageNode), Without<SailSlice>>,
    mut slices: Query<(&SailSlice, &mut ImageNode, &mut UiTransform)>,
) {
    let Some(sails) = &ship.current(&data).sails else {
        return;
    };
    let full = sails.unfurl + sails.stagger * sails.layers.len().saturating_sub(1) as f32;
    let dt = time.delta_secs();
    cam.sails = if !cam.open && cam.zoom < 0.05 {
        cam.sails + dt
    } else {
        cam.sails - dt * sails.unfurl / sails.furl
    }
    .clamp(0.0, full);
    let fade = 1.0 - cam.eased();
    let open =
        |index: usize| ((cam.sails - index as f32 * sails.stagger) / sails.unfurl).clamp(0.0, 1.0);
    for (bundle, mut image) in &mut bundles {
        // Скатка распускается в полотно: гаснет в самом начале.
        let s = open(bundle.0);
        image.color = Color::WHITE.with_alpha((1.0 - smoothstep(0.0, 0.2, s)) * fade);
    }
    let t = time.elapsed_secs();
    let last = sails.slices.max(2) as f32 - 1.0;
    for (slice, mut image, mut transform) in &mut slices {
        let s = open(slice.sail);
        let k = slice.k as f32;
        // Падение: с ускорением вниз, у низа — короткий отскок.
        let drop = if s < 0.7 {
            (s / 0.7).powi(2)
        } else {
            let u = (s - 0.7) / 0.3;
            1.0 + 0.05 * (std::f32::consts::PI * u).sin() * (1.0 - u)
        };
        // Ветер наполняет полотно уже упавшим: оно было провисшим и чуть уже, раздувается.
        let fill = smoothstep(0.6, 1.0, s);
        let depth = k / last;
        let phase = slice.sail as f32 * 1.3;
        let wave = sails.ripple * depth.powf(1.3) * (t * 2.1 - k * 0.5 + phase).sin() * fill;
        // Полотно целиком тянется вниз от реи: полоса сжата к рее во столько же раз.
        let gather = (1.0 - drop) * (k + SLICE_OVERLAP / 2.0) / SLICE_OVERLAP;
        transform.translation = Val2::percent(wave * 100.0, -gather * 100.0);
        let breathe = 0.012 * depth * (t * 1.4 - k * 0.35 + phase).sin() * fill;
        let pop = 0.04 * (std::f32::consts::PI * fill).sin();
        transform.scale = Vec2::new(0.92 + 0.08 * fill + pop + breathe, drop.max(0.01));
        image.color = Color::WHITE.with_alpha(smoothstep(0.0, 0.1, s) * fade);
    }
}

/// Корабль на экране: снаружи — на главной, на осмотре — весь разрез по ширине экрана. Между ними
/// плавный наезд; наружный облик гаснет, кнопки главной разъезжаются. Плюс качка на ветру.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn frame_ship(
    time: Res<Time>,
    data: Res<ShipData>,
    ship: Res<Ship>,
    mut cam: ResMut<ShipCam>,
    mut ships: Query<(&mut Node, &mut UiTransform), With<ShipSprite>>,
    mut exterior: Query<&mut Visibility, (With<Exterior>, Without<Slide>)>,
    mut images: Query<&mut ImageNode, With<ExteriorImage>>,
    mut slides: Query<(&Slide, &mut UiTransform, &mut Visibility), Without<ShipSprite>>,
) {
    let def = ship.current(&data);
    let look = &data.scene.look;
    let step = time.delta_secs() / look.seconds;
    cam.zoom = if cam.open {
        (cam.zoom + step).min(1.0)
    } else {
        (cam.zoom - step).max(0.0)
    };
    let e = cam.eased();
    let (outside, inside) = (look.outside(def), look.inside(def));
    let side = outside.side.lerp(inside.side, e);
    let focus = outside.focus.lerp(inside.focus, e);
    let center = outside.center.lerp(inside.center, e);
    let corner = center - focus * side / SHIP_ART;
    let bob = time.elapsed_secs() / BOB_PERIOD * std::f32::consts::TAU;
    // На осмотре качка слабее: комнаты не должны плавать под пальцем.
    let sway = 1.0 - 0.75 * e;
    for (mut node, mut transform) in &mut ships {
        node.left = px(corner.x);
        node.top = px(corner.y);
        node.width = px(side);
        node.height = px(side);
        transform.translation = Val2::px(0.0, BOB_PX * bob.sin() * sway);
        // Нос чуть клюёт вслед за качкой.
        transform.rotation = Rot2::degrees(BOB_TILT_DEG * bob.cos() * sway);
    }
    let shown = |hidden: bool| {
        if hidden {
            Visibility::Hidden
        } else {
            Visibility::Inherited
        }
    };
    for mut vis in &mut exterior {
        vis.set_if_neq(shown(e > 0.98));
    }
    for mut image in &mut images {
        image.color = Color::WHITE.with_alpha(1.0 - e);
    }
    for (slide, mut transform, mut vis) in &mut slides {
        let away = if slide.inside { 1.0 - e } else { e };
        transform.translation = Val2::px(slide.by.x * away, slide.by.y * away);
        vis.set_if_neq(shown(away > 0.98));
    }
}

/// Рисунки корабля (1024 px) на главной сжаты втрое и качаются: без мип-уровней край рябит
/// и мерцает. Bevy их сам не строит — строим при загрузке картинок из `ship/`.
fn ship_mips(
    mut events: MessageReader<AssetEvent<Image>>,
    mut images: ResMut<Assets<Image>>,
    server: Res<AssetServer>,
) {
    for event in events.read() {
        let AssetEvent::LoadedWithDependencies { id } = event else {
            continue;
        };
        let ours = server
            .get_path(*id)
            .is_some_and(|p| p.path().starts_with("ship") || p.path().starts_with("ships"));
        if !ours {
            continue;
        }
        let Some(mut image) = images.get_mut(*id) else {
            continue;
        };
        add_mips(&mut image);
    }
}

fn add_mips(image: &mut Image) {
    use bevy::image::ImageSampler;
    use bevy::render::render_resource::TextureFormat;
    let desc = &image.texture_descriptor;
    let srgb = match desc.format {
        TextureFormat::Rgba8UnormSrgb => true,
        TextureFormat::Rgba8Unorm => false,
        _ => return,
    };
    if desc.mip_level_count > 1 || desc.size.depth_or_array_layers > 1 {
        return;
    }
    let Some(base) = image.data.as_ref() else {
        return;
    };
    let (mut w, mut h) = (desc.size.width as usize, desc.size.height as usize);
    // Усреднение — в линейном свете и с весом по альфе: прозрачный фон не красит край.
    let to_lin = |c: u8| {
        let c = c as f32 / 255.0;
        if srgb { c.powf(2.2) } else { c }
    };
    let to_byte = |c: f32| {
        let c = if srgb { c.powf(1.0 / 2.2) } else { c };
        (c * 255.0).round().clamp(0.0, 255.0) as u8
    };
    let mut level: Vec<[f32; 4]> = base
        .as_chunks::<4>()
        .0
        .iter()
        .map(|p| {
            let a = p[3] as f32 / 255.0;
            [to_lin(p[0]) * a, to_lin(p[1]) * a, to_lin(p[2]) * a, a]
        })
        .collect();
    let mut data = base.clone();
    let mut levels = 1;
    while w > 1 || h > 1 {
        let (nw, nh) = ((w / 2).max(1), (h / 2).max(1));
        let mut next = vec![[0.0f32; 4]; nw * nh];
        for y in 0..nh {
            for x in 0..nw {
                let mut sum = [0.0f32; 4];
                for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                    let p = level[(2 * y + dy).min(h - 1) * w + (2 * x + dx).min(w - 1)];
                    for c in 0..4 {
                        sum[c] += p[c] / 4.0;
                    }
                }
                next[y * nw + x] = sum;
            }
        }
        for p in &next {
            let a = p[3];
            let un = |c: f32| if a > 0.0 { c / a } else { 0.0 };
            data.extend([to_byte(un(p[0])), to_byte(un(p[1])), to_byte(un(p[2]))]);
            data.push((a * 255.0).round() as u8);
        }
        (level, w, h) = (next, nw, nh);
        levels += 1;
    }
    image.data = Some(data);
    image.texture_descriptor.mip_level_count = levels;
    image.sampler = ImageSampler::linear();
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

/// H — следующий корабль верфи без покупки (по кругу).
#[cfg(feature = "dev")]
fn dev_hull_key(
    keys: Res<ButtonInput<KeyCode>>,
    data: Res<ShipData>,
    mut ship: ResMut<Ship>,
    mut rebuild: ResMut<MenuRebuild>,
) {
    if keys.just_pressed(KeyCode::KeyH) {
        ship.dev_next_ship(&data);
        rebuild.0 = true;
    }
}
