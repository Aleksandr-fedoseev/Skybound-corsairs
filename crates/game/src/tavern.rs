//! Таверна: найм героев за дублоны и бесплатный найм раз в день.
//!
//! Шансы и гарант показаны на экране. Новый герой приходит в команду, повтор — копией
//! для будущих звёзд. Пока всё считается на клиенте; в релизе крутки считает сервер.

use std::time::{SystemTime, UNIX_EPOCH};

use battle_core::Rng;
use bevy::prelude::*;
use game_data::{GameData, Rarity};
use serde::{Deserialize, Serialize};

use crate::Screen;
use crate::audio::Sfx;
use crate::battle_screen::{BattleScreen, random_seed};
use crate::crew::Roster;
use crate::menu::{GOLD, PARCHMENT, Profile, Toast, bottom_nav, pressable, spawn_toast, text};
use crate::ship::{Ship, ShipData};
use crate::style::{self, INK, INK_SOFT, Look, Skin};
use crate::view::Fonts;

const BRIGHT: Color = Color::srgb(1.0, 0.93, 0.78);
const SR_COLOR: Color = Color::srgb(0.70, 0.55, 1.0);
const R_COLOR: Color = Color::srgb(0.70, 0.74, 0.80);
/// Карточки найма открываются по одной.
const REVEAL_STEP: f32 = 0.18;
const REVEAL_POP: f32 = 0.25;
const SSR_POP_SCALE: f32 = 1.35;
const SECONDS_PER_DAY: u64 = 86_400;

/// Настройки найма: `data/tavern.json`.
#[derive(Resource, Deserialize)]
pub struct TavernData {
    pub single_price: u32,
    pub ten_price: u32,
    pub rates: Rates,
    /// SSR гарантирован не позже этого найма с прошлого SSR.
    pub ssr_pity: u32,
    /// В каждых `sr_every` наймах подряд хотя бы один SR или выше.
    pub sr_every: u32,
    /// Кто может выпасть. Внутри одной редкости все герои выпадают поровну.
    pub pool: Vec<String>,
}

/// Шансы на редкость; R — всё остальное.
#[derive(Deserialize)]
pub struct Rates {
    pub ssr: f32,
    pub sr: f32,
}

impl TavernData {
    pub fn bundled() -> Self {
        serde_json::from_str(include_str!("../../../data/tavern.json"))
            .expect("data/tavern.json корректен")
    }
}

/// Счётчики гаранта: сколько наймов прошло с последнего SSR и с последнего SR-или-выше.
#[derive(Resource, Default, Clone, Copy, Debug, Serialize, Deserialize)]
pub struct Pity {
    pub since_ssr: u32,
    pub since_sr: u32,
}

/// Один найм. `roll` решает редкость, `pick` — героя внутри редкости (оба в [0, 1)).
pub fn pull(
    cfg: &TavernData,
    pity: &mut Pity,
    roll: f32,
    pick: f32,
    pool: &[(String, Rarity)],
) -> (String, Rarity) {
    pity.since_ssr += 1;
    pity.since_sr += 1;
    let mut rarity = if roll < cfg.rates.ssr {
        Rarity::Ssr
    } else if roll < cfg.rates.ssr + cfg.rates.sr {
        Rarity::Sr
    } else {
        Rarity::R
    };
    if pity.since_ssr >= cfg.ssr_pity {
        rarity = Rarity::Ssr;
    } else if rarity == Rarity::R && pity.since_sr >= cfg.sr_every {
        rarity = Rarity::Sr;
    }
    match rarity {
        Rarity::Ssr => *pity = Pity::default(),
        Rarity::Sr => pity.since_sr = 0,
        Rarity::R => {}
    }
    let same: Vec<&(String, Rarity)> = pool.iter().filter(|(_, r)| *r == rarity).collect();
    let candidates: Vec<&(String, Rarity)> = if same.is_empty() {
        pool.iter().collect()
    } else {
        same
    };
    let index = ((pick * candidates.len() as f32) as usize).min(candidates.len() - 1);
    candidates[index].clone()
}

/// Состояние Таверны между заходами (гарант и бесплатные наймы сохраняются в файл).
#[derive(Resource, Clone, Serialize, Deserialize)]
pub struct TavernState {
    pity: Pity,
    #[serde(skip, default = "fresh_rng")]
    rng: Rng,
    /// День (по UTC) и сколько бесплатных наймов в этот день уже взято.
    free_used: (u64, u32),
}

fn fresh_rng() -> Rng {
    Rng::new(random_seed())
}

impl Default for TavernState {
    fn default() -> Self {
        Self {
            pity: Pity::default(),
            rng: Rng::new(random_seed()),
            free_used: (0, 0),
        }
    }
}

fn today() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() / SECONDS_PER_DAY)
}

impl TavernState {
    /// Сколько бесплатных наймов осталось сегодня при лимите `per_day`.
    fn free_left(&self, per_day: u32) -> u32 {
        let (day, used) = self.free_used;
        if day == today() {
            per_day.saturating_sub(used)
        } else {
            per_day
        }
    }

    fn use_free(&mut self) {
        let day = today();
        let used = if self.free_used.0 == day {
            self.free_used.1
        } else {
            0
        };
        self.free_used = (day, used + 1);
    }
}

/// Что выпало в последнем найме: герой, редкость, новый ли.
#[derive(Resource, Default)]
struct Results {
    list: Vec<(String, Rarity, bool)>,
    /// Сколько секунд прошло с начала показа.
    elapsed: f32,
}

#[derive(Resource, Default)]
struct TavernDirty(bool);

#[derive(Component)]
struct TavernRoot;

#[derive(Component)]
struct ResultsOverlay;

/// Карточка результата номер `.0`, её редкость SSR.
#[derive(Component)]
struct Reveal(usize, bool);

#[derive(Clone, Copy)]
enum Hire {
    Free,
    Single,
    Ten,
}

pub fn plugin(app: &mut App) {
    app.insert_resource(TavernData::bundled())
        .init_resource::<TavernState>()
        .init_resource::<Results>()
        .init_resource::<TavernDirty>()
        .add_systems(OnEnter(Screen::Tavern), spawn_tavern)
        .add_systems(
            Update,
            (rebuild_tavern, reveal_results).run_if(in_state(Screen::Tavern)),
        );
}

fn rarity_label(rarity: Rarity) -> &'static str {
    match rarity {
        Rarity::R => "R",
        Rarity::Sr => "SR",
        Rarity::Ssr => "SSR",
    }
}

fn rarity_color(rarity: Rarity) -> Color {
    match rarity {
        Rarity::R => R_COLOR,
        Rarity::Sr => SR_COLOR,
        Rarity::Ssr => GOLD,
    }
}

fn pool_of(cfg: &TavernData, data: &GameData) -> Vec<(String, Rarity)> {
    cfg.pool
        .iter()
        .filter_map(|id| data.hero(id).map(|h| (h.id.clone(), h.rarity)))
        .collect()
}

#[allow(clippy::too_many_arguments)]
fn spawn_tavern(
    mut commands: Commands,
    assets: Res<AssetServer>,
    fonts: Res<Fonts>,
    cfg: Res<TavernData>,
    state: Res<TavernState>,
    ship: Res<Ship>,
    ship_data: Res<ShipData>,
    profile: Res<Profile>,
    roster: Res<Roster>,
    screen: Res<BattleScreen>,
) {
    build(
        &mut commands,
        &assets,
        &fonts,
        &cfg,
        &state,
        ship.free_hires(&ship_data),
        &profile,
        &roster,
        &screen.data,
    );
}

#[allow(clippy::too_many_arguments)]
fn rebuild_tavern(
    mut commands: Commands,
    mut dirty: ResMut<TavernDirty>,
    roots: Query<Entity, With<TavernRoot>>,
    assets: Res<AssetServer>,
    fonts: Res<Fonts>,
    cfg: Res<TavernData>,
    state: Res<TavernState>,
    ship: Res<Ship>,
    ship_data: Res<ShipData>,
    profile: Res<Profile>,
    roster: Res<Roster>,
    screen: Res<BattleScreen>,
    results: Res<Results>,
) {
    if !dirty.0 {
        return;
    }
    dirty.0 = false;
    for root in &roots {
        commands.entity(root).despawn();
    }
    build(
        &mut commands,
        &assets,
        &fonts,
        &cfg,
        &state,
        ship.free_hires(&ship_data),
        &profile,
        &roster,
        &screen.data,
    );
    if !results.list.is_empty() {
        results_overlay(&mut commands, &assets, &fonts, &results, &screen.data);
    }
}

#[allow(clippy::too_many_arguments)]
fn build(
    commands: &mut Commands,
    assets: &AssetServer,
    fonts: &Fonts,
    cfg: &TavernData,
    state: &TavernState,
    free_per_day: u32,
    profile: &Profile,
    roster: &Roster,
    data: &GameData,
) {
    commands
        .spawn((
            TavernRoot,
            DespawnOnExit(Screen::Tavern),
            Node {
                width: percent(100),
                height: percent(100),
                ..default()
            },
            BackgroundColor(Color::srgb(0.10, 0.07, 0.05)),
        ))
        .with_children(|root| {
            style::backdrop(root, assets, "backgrounds/tavern.jpg", 0.15);
            style::top_bar(
                root,
                assets,
                fonts,
                &[("doubloons", profile.doubloons), ("silver", profile.silver)],
            );
            style::title(root, fonts, "Таверна · найм героев", 56.0);
            poster(root, assets, fonts, cfg, roster, data);
            pity_panel(root, fonts, cfg, state);
            hire_buttons(
                root,
                assets,
                fonts,
                cfg,
                state.free_left(free_per_day),
                profile,
            );
            pool_row(root, assets, fonts, cfg, roster, data);
            bottom_nav(root, assets, fonts, Screen::Tavern);
            spawn_toast(root, fonts);
        });
}

/// Плакат «Разыскивается»: SSR-герой из таверны, которого ещё нет в команде.
fn poster(
    root: &mut ChildSpawnerCommands,
    assets: &AssetServer,
    fonts: &Fonts,
    cfg: &TavernData,
    roster: &Roster,
    data: &GameData,
) {
    let ssr: Vec<_> = cfg
        .pool
        .iter()
        .filter_map(|id| data.hero(id))
        .filter(|h| h.rarity == Rarity::Ssr)
        .collect();
    let Some(hero) = ssr.iter().find(|h| !roster.owns(&h.id)).or(ssr.first()) else {
        return;
    };
    root.spawn(Node {
        position_type: PositionType::Absolute,
        top: px(106),
        left: px(0),
        right: px(0),
        justify_content: JustifyContent::Center,
        ..default()
    })
    .with_children(|row| {
        row.spawn((
            Node {
                width: px(300),
                height: px(352),
                padding: UiRect::all(px(20)),
                ..default()
            },
            Skin(Look::Wood),
        ))
        .with_children(|frame| {
            frame
                .spawn((
                    Node {
                        flex_grow: 1.0,
                        flex_direction: FlexDirection::Column,
                        align_items: AlignItems::Center,
                        padding: UiRect::top(px(8)),
                        overflow: Overflow::clip(),
                        ..default()
                    },
                    Skin(Look::Parchment),
                ))
                .with_children(|paper| {
                    paper.spawn(text(fonts, "РАЗЫСКИВАЕТСЯ", true, 26.0, INK));
                    paper.spawn((
                        Node {
                            height: px(228),
                            margin: UiRect::top(px(4)),
                            ..default()
                        },
                        ImageNode::new(assets.load(format!("ui/full_{}.png", hero.id))),
                        Pickable::IGNORE,
                    ));
                });
            // Лента с именем поверх низа плаката.
            frame
                .spawn((
                    Node {
                        position_type: PositionType::Absolute,
                        left: px(-10),
                        right: px(-10),
                        bottom: px(14),
                        height: px(52),
                        flex_direction: FlexDirection::Column,
                        justify_content: JustifyContent::Center,
                        align_items: AlignItems::Center,
                        ..default()
                    },
                    Skin(Look::Go),
                ))
                .with_children(|ribbon| {
                    ribbon.spawn(text(
                        fonts,
                        format!("SSR · {} — {}", hero.name, hero.title),
                        true,
                        15.0,
                        Color::WHITE,
                    ));
                    ribbon.spawn(text(
                        fonts,
                        if roster.owns(&hero.id) {
                            "уже в команде — копия даст звезду".to_string()
                        } else {
                            format!("шанс SSR {}%", (cfg.rates.ssr * 1000.0).round() / 10.0)
                        },
                        false,
                        12.0,
                        BRIGHT,
                    ));
                });
        });
    });
}

/// Кто сейчас в таверне: все, кто может выпасть.
fn pool_row(
    root: &mut ChildSpawnerCommands,
    assets: &AssetServer,
    fonts: &Fonts,
    cfg: &TavernData,
    roster: &Roster,
    data: &GameData,
) {
    root.spawn((
        Node {
            position_type: PositionType::Absolute,
            bottom: px(86),
            left: px(6),
            right: px(6),
            height: px(84),
            padding: UiRect::new(px(52), px(48), px(6), px(6)),
            justify_content: JustifyContent::SpaceBetween,
            align_items: AlignItems::Center,
            ..default()
        },
        Skin(Look::Plank),
    ))
    .with_children(|rail| {
        for id in &cfg.pool {
            let Some(hero) = data.hero(id) else {
                continue;
            };
            let owned = roster.owns(id);
            rail.spawn(Node {
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                ..default()
            })
            .with_children(|cell| {
                cell.spawn(style::medallion(assets, &hero.id, 50.0, false));
                cell.spawn(text(
                    fonts,
                    if owned {
                        rarity_label(hero.rarity).to_string()
                    } else {
                        "новый!".to_string()
                    },
                    true,
                    11.0,
                    if owned {
                        rarity_color(hero.rarity)
                    } else {
                        GOLD
                    },
                ));
            });
        }
    });
}

/// Опубликованные шансы и гарант.
fn pity_panel(
    root: &mut ChildSpawnerCommands,
    fonts: &Fonts,
    cfg: &TavernData,
    state: &TavernState,
) {
    let done = state.pity.since_ssr.min(cfg.ssr_pity);
    let pct = |p: f32| format!("{}%", (p * 1000.0).round() / 10.0);
    let odds = format!(
        "Шансы: SSR {} · SR {} · R {}. Гарант: SSR не позже {}-го найма, в каждых {} — SR или выше. Внутри редкости герои выпадают поровну; повтор — копия для звёзд.",
        pct(cfg.rates.ssr),
        pct(cfg.rates.sr),
        pct(1.0 - cfg.rates.ssr - cfg.rates.sr),
        cfg.ssr_pity,
        cfg.sr_every
    );
    root.spawn((
        Node {
            position_type: PositionType::Absolute,
            top: px(468),
            left: px(40),
            right: px(40),
            padding: UiRect::axes(px(22), px(12)),
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::Center,
            row_gap: px(6),
            ..default()
        },
        Skin(Look::Parchment),
    ))
    .with_children(|panel| {
        panel.spawn(text(
            fonts,
            format!("До гарантированного SSR: {done} / {}", cfg.ssr_pity),
            true,
            14.0,
            INK,
        ));
        panel
            .spawn((
                Node {
                    width: percent(100),
                    height: px(12),
                    border: UiRect::all(px(2)),
                    border_radius: BorderRadius::all(px(6)),
                    ..default()
                },
                BackgroundColor(Color::srgb(0.22, 0.14, 0.08)),
                BorderColor::all(Color::srgb(0.72, 0.55, 0.26)),
            ))
            .with_child((
                Node {
                    width: percent(100.0 * done as f32 / cfg.ssr_pity.max(1) as f32),
                    height: percent(100),
                    border_radius: BorderRadius::all(px(4)),
                    ..default()
                },
                BackgroundColor(Color::srgb(0.95, 0.66, 0.22)),
            ));
        let mut rates = panel.spawn((
            Node {
                min_height: px(28),
                padding: UiRect::axes(px(14), px(3)),
                align_items: AlignItems::Center,
                ..default()
            },
            Skin(Look::TabOff),
        ));
        rates.with_child(text(fonts, "Шансы и гарант", true, 12.0, BRIGHT));
        pressable(&mut rates);
        rates.observe(move |_: On<PointerClick>, mut toast: ResMut<Toast>| {
            toast.show_long(&odds);
        });
    });
}

fn hire_buttons(
    root: &mut ChildSpawnerCommands,
    assets: &AssetServer,
    fonts: &Fonts,
    cfg: &TavernData,
    free_left: u32,
    profile: &Profile,
) {
    root.spawn(Node {
        position_type: PositionType::Absolute,
        top: px(590),
        left: px(16),
        right: px(16),
        flex_direction: FlexDirection::Column,
        row_gap: px(8),
        ..default()
    })
    .with_children(|col| {
        let free = free_left > 0;
        hire_button(
            col,
            assets,
            fonts,
            Hire::Free,
            if free {
                "Бесплатный найм"
            } else {
                "Бесплатный найм — завтра"
            },
            None,
            format!("осталось сегодня: {free_left}"),
            if free { Look::Dark } else { Look::Off },
            52.0,
        );
        col.spawn(Node {
            column_gap: px(12),
            ..default()
        })
        .with_children(|row| {
            for (hire, title, price) in [
                (Hire::Single, "Нанять ×1", cfg.single_price),
                (Hire::Ten, "Нанять ×10", cfg.ten_price),
            ] {
                hire_button(
                    row,
                    assets,
                    fonts,
                    hire,
                    title,
                    Some(price),
                    String::new(),
                    if profile.doubloons >= price {
                        Look::Go
                    } else {
                        Look::Off
                    },
                    72.0,
                );
            }
        });
    });
}

#[allow(clippy::too_many_arguments)]
fn hire_button(
    parent: &mut ChildSpawnerCommands,
    assets: &AssetServer,
    fonts: &Fonts,
    hire: Hire,
    title: &str,
    price: Option<u32>,
    sub: String,
    look: Look,
    height: f32,
) {
    let mut button = parent.spawn((
        Node {
            flex_grow: 1.0,
            flex_basis: px(0),
            height: px(height),
            flex_direction: FlexDirection::Column,
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            ..default()
        },
        Skin(look),
    ));
    button.with_children(|b| {
        b.spawn(text(
            fonts,
            title,
            true,
            if price.is_some() { 21.0 } else { 15.0 },
            Color::WHITE,
        ));
        match price {
            Some(price) => style::costs(b, assets, fonts, &[("doubloons", price)], 15.0, BRIGHT),
            None => {
                b.spawn(text(fonts, sub, false, 11.0, PARCHMENT));
            }
        }
    });
    pressable(&mut button);
    button.observe(move |_: On<PointerClick>, mut commands: Commands| {
        commands.run_system_cached_with(hire_heroes, hire);
    });
}

/// Нанять: проверить цену, вытянуть героев, записать в команду, показать карточки.
#[allow(clippy::too_many_arguments)]
fn hire_heroes(
    In(hire): In<Hire>,
    cfg: Res<TavernData>,
    screen: Res<BattleScreen>,
    mut state: ResMut<TavernState>,
    ship: Res<Ship>,
    ship_data: Res<ShipData>,
    mut profile: ResMut<Profile>,
    mut roster: ResMut<Roster>,
    mut results: ResMut<Results>,
    mut toast: ResMut<Toast>,
    mut dirty: ResMut<TavernDirty>,
) {
    let count = match hire {
        Hire::Free => {
            if state.free_left(ship.free_hires(&ship_data)) == 0 {
                toast.show("Бесплатные наймы на сегодня кончились — заходите завтра");
                return;
            }
            state.use_free();
            1
        }
        Hire::Single | Hire::Ten => {
            let (price, count) = match hire {
                Hire::Ten => (cfg.ten_price, 10),
                _ => (cfg.single_price, 1),
            };
            if profile.doubloons < price {
                toast.show(&format!(
                    "Не хватает дублонов: нужно {price}. Дублоны дают первые прохождения этапов"
                ));
                return;
            }
            profile.doubloons -= price;
            count
        }
    };
    let pool = pool_of(&cfg, &screen.data);
    let state = &mut *state;
    results.list = (0..count)
        .map(|_| {
            let (roll, pick) = (state.rng.next_f32(), state.rng.next_f32());
            let (id, rarity) = pull(&cfg, &mut state.pity, roll, pick, &pool);
            let new = roster.recruit(&id);
            (id, rarity, new)
        })
        .collect();
    results.elapsed = 0.0;
    dirty.0 = true;
}

fn results_overlay(
    commands: &mut Commands,
    assets: &AssetServer,
    fonts: &Fonts,
    results: &Results,
    data: &GameData,
) {
    commands
        .spawn((
            ResultsOverlay,
            DespawnOnExit(Screen::Tavern),
            Node {
                position_type: PositionType::Absolute,
                width: percent(100),
                height: percent(100),
                flex_direction: FlexDirection::Column,
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                row_gap: px(16),
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.82)),
            GlobalZIndex(30),
        ))
        .with_children(|overlay| {
            let any_new = results.list.iter().any(|(_, _, new)| *new);
            overlay.spawn(text(
                fonts,
                if any_new {
                    "Новые лица на борту!"
                } else {
                    "Итоги найма"
                },
                true,
                26.0,
                GOLD,
            ));
            overlay
                .spawn(Node {
                    width: px(520),
                    flex_wrap: FlexWrap::Wrap,
                    justify_content: JustifyContent::Center,
                    column_gap: px(8),
                    row_gap: px(10),
                    ..default()
                })
                .with_children(|grid| {
                    for (i, (id, rarity, new)) in results.list.iter().enumerate() {
                        let Some(hero) = data.hero(id) else {
                            continue;
                        };
                        let ssr = *rarity == Rarity::Ssr;
                        grid.spawn((
                            Reveal(i, ssr),
                            Node {
                                width: px(96),
                                height: px(150),
                                padding: UiRect::top(px(10)),
                                flex_direction: FlexDirection::Column,
                                align_items: AlignItems::Center,
                                row_gap: px(2),
                                border_radius: BorderRadius::all(px(12)),
                                ..default()
                            },
                            Skin(Look::Parchment),
                            Outline::new(px(if ssr { 3.0 } else { 0.0 }), px(1), GOLD),
                            Visibility::Hidden,
                        ))
                        .with_children(|card| {
                            card.spawn(style::medallion(assets, &hero.id, 66.0, false));
                            card.spawn(text(fonts, hero.name.clone(), true, 15.0, INK));
                            card.spawn(text(
                                fonts,
                                rarity_label(*rarity),
                                true,
                                13.0,
                                rarity_color(*rarity),
                            ));
                            card.spawn(text(
                                fonts,
                                if *new { "новый!" } else { "копия" },
                                *new,
                                12.0,
                                if *new {
                                    Color::srgb(0.70, 0.36, 0.05)
                                } else {
                                    INK_SOFT
                                },
                            ));
                        });
                    }
                });
            let mut done = overlay.spawn((
                Node {
                    width: px(220),
                    height: px(56),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    ..default()
                },
                Skin(Look::Go),
            ));
            done.with_child(text(fonts, "Забрать", true, 20.0, Color::WHITE));
            pressable(&mut done);
            done.observe(
                |_: On<PointerClick>,
                 mut commands: Commands,
                 mut results: ResMut<Results>,
                 overlays: Query<Entity, With<ResultsOverlay>>| {
                    results.list.clear();
                    for overlay in &overlays {
                        commands.entity(overlay).despawn();
                    }
                },
            );
        });
}

/// Карточки появляются по одной; SSR — с увеличением.
fn reveal_results(
    time: Res<Time>,
    mut results: ResMut<Results>,
    mut cards: Query<(&Reveal, &mut Visibility, &mut UiTransform)>,
    mut sfx: MessageWriter<Sfx>,
) {
    if results.list.is_empty() {
        return;
    }
    results.elapsed += time.delta_secs();
    for (Reveal(i, ssr), mut vis, mut transform) in &mut cards {
        let t = results.elapsed - *i as f32 * REVEAL_STEP;
        if t < 0.0 {
            continue;
        }
        if *vis == Visibility::Hidden {
            sfx.write(Sfx(if *ssr { "ssr" } else { "flip" }));
        }
        vis.set_if_neq(Visibility::Inherited);
        let k = (t / REVEAL_POP).min(1.0);
        let peak = if *ssr { SSR_POP_SCALE } else { 1.15 };
        // Выскакивает крупнее и оседает до обычного размера.
        transform.scale = Vec2::splat(1.0 + (peak - 1.0) * (1.0 - k));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pool() -> Vec<(String, Rarity)> {
        vec![
            ("a".into(), Rarity::Ssr),
            ("b".into(), Rarity::Sr),
            ("c".into(), Rarity::R),
        ]
    }

    #[test]
    fn ssr_guaranteed_by_pity() {
        let cfg = TavernData::bundled();
        let mut pity = Pity::default();
        // Всегда самый неудачный бросок: SSR должен прийти ровно на гаранте.
        let rarities: Vec<Rarity> = (0..cfg.ssr_pity)
            .map(|_| pull(&cfg, &mut pity, 0.999, 0.0, &pool()).1)
            .collect();
        assert_eq!(rarities.last(), Some(&Rarity::Ssr));
        assert_eq!(rarities.iter().filter(|r| **r == Rarity::Ssr).count(), 1);
    }

    #[test]
    fn sr_in_every_window() {
        let cfg = TavernData::bundled();
        let mut pity = Pity::default();
        let rarities: Vec<Rarity> = (0..100)
            .map(|_| pull(&cfg, &mut pity, 0.999, 0.0, &pool()).1)
            .collect();
        for window in rarities.windows(cfg.sr_every as usize) {
            assert!(window.iter().any(|r| *r != Rarity::R), "{window:?}");
        }
    }

    #[test]
    fn rates_pick_rarity() {
        let cfg = TavernData::bundled();
        let mut pity = Pity::default();
        assert_eq!(pull(&cfg, &mut pity, 0.0, 0.0, &pool()).1, Rarity::Ssr);
        assert_eq!(
            pull(&cfg, &mut pity, cfg.rates.ssr + 0.001, 0.0, &pool()).1,
            Rarity::Sr
        );
    }
}
