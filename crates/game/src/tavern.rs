//! Таверна: найм героев за дублоны и бесплатный найм раз в день.
//!
//! Найм даёт копии героев команды (все поровну) — копии нужны для звёзд. Новые герои приходят
//! только за главы (решения №76, №77). Пока всё считается на клиенте; в релизе крутки считает сервер.

use std::time::{SystemTime, UNIX_EPOCH};

use battle_core::Rng;
use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::Screen;
use crate::battle_screen::random_seed;
use crate::crew::{CrewData, Roster};
use crate::menu::{PARCHMENT, Profile, Toast, bottom_nav, pressable, spawn_toast, text};
use crate::ship::{Ship, ShipData};
use crate::style::{self, Look, Skin};
use crate::view::Fonts;

const BRIGHT: Color = Color::srgb(1.0, 0.93, 0.78);
const SECONDS_PER_DAY: u64 = 86_400;

/// Настройки найма: `data/tavern.json`.
#[derive(Resource, Deserialize)]
pub struct TavernData {
    pub single_price: u32,
    pub ten_price: u32,
    /// Шанс, что из сундука выйдет копия героя; иначе — ресурсы из `loot`.
    pub copy_chance: f32,
    pub loot: Vec<Loot>,
}

/// Ресурсы из сундука и их вес среди остальных.
#[derive(Deserialize)]
pub struct Loot {
    pub weight: u32,
    #[serde(flatten)]
    pub bundle: crate::events::Bundle,
}

impl TavernData {
    /// Ресурсы по броску `roll` в [0, 1).
    pub fn loot_at(&self, roll: f32) -> crate::events::Bundle {
        let total: u32 = self.loot.iter().map(|l| l.weight).sum();
        let mut left = roll * total as f32;
        for l in &self.loot {
            if left < l.weight as f32 {
                return l.bundle;
            }
            left -= l.weight as f32;
        }
        self.loot.last().map(|l| l.bundle).unwrap_or_default()
    }
}

impl TavernData {
    pub fn bundled() -> Self {
        serde_json::from_str(include_str!("../../../data/tavern.json"))
            .expect("data/tavern.json корректен")
    }
}

/// Какой герой в копии: один из героев команды, все поровну (`pick` в [0, 1)).
/// Новых героев Таверна не даёт — они приходят за главы (решения №76, №77).
pub fn pull(pick: f32, owned: &[String]) -> String {
    let index = ((pick * owned.len() as f32) as usize).min(owned.len() - 1);
    owned[index].clone()
}

/// Состояние Таверны между заходами (гарант и бесплатные наймы сохраняются в файл).
#[derive(Resource, Clone, Serialize, Deserialize)]
pub struct TavernState {
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

#[derive(Resource, Default)]
struct TavernDirty(bool);

#[derive(Component)]
struct TavernRoot;

#[derive(Clone, Copy)]
enum Hire {
    Free,
    Single,
    Ten,
}

pub fn plugin(app: &mut App) {
    app.insert_resource(TavernData::bundled())
        .init_resource::<TavernState>()
        .init_resource::<TavernDirty>()
        .add_systems(OnEnter(Screen::Tavern), spawn_tavern)
        .add_systems(Update, rebuild_tavern.run_if(in_state(Screen::Tavern)));
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
) {
    build(
        &mut commands,
        &assets,
        &fonts,
        &cfg,
        &state,
        ship.free_hires(&ship_data),
        &profile,
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
    );
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
        ))
        .with_children(|root| {
            style::top_bar(
                root,
                assets,
                fonts,
                &[("doubloons", profile.doubloons), ("silver", profile.silver)],
            );
            style::title(root, fonts, "Таверна · найм героев", 56.0);
            crate::help::corner(root, fonts, "tavern", 60.0, 12.0);
            chest_odds(root, assets, fonts, cfg);
            hire_buttons(
                root,
                assets,
                fonts,
                cfg,
                state.free_left(free_per_day),
                profile,
                640.0,
            );
            bottom_nav(root, assets, fonts, Screen::Tavern);
            spawn_toast(root, fonts);
        });
}

/// Что лежит в сундуке и с каким шансом.
fn chest_odds(
    root: &mut ChildSpawnerCommands,
    assets: &AssetServer,
    fonts: &Fonts,
    cfg: &TavernData,
) {
    let total: u32 = cfg.loot.iter().map(|l| l.weight).sum::<u32>().max(1);
    let rest = 1.0 - cfg.copy_chance;
    root.spawn((
        Node {
            position_type: PositionType::Absolute,
            top: px(116),
            left: px(10),
            right: px(10),
            padding: UiRect::axes(px(16), px(10)),
            flex_direction: FlexDirection::Column,
            row_gap: px(6),
            ..default()
        },
        Skin(Look::Tile),
    ))
    .with_children(|panel| {
        panel.spawn(style::heading(fonts, "Что в сундуке", 19.0, BRIGHT));
        let row = |panel: &mut ChildSpawnerCommands, path: String, label: String, chance: f32| {
            panel
                .spawn(Node {
                    column_gap: px(10),
                    align_items: AlignItems::Center,
                    ..default()
                })
                .with_children(|r| {
                    r.spawn((
                        Node {
                            width: px(30),
                            height: px(30),
                            flex_shrink: 0.0,
                            ..default()
                        },
                        ImageNode::new(assets.load(path)),
                    ));
                    r.spawn((
                        Node {
                            flex_grow: 1.0,
                            ..default()
                        },
                        style::heading(fonts, label, 17.0, PARCHMENT),
                    ));
                    r.spawn(style::heading(
                        fonts,
                        format!("{}%", (chance * 100.0).round()),
                        17.0,
                        BRIGHT,
                    ));
                });
        };
        row(
            panel,
            "ui/star.png".into(),
            "Копия героя команды".into(),
            cfg.copy_chance,
        );
        for l in &cfg.loot {
            if let Some((kind, n)) = l.bundle.main() {
                row(
                    panel,
                    format!("ui/icon_{kind}.png"),
                    format!("{} {n}", resource_name(kind)),
                    rest * l.weight as f32 / total as f32,
                );
            }
        }
    });
}

fn resource_name(kind: &str) -> &'static str {
    match kind {
        "journals" => "Судовые журналы",
        "silver" => "Серебро",
        "charts" => "Лоция",
        "details" => "Детали",
        "supplies" => "Припасы",
        "planks" => "Доски",
        "ether" => "Эфир",
        "doubloons" => "Дублоны",
        _ => "Ресурсы",
    }
}

fn hire_buttons(
    root: &mut ChildSpawnerCommands,
    assets: &AssetServer,
    fonts: &Fonts,
    cfg: &TavernData,
    free_left: u32,
    profile: &Profile,
    top: f32,
) {
    root.spawn(Node {
        position_type: PositionType::Absolute,
        top: px(top),
        left: px(10),
        right: px(16),
        flex_direction: FlexDirection::Column,
        row_gap: px(8),
        ..default()
    })
    .with_children(|col| {
        let free = free_left > 0;
        // В строке: у кнопки flex_basis 0 — по ширине, а в колонке он схлопнул бы её высоту.
        col.spawn(Node::default()).with_children(|row| {
            hire_button(
                row,
                assets,
                fonts,
                Hire::Free,
                &if free {
                    format!("Бесплатный найм · осталось {free_left}")
                } else {
                    "Бесплатный найм — завтра".to_string()
                },
                None,
                String::new(),
                if free { Look::Dark } else { Look::Off },
                54.0,
            );
        });
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
                    Some((price, profile.doubloons >= price)),
                    String::new(),
                    // Кнопка найма всегда янтарная; не хватает дублонов — цена красная.
                    Look::Go,
                    74.0,
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
    price: Option<(u32, bool)>,
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
        b.spawn(style::heading(
            fonts,
            title,
            if price.is_some() { 26.0 } else { 18.0 },
            Color::WHITE,
        ));
        match price {
            Some((price, affordable)) => style::costs(
                b,
                assets,
                fonts,
                &[("doubloons", price)],
                16.0,
                if affordable {
                    BRIGHT
                } else {
                    Color::srgb(0.50, 0.07, 0.04)
                },
            ),
            None if !sub.is_empty() => {
                b.spawn(text(fonts, sub, false, 11.0, PARCHMENT));
            }
            None => {}
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
    crew: Res<CrewData>,
    mut state: ResMut<TavernState>,
    ship: Res<Ship>,
    ship_data: Res<ShipData>,
    mut profile: ResMut<Profile>,
    mut roster: ResMut<Roster>,
    mut reveal: ResMut<crate::hire_reveal::HireReveal>,
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
    let owned = roster.owned();
    let state = &mut *state;
    // Сундук: копия героя с шансом `copy_chance`, иначе ресурсы; в найме ×10 копия хотя бы одна.
    let mut copies = (0..count)
        .map(|_| state.rng.next_f32() < cfg.copy_chance)
        .collect::<Vec<_>>();
    if count == 10 && !copies.contains(&true) {
        copies[9] = true;
    }
    let list = copies
        .into_iter()
        .map(|copy| {
            if copy {
                let id = pull(state.rng.next_f32(), &owned);
                // Порог звезды пересекла именно эта копия — подсветить её в показе.
                let before = roster.star_copies_ready(&id, &crew);
                roster.recruit(&id);
                let star = !before && roster.star_copies_ready(&id, &crew);
                crate::hire_reveal::Pulled {
                    id,
                    star,
                    loot: None,
                }
            } else {
                let loot = cfg.loot_at(state.rng.next_f32());
                loot.put(&mut profile);
                crate::hire_reveal::Pulled {
                    id: String::new(),
                    star: false,
                    loot: Some(loot),
                }
            }
        })
        .collect();
    reveal.start(list);
    dirty.0 = true;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pull_gives_only_owned_heroes_evenly() {
        let owned: Vec<String> = ["a", "b", "c", "d"].iter().map(|s| s.to_string()).collect();
        let mut counts = std::collections::HashMap::new();
        for i in 0..400 {
            let id = pull(i as f32 / 400.0, &owned);
            assert!(owned.contains(&id));
            *counts.entry(id).or_insert(0) += 1;
        }
        assert!(counts.values().all(|&n| n == 100), "{counts:?}");
        assert_eq!(pull(0.9999, &owned), "d");
    }

    #[test]
    fn chest_loot_follows_weights() {
        let cfg = TavernData::bundled();
        let total: u32 = cfg.loot.iter().map(|l| l.weight).sum();
        // Бросок в начале и в конце диапазона — первая и последняя добыча.
        assert_eq!(
            cfg.loot_at(0.0).main(),
            cfg.loot.first().and_then(|l| l.bundle.main())
        );
        assert_eq!(
            cfg.loot_at(0.9999).main(),
            cfg.loot.last().and_then(|l| l.bundle.main())
        );
        // Ровная сетка бросков даёт доли по весам.
        let first = cfg.loot[0].bundle.main();
        let hits = (0..1000)
            .filter(|i| cfg.loot_at(*i as f32 / 1000.0).main() == first)
            .count();
        let want = 1000 * cfg.loot[0].weight / total;
        assert!(
            (hits as i64 - want as i64).abs() <= 2,
            "{hits} против {want}"
        );
    }
}
