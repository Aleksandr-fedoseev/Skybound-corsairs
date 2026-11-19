//! Таверна: найм героев за дублоны и бесплатный найм раз в день.
//!
//! Шансы и гарант показаны на экране. Новый герой приходит в команду, повтор — копией
//! для будущих звёзд. Пока всё считается на клиенте; в релизе крутки считает сервер.

use std::time::{SystemTime, UNIX_EPOCH};

use battle_core::Rng;
use bevy::prelude::*;
use game_data::{GameData, Rarity};
use serde::Deserialize;

use crate::Screen;
use crate::battle_screen::{BattleScreen, random_seed};
use crate::crew::Roster;
use crate::menu::{
    BRASS, GO, GO_EDGE, GOLD, MUTED, PARCHMENT, Profile, Toast, WOOD, bottom_nav, pressable,
    spawn_toast, text,
};
use crate::view::{Fonts, element_color};

const FREE: Color = Color::srgb(0.30, 0.62, 0.38);
const DISABLED: Color = Color::srgb(0.36, 0.38, 0.43);
const DARK_TEXT: Color = Color::srgb(0.08, 0.10, 0.16);
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
#[derive(Resource, Default, Clone, Copy, Debug)]
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

/// Состояние Таверны между заходами.
#[derive(Resource)]
struct TavernState {
    pity: Pity,
    rng: Rng,
    /// День (по UTC), когда был взят бесплатный найм.
    free_used_on: Option<u64>,
}

impl Default for TavernState {
    fn default() -> Self {
        Self {
            pity: Pity::default(),
            rng: Rng::new(random_seed()),
            free_used_on: None,
        }
    }
}

fn today() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() / SECONDS_PER_DAY)
}

impl TavernState {
    fn free_ready(&self) -> bool {
        self.free_used_on != Some(today())
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
        &profile,
        &roster,
        &screen.data,
    );
    if !results.list.is_empty() {
        results_overlay(&mut commands, &fonts, &results, &screen.data);
    }
}

#[allow(clippy::too_many_arguments)]
fn build(
    commands: &mut Commands,
    assets: &AssetServer,
    fonts: &Fonts,
    cfg: &TavernData,
    state: &TavernState,
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
            BackgroundColor(Color::srgb(0.17, 0.12, 0.08)),
        ))
        .with_children(|root| {
            header(root, fonts, profile);
            root.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    top: px(66),
                    left: px(135),
                    width: px(270),
                    height: px(212),
                    ..default()
                },
                ImageNode::new(assets.load("menu/module_tavern.png")),
                Pickable::IGNORE,
            ));
            pool_row(root, fonts, cfg, roster, data);
            odds(root, fonts, cfg, state);
            hire_buttons(root, fonts, cfg, state, profile);
            bottom_nav(root, assets, fonts, Screen::Tavern);
            spawn_toast(root, fonts);
        });
}

fn header(root: &mut ChildSpawnerCommands, fonts: &Fonts, profile: &Profile) {
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
        bar.spawn(text(fonts, "Таверна", true, 20.0, PARCHMENT));
        bar.spawn(text(
            fonts,
            format!("дублоны {}", profile.doubloons),
            true,
            15.0,
            GOLD,
        ));
    });
}

fn portrait(
    parent: &mut ChildSpawnerCommands,
    fonts: &Fonts,
    size: f32,
    name: &str,
    color: Color,
    edge: Color,
) {
    parent
        .spawn((
            Node {
                width: px(size),
                height: px(size),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                border: UiRect::all(px(3)),
                border_radius: BorderRadius::MAX,
                ..default()
            },
            BackgroundColor(color),
            BorderColor::all(edge),
            Pickable::IGNORE,
        ))
        .with_child((
            Text::new(name.chars().next().map(String::from).unwrap_or_default()),
            fonts.text(true, size * 0.45),
            TextColor(DARK_TEXT),
            Pickable::IGNORE,
        ));
}

/// Кто сейчас в таверне: все, кто может выпасть.
fn pool_row(
    root: &mut ChildSpawnerCommands,
    fonts: &Fonts,
    cfg: &TavernData,
    roster: &Roster,
    data: &GameData,
) {
    root.spawn(Node {
        position_type: PositionType::Absolute,
        top: px(286),
        left: px(10),
        width: px(520),
        flex_direction: FlexDirection::Column,
        align_items: AlignItems::Center,
        row_gap: px(6),
        ..default()
    })
    .with_children(|col| {
        col.spawn(text(fonts, "Сегодня в таверне", true, 15.0, PARCHMENT));
        col.spawn(Node {
            column_gap: px(10),
            ..default()
        })
        .with_children(|row| {
            for id in &cfg.pool {
                let Some(hero) = data.hero(id) else {
                    continue;
                };
                row.spawn(Node {
                    width: px(76),
                    flex_direction: FlexDirection::Column,
                    align_items: AlignItems::Center,
                    ..default()
                })
                .with_children(|cell| {
                    portrait(
                        cell,
                        fonts,
                        56.0,
                        &hero.name,
                        element_color(hero.element),
                        rarity_color(hero.rarity),
                    );
                    cell.spawn(text(fonts, hero.name.clone(), true, 13.0, PARCHMENT));
                    let (label, color) = if roster.owns(id) {
                        ("в команде", MUTED)
                    } else {
                        ("новый!", GOLD)
                    };
                    cell.spawn(text(
                        fonts,
                        rarity_label(hero.rarity),
                        true,
                        12.0,
                        rarity_color(hero.rarity),
                    ));
                    cell.spawn(text(fonts, label, false, 11.0, color));
                });
            }
        });
    });
}

/// Опубликованные шансы и гарант.
fn odds(root: &mut ChildSpawnerCommands, fonts: &Fonts, cfg: &TavernData, state: &TavernState) {
    let pct = |p: f32| format!("{}%", (p * 1000.0).round() / 10.0);
    let r = 1.0 - cfg.rates.ssr - cfg.rates.sr;
    let left = cfg.ssr_pity.saturating_sub(state.pity.since_ssr);
    root.spawn((
        Node {
            position_type: PositionType::Absolute,
            top: px(424),
            left: px(10),
            width: px(520),
            padding: UiRect::all(px(12)),
            flex_direction: FlexDirection::Column,
            row_gap: px(4),
            border: UiRect::all(px(1)),
            border_radius: BorderRadius::all(px(12)),
            ..default()
        },
        BackgroundColor(WOOD),
        BorderColor::all(BRASS),
    ))
    .with_children(|panel| {
        panel.spawn(text(
            fonts,
            format!(
                "Шансы: SSR {} · SR {} · R {}",
                pct(cfg.rates.ssr),
                pct(cfg.rates.sr),
                pct(r)
            ),
            true,
            14.0,
            PARCHMENT,
        ));
        panel.spawn(text(
            fonts,
            format!(
                "Гарант: SSR не позже {}-го найма — осталось {left}. В каждых {} наймах есть SR или выше.",
                cfg.ssr_pity, cfg.sr_every
            ),
            false,
            12.0,
            MUTED,
        ));
        panel.spawn(text(
            fonts,
            "Внутри редкости герои выпадают поровну. Повтор героя — копия для звёзд.",
            false,
            12.0,
            MUTED,
        ));
    });
}

fn hire_buttons(
    root: &mut ChildSpawnerCommands,
    fonts: &Fonts,
    cfg: &TavernData,
    state: &TavernState,
    profile: &Profile,
) {
    root.spawn(Node {
        position_type: PositionType::Absolute,
        top: px(560),
        left: px(10),
        width: px(520),
        flex_direction: FlexDirection::Column,
        row_gap: px(10),
        ..default()
    })
    .with_children(|col| {
        let free = state.free_ready();
        hire_button(
            col,
            fonts,
            Hire::Free,
            if free {
                "Бесплатный найм"
            } else {
                "Бесплатный найм — завтра"
            },
            "раз в день".to_string(),
            if free { FREE } else { DISABLED },
        );
        col.spawn(Node {
            column_gap: px(10),
            ..default()
        })
        .with_children(|row| {
            for (hire, title, price) in [
                (Hire::Single, "Нанять ×1", cfg.single_price),
                (Hire::Ten, "Нанять ×10", cfg.ten_price),
            ] {
                hire_button(
                    row,
                    fonts,
                    hire,
                    title,
                    format!("{price} дублонов"),
                    if profile.doubloons >= price {
                        GO
                    } else {
                        DISABLED
                    },
                );
            }
        });
    });
}

fn hire_button(
    parent: &mut ChildSpawnerCommands,
    fonts: &Fonts,
    hire: Hire,
    title: &str,
    sub: String,
    fill: Color,
) {
    let mut button = parent.spawn((
        Node {
            flex_grow: 1.0,
            flex_basis: px(0),
            height: px(70),
            flex_direction: FlexDirection::Column,
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            border: UiRect::all(px(2)),
            border_radius: BorderRadius::all(px(12)),
            ..default()
        },
        BackgroundColor(fill),
        BorderColor::all(if fill == DISABLED { MUTED } else { GO_EDGE }),
    ));
    button.with_children(|b| {
        b.spawn(text(fonts, title, true, 20.0, Color::WHITE));
        b.spawn(text(fonts, sub, false, 13.0, PARCHMENT));
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
    mut profile: ResMut<Profile>,
    mut roster: ResMut<Roster>,
    mut results: ResMut<Results>,
    mut toast: ResMut<Toast>,
    mut dirty: ResMut<TavernDirty>,
) {
    let count = match hire {
        Hire::Free => {
            if !state.free_ready() {
                toast.show("Бесплатный найм уже был сегодня — заходите завтра");
                return;
            }
            state.free_used_on = Some(today());
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

fn results_overlay(commands: &mut Commands, fonts: &Fonts, results: &Results, data: &GameData) {
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
                                border: UiRect::all(px(if ssr { 3.0 } else { 1.0 })),
                                border_radius: BorderRadius::all(px(12)),
                                ..default()
                            },
                            BackgroundColor(WOOD),
                            BorderColor::all(rarity_color(*rarity)),
                            Visibility::Hidden,
                        ))
                        .with_children(|card| {
                            portrait(
                                card,
                                fonts,
                                62.0,
                                &hero.name,
                                element_color(hero.element),
                                rarity_color(*rarity),
                            );
                            card.spawn(text(fonts, hero.name.clone(), true, 15.0, PARCHMENT));
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
                                if *new { GOLD } else { MUTED },
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
                    border: UiRect::all(px(2)),
                    border_radius: BorderRadius::all(px(12)),
                    ..default()
                },
                BackgroundColor(GO),
                BorderColor::all(GO_EDGE),
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
