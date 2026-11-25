//! Главное меню — палуба «Ласточки» у родного острова.
//!
//! Корабль в центре, ресурсы сверху, отсеки и кнопка «В поход» снизу. Прогресс по этапам и
//! ресурсы пока живут только в памяти: сохранения и экономика на сервере будут позже.

use battle_core::Outcome;
use bevy::image::{ImageAddressMode, ImageLoaderSettings, ImageSampler, ImageSamplerDescriptor};
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
use crate::summary::{Summary, Tone};
use crate::view::Fonts;

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
/// Винты за кормой: на рисунке анфас, в игре сплющены по ширине (вид сбоку) и крутятся.
const PROP_SQUASH: f32 = 0.45;
const PROP_TURNS: f32 = 1.6;

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
    /// Лучшая сила отряда за всё время — для таблицы капитанов.
    #[serde(default)]
    pub best_power: u32,
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
            best_power: 0,
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

/// Всплывающая подсказка. Живёт на том экране, где появилась: при переходе на другой пропадает.
#[derive(Resource, Default)]
pub(crate) struct Toast {
    text: String,
    left: f32,
    /// Иконка ресурса слева (`ui/icon_<имя>.png`), если подсказка про ресурс.
    icon: Option<&'static str>,
    /// Экран, где подсказка появилась; `None` — ещё не показана.
    on: Option<Screen>,
}

impl Toast {
    pub(crate) fn show(&mut self, text: &str) {
        self.set(text, None, TOAST_SECONDS);
    }

    /// Подсказка про ресурс: его иконка слева и короткий текст.
    pub(crate) fn show_icon(&mut self, icon: &'static str, text: &str) {
        self.set(text, Some(icon), TOAST_SECONDS);
    }

    /// Длинная подсказка — например, отчёт «пока вас не было».
    pub(crate) fn show_long(&mut self, text: &str) {
        self.set(text, None, TOAST_LONG_SECONDS);
    }

    fn set(&mut self, text: &str, icon: Option<&'static str>, seconds: f32) {
        self.text = text.to_string();
        self.icon = icon;
        self.left = seconds;
        self.on = None;
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
    Supplies,
    Doubloons,
}

#[derive(Component)]
struct EtherBubble;

#[derive(Component)]
struct ToastNode;

#[derive(Component)]
struct ToastIcon;

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
            (drift_clouds, scroll_backdrop, scroll_sky).run_if(not(in_state(Screen::Menu))),
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
                    scroll_sky,
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
    event_data: Res<'w, EventData>,
    settings: (
        Res<'w, crate::settings::Settings>,
        Res<'w, crate::settings::SettingsOpen>,
    ),
    ladder: (
        Res<'w, crate::ladder::LadderData>,
        Res<'w, crate::ladder::LadderOpen>,
        Res<'w, crate::arena::ArenaData>,
        Res<'w, crate::arena::Arena>,
    ),
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
                side_buttons(
                    r,
                    fonts,
                    view.event_data.current_short(),
                    view.contracts.claimable(&view.contract_data),
                );
                crate::ladder::button(r, assets, fonts);
            });
            slide(root, Vec2::new(0.0, 200.0), false, |r| {
                voyage(r, fonts, next, done)
            });
            // Улучшения отсеков — только на осмотре: выезжают снизу на место «В поход».
            slide(root, Vec2::new(0.0, 200.0), true, |r| {
                compartments(r, assets, fonts, ship_data, ship, view.page.0);
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
            if view.settings.1.0 {
                crate::settings::panel(root, fonts, &view.settings.0);
            } else if let Some(tab) = view.ladder.1.0 {
                let (data, _, arena_data, arena) = &view.ladder;
                let rows = match tab {
                    crate::ladder::Tab::Arena => {
                        let mut a = (**arena).clone();
                        a.roll(arena_data, crate::cabins::today());
                        let week = 7.0 * 86_400.0;
                        let progress = 1.0 - crate::events::week_left() as f32 / week;
                        data.arena(a.week, progress, a.rating)
                    }
                    crate::ladder::Tab::Power => data.power(view.profile.best_power),
                };
                crate::ladder::panel(root, assets, fonts, tab, &rows, data.shown);
            } else if view.board_open.0 {
                crate::contracts::board(root, assets, fonts, &view.contract_data, &view.contracts);
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
        // Один и тот же корабль на главной и на осмотре: снаружи борт закрыт, на осмотре
        // корабль приближается и борт растворяется — видны комнаты.
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
        .observe(open_ship);
        propellers(deck, assets, view);
        // Ветроловы накопили эфир — пузырь над кораблём, тап собирает.
        let [x, y] = view.bubble;
        ether_bubble(deck, fonts, pct(x), pct(y));
        // Камбуз наварил припасов — пузырь рядом с эфиром.
        crate::supplies::galley_bubble(deck, fonts, pct(x), pct(y + 9.0));
    });
}

/// Ячейки комнат поверх разреза: на главной скрыты.
#[derive(Component)]
struct ShipRooms;

/// Тап по кораблю на главной — к осмотру.
fn open_ship(
    _: On<PointerClick>,
    mut cam: ResMut<ShipCam>,
    mut sfx: MessageWriter<Sfx>,
    mut rebuild: ResMut<MenuRebuild>,
) {
    if cam.open {
        return;
    }
    cam.open = true;
    sfx.write(Sfx("click"));
    rebuild.0 = true;
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

/// Винты за кормой — поверх корпуса: сплющены по ширине (вид сбоку) и крутятся всегда.
fn propellers(ship: &mut ChildSpawnerCommands, assets: &AssetServer, view: &crate::ship::ShipDef) {
    let pct = |v: f32| percent(v / SHIP_ART * 100.0);
    for prop in &view.propellers {
        let [x, y] = prop.at;
        ship.spawn((
            Node {
                position_type: PositionType::Absolute,
                left: pct(x - prop.size / 2.0),
                top: pct(y - prop.size / 2.0),
                width: pct(prop.size),
                height: pct(prop.size),
                ..default()
            },
            UiTransform::from_scale(Vec2::new(PROP_SQUASH, 1.0)),
            Pickable::IGNORE,
        ))
        .with_child((
            Spinner(PROP_TURNS),
            Node {
                width: percent(100),
                height: percent(100),
                ..default()
            },
            ImageNode::new(assets.load("fx/propeller.png")),
            UiTransform::default(),
            Pickable::IGNORE,
        ));
    }
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
    // Ячейки комнат (подписи, рамки) — только когда борт открыт: снаружи подпись у края
    // комнаты вылезла бы за обшивку.
    deck.spawn((
        ShipRooms,
        Node {
            position_type: PositionType::Absolute,
            width: percent(100),
            height: percent(100),
            ..default()
        },
        Pickable::IGNORE,
    ))
    .with_children(|deck| {
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
                    c.spawn((room_tag(room.tag), Pickable::IGNORE)).with_child(
                        crate::style::heading(fonts, format!("ур. {level}"), 12.0, PARCHMENT),
                    );
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
    // Небо — один неподвижный узел на весь экран: едет окно в текстуре, а текстура повторяется
    // по ширине. Краёв картинки на экране нет, поэтому нет и шва: кольцо замыкает сам сэмплер.
    let image = assets
        .load_builder()
        .with_settings(|s: &mut ImageLoaderSettings| {
            s.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
                address_mode_u: ImageAddressMode::Repeat,
                ..ImageSamplerDescriptor::linear()
            });
        })
        .load(sky.image.clone());
    root.spawn((
        SkyScroll {
            width: sky.width,
            speed: sky.speed,
        },
        Node {
            position_type: PositionType::Absolute,
            left: px(0),
            top: px(0),
            width: percent(100),
            height: percent(100),
            ..default()
        },
        ImageNode {
            rect: Some(Rect::new(0.0, 0.0, 1.0, 1.0)),
            ..ImageNode::new(image)
        },
        Pickable::IGNORE,
    ));
    // Водопад и мельницы — в двух прозрачных слоях шириной в панораму: едут вместе с небом.
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
            UiTransform::default(),
            Pickable::IGNORE,
        ))
        .with_children(|pano| {
            // Водопад и мельницы — дети копии панорамы: едут вместе с ней.
            if let Some(fall) = &sky.waterfall {
                let [x, y, w, h] = fall.rect;
                pano.spawn((
                    FallingWater,
                    Node {
                        position_type: PositionType::Absolute,
                        left: px(x),
                        top: px(y),
                        width: px(w),
                        height: px(h),
                        ..default()
                    },
                    ImageNode {
                        rect: Some(Rect::new(0.0, 0.0, w, h)),
                        ..ImageNode::new(assets.load(fall.image.clone()))
                    },
                    Pickable::IGNORE,
                ));
            }
            for mill in &sky.mills {
                pano.spawn((
                    MillSails(mill.turns_per_second),
                    Node {
                        position_type: PositionType::Absolute,
                        left: px(mill.at[0] - mill.size / 2.0),
                        top: px(mill.at[1] - mill.size / 2.0),
                        width: px(mill.size),
                        height: px(mill.size),
                        ..default()
                    },
                    ImageNode::new(assets.load(mill.image.clone())),
                    UiTransform::default(),
                    Pickable::IGNORE,
                ));
            }
        });
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

/// Небо: окно в повторяющейся текстуре панорамы (px текстуры = логический px).
#[derive(Component)]
struct SkyScroll {
    width: f32,
    speed: f32,
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

/// Водопад на панораме: кадр берётся из листа по часам.
#[derive(Component)]
struct FallingWater;

/// Крылья мельницы на панораме, оборотов в секунду.
#[derive(Component)]
struct MillSails(f32);

/// Фон едет от часов: после пересборки меню не прыгает.
/// Небо едет: окно в текстуре размером с экран (логические px; высота панорамы равна высоте
/// экрана). Размер берём у окна, а не у разметки узла: в кадр пересборки она ещё не посчитана,
/// и небо мигнуло бы синим фоном.
fn scroll_sky(
    time: Res<Time>,
    window: Single<&Window>,
    mut skies: Query<(&SkyScroll, &mut ImageNode)>,
) {
    let t = time.elapsed_secs();
    let (w, h) = (window.width(), window.height());
    for (sky, mut image) in &mut skies {
        let x = (t * sky.speed).rem_euclid(sky.width);
        image.rect = Some(Rect::new(x, 0.0, x + w, h));
    }
}

#[allow(clippy::type_complexity)]
fn scroll_backdrop(
    time: Res<Time>,
    data: Res<ShipData>,
    mut skies: Query<(&Backdrop, &mut UiTransform), (Without<Bird>, Without<MillSails>)>,
    mut birds: Query<(&Bird, &mut UiTransform), (Without<Backdrop>, Without<MillSails>)>,
    mut mills: Query<(&MillSails, &mut UiTransform), (Without<Backdrop>, Without<Bird>)>,
    mut water: Query<&mut ImageNode, With<FallingWater>>,
) {
    let t = time.elapsed_secs();
    for (sky, mut transform) in &mut skies {
        let x = -(t * sky.speed).rem_euclid(sky.width) + sky.copy * sky.width;
        transform.translation = Val2::px(x, 0.0);
    }
    for (mill, mut transform) in &mut mills {
        transform.rotation = Rot2::radians(-t * mill.0 * std::f32::consts::TAU);
    }
    if let Some(fall) = &data.scene.backdrop.waterfall {
        let frame = (t * fall.fps) as u32 % fall.frames;
        let [_, _, w, h] = fall.rect;
        let corner = Vec2::new(
            (frame % fall.columns) as f32 * w,
            (frame / fall.columns) as f32 * h,
        );
        for mut image in &mut water {
            image.rect = Some(Rect::from_corners(corner, corner + Vec2::new(w, h)));
        }
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
            left: px(50),
            right: px(50),
            padding: UiRect::axes(px(18), px(12)),
            column_gap: px(12),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            border: UiRect::all(px(2)),
            border_radius: BorderRadius::all(px(14)),
            ..default()
        },
        BackgroundColor(Color::srgba(0.13, 0.09, 0.06, 0.96)),
        BorderColor::all(BRASS),
        BoxShadow::new(
            Color::srgba(0.0, 0.0, 0.0, 0.55),
            px(0),
            px(4),
            px(0),
            px(10),
        ),
        Visibility::Hidden,
        GlobalZIndex(20),
        Pickable::IGNORE,
    ))
    .with_children(|t| {
        t.spawn((
            ToastIcon,
            Node {
                width: px(40),
                height: px(40),
                flex_shrink: 0.0,
                display: Display::None,
                ..default()
            },
            ImageNode::default(),
        ));
        t.spawn((
            crate::style::heading(fonts, "", 17.0, PARCHMENT),
            TextLayout::justify(Justify::Center),
        ));
    });
}

/// Полоса ресурсов главной — как на остальных экранах, плюс уровень капитана, вместимость
/// трюма у серебра и «+» у дублонов. Числа обновляет `refresh_values`.
fn top_bar(
    root: &mut ChildSpawnerCommands,
    assets: &AssetServer,
    fonts: &Fonts,
    profile: &Profile,
) {
    // Пять ресурсов, уровень и шестерёнка — плашки плотнее, чем на других экранах.
    let mut top = crate::style::bar(root);
    top.entry::<Node>().and_modify(|mut node| {
        node.column_gap = px(5);
        node.padding = UiRect::new(px(8), px(14), px(7), px(7));
    });
    top.with_children(|bar| {
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
            (Value::Planks, "planks", 15.0),
            (Value::Ether, "ether", 15.0),
            (Value::Supplies, "supplies", 15.0),
            (Value::Doubloons, "doubloons", 15.0),
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
                          supply: Res<crate::supplies::SupplyData>,
                          mut toast: ResMut<Toast>| {
                        let cap = ship.capacity(&ship_data);
                        let (icon, n, max) = match value {
                            Value::Silver => ("silver", profile.silver, cap.silver),
                            Value::Planks => ("planks", profile.planks, cap.planks),
                            Value::Supplies => {
                                toast.show_icon(
                                    "supplies",
                                    &format!(
                                        "{} · сами до {}, Камбуз — сверх",
                                        profile.supplies, supply.cap
                                    ),
                                );
                                return;
                            }
                            _ => ("ether", profile.ether, cap.ether),
                        };
                        toast.show_icon(icon, &format!("{n} / {max} · вместимость Трюма"));
                    },
                );
            }
            if matches!(value, Value::Doubloons) {
                // Чуть шире остальных: справа «+», число не должно уходить под него.
                slot.entry::<Node>()
                    .and_modify(|mut node| node.flex_grow = 1.4);
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
        crate::settings::button(bar, assets);
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
            top: px(762),
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
            padding: UiRect::new(px(16), px(10), px(7), px(7)),
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
                row_gap: px(4),
                ..default()
            })
            .with_children(|info| {
                info.spawn(Node::default()).with_children(|row| {
                    let stage = if done {
                        "всё пройдено · босс".to_string()
                    } else {
                        stage_title(&next.id)
                    };
                    crate::style::pill(row, fonts, stage, Color::srgb(0.24, 0.2, 0.3));
                });
                info.spawn(crate::style::heading(
                    fonts,
                    next.name.clone(),
                    23.0,
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
            // Припасы на потолке не красим: сверх него их варит Камбуз — это хорошо.
            Value::Supplies => (profile.supplies, u32::MAX),
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
    state: Res<State<Screen>>,
    assets: Res<AssetServer>,
    mut toast: ResMut<Toast>,
    mut nodes: Query<(&mut Visibility, &Children), With<ToastNode>>,
    mut icons: Query<(&mut Node, &mut ImageNode), With<ToastIcon>>,
    mut texts: Query<&mut Text>,
) {
    // Подсказка принадлежит экрану, где появилась: ушли с него — она не догоняет.
    let here = *state.get();
    match toast.on {
        None if toast.left > 0.0 => toast.on = Some(here),
        Some(on) if on != here => {
            toast.left = 0.0;
            toast.on = None;
        }
        _ => {}
    }
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
            if let Ok((mut node, mut image)) = icons.get_mut(*child) {
                let display = if toast.icon.is_some() {
                    Display::Flex
                } else {
                    Display::None
                };
                if node.display != display {
                    node.display = display;
                }
                if let Some(icon) = toast.icon {
                    let handle = assets.load(format!("ui/icon_{icon}.png"));
                    if image.image != handle {
                        image.image = handle;
                    }
                }
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
    mut rooms: Query<&mut Visibility, (With<ShipRooms>, Without<Exterior>, Without<Slide>)>,
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
    // Борт открывается, когда корабль уже почти приблизился.
    let open = smoothstep(0.55, 1.0, e);
    for mut vis in &mut rooms {
        vis.set_if_neq(shown(open < 0.01));
    }
    for mut image in &mut images {
        image.color = Color::WHITE.with_alpha(1.0 - open);
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

/// Здоровье героев после боя (доля от максимума, 0 — пал): Блокада и Руины несут его дальше.
/// Герои, которые пали ещё раньше и в бой не шли, остаются павшими.
fn survivors(screen: &BattleScreen) -> Vec<(String, f32)> {
    let allies = screen
        .battle
        .units()
        .iter()
        .filter(|u| u.side == battle_core::Side::Ally);
    let mut hp: Vec<(String, f32)> = screen
        .team
        .iter()
        .zip(allies)
        .map(|(m, u)| (m.id.clone(), u.hp as f32 / u.def.stats.max_hp as f32))
        .collect();
    if let Some(fight) = &screen.custom {
        for (id, f) in &fight.ally_hp {
            if *f <= 0.0 && !hp.iter().any(|(h, _)| h == id) {
                hp.push((id.clone(), 0.0));
            }
        }
    }
    hp
}

/// Итог одной победы на этапе главы.
pub(crate) struct StageWin {
    pub reward: Reward,
    /// Процент добычи (повторы за день приносят меньше).
    pub percent: u32,
    /// Сколько припасов ушло.
    pub cost: u32,
    /// Побед на этапе сегодня до этой.
    pub wins: u32,
}

/// Победа на этапе главы — в бою или рейдом: припасы, убывающая добыча, счётчик побед за день.
pub(crate) fn stage_win(
    stage: usize,
    first: bool,
    data: &MenuData,
    progress: &mut Progress,
    spoils: &mut Spoils,
    supply: &crate::supplies::SupplyData,
) -> StageWin {
    // Повторные победы за день приносят меньше (supplies.rs); первая — всегда целиком.
    let today = crate::cabins::today();
    let wins = progress.wins_today(stage, today);
    let percent = supply.percent(wins).unwrap_or(0);
    let cost = supply.cost_for(first);
    progress.add_win(stage, today);
    spoils.profile.supplies = spoils.profile.supplies.saturating_sub(cost);
    let cut = |n: u32| n * percent / 100;
    let r = &data.victory_reward;
    let reward = Reward {
        silver: cut(r.silver),
        planks: cut(r.planks),
        journals: cut(r.journals),
    };
    spoils.give(reward.silver, reward.planks, reward.journals);
    StageWin {
        reward,
        percent,
        cost,
        wins,
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
    mut result: ResMut<crate::summary::ResultSummary>,
    mut deeds: MessageWriter<Deed>,
    abyss: (Res<AbyssData>, ResMut<Abyss>),
    events: (Res<EventData>, ResMut<StormEvent>, ResMut<WeekEvents>),
    supply: Res<crate::supplies::SupplyData>,
    ruins: (Res<crate::ruins::RuinsData>, ResMut<crate::ruins::Ruins>),
    arena: (Res<crate::arena::ArenaData>, ResMut<crate::arena::Arena>),
) {
    let (abyss_data, mut abyss) = abyss;
    let (ruins_data, mut ruins) = ruins;
    let (arena_data, mut arena) = arena;
    let (event_data, mut storm, mut week_events) = events;
    let Some(outcome) = screen.shown_outcome else {
        return;
    };
    let battle = (screen.stage, screen.seed);
    if *counted == Some(battle) {
        return;
    }
    *counted = Some(battle);
    let summary = match (outcome, &screen.custom) {
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
            week_events.blockade_win(
                wave,
                survivors(&screen),
                &event_data.blockade,
                &mut spoils.profile,
            )
        }
        (outcome, Some(fight)) if matches!(fight.kind, FightKind::Ruins(..)) => {
            let FightKind::Ruins(floor, room) = fight.kind else {
                unreachable!()
            };
            if outcome == Outcome::Victory {
                deeds.write(Deed("win"));
                ruins.win(
                    floor,
                    room,
                    survivors(&screen),
                    &ruins_data,
                    &mut spoils.profile,
                )
            } else {
                ruins.lose(floor)
            }
        }
        (outcome, Some(fight)) if matches!(fight.kind, FightKind::Arena(_)) => {
            let FightKind::Arena(tier) = fight.kind else {
                unreachable!()
            };
            let win = outcome == Outcome::Victory;
            if win {
                deeds.write(Deed("win"));
            }
            arena.finish(
                win,
                tier,
                &arena_data,
                crate::cabins::today(),
                &mut spoils.profile,
            )
        }
        (Outcome::Defeat, _) => Summary::new("Усильте отряд или смените тактику"),
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
                Summary::new(format!("Этаж {floor} пройден!"))
                    .gain("silver", r.silver as i64)
                    .gain("journals", r.journals as i64)
                    .gain("details", r.details as i64)
                    .gain("charts", r.charts as i64)
                    .gain("doubloons", r.doubloons as i64)
                    .gain("figureheads", r.figureheads as i64)
            } else {
                Summary::new("Этаж уже пройден").chip("награда выдаётся один раз", Tone::Info)
            }
        }
        (Outcome::Victory, Some(fight))
            if fight.kind == FightKind::Convoy && convoys.ready == 0 =>
        {
            Summary::new("Конвоев в очереди нет — добычи нет")
                .chip("новое судно скоро появится", Tone::Info)
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
            let mut sum = Summary::new("Добыча взята!")
                .gain("silver", silver as i64)
                .gain("planks", planks as i64)
                .gain("journals", fight.journals as i64);
            if k > 1 {
                sum = sum.chip(format!("попутный ветер ×{k}"), Tone::Gold);
            }
            sum
        }
        (Outcome::Victory, None) => {
            let first = screen.stage == progress.cleared;
            let win = stage_win(
                screen.stage,
                first,
                &data,
                &mut progress,
                &mut spoils,
                &supply,
            );
            let (reward, percent, cost, wins) = (win.reward, win.percent, win.cost, win.wins);
            deeds.write(Deed("win"));
            if first {
                story.0.push(screen.data.stages[screen.stage].id.clone());
                progress.cleared += 1;
                spoils.profile.doubloons += data.first_clear_doubloons;
            }
            let stages = &screen.data.stages;
            let line = match stages.get(progress.cleared) {
                Some(stage) if first => format!("Открыт этап «{}»", stage.name),
                None if first => "Все главы пройдены!".to_string(),
                _ => String::new(),
            };
            let mut sum = Summary::new(line)
                .gain("silver", reward.silver as i64)
                .gain("planks", reward.planks as i64)
                .gain("journals", reward.journals as i64)
                .gain(
                    "doubloons",
                    if first {
                        data.first_clear_doubloons as i64
                    } else {
                        0
                    },
                )
                .gain("supplies", -(cost as i64));
            if percent < 100 {
                sum = sum.chip(format!("повтор: {percent}% добычи"), Tone::Info);
            }
            // Лимит касается повторов: после первой победы на этапе он только сбивает с толку.
            if !first {
                sum = sum.chip(
                    format!("побед сегодня {}/{}", wins + 1, supply.daily_limit()),
                    Tone::Info,
                );
            }
            sum
        }
    };
    result.set(summary);
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
