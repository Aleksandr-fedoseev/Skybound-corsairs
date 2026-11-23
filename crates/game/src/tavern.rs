//! Таверна: найм героев за дублоны и бесплатный найм раз в день.
//!
//! Найм даёт копии героев команды (все поровну) — копии нужны для звёзд. Новые герои приходят
//! только за главы (решения №69, №70). Пока всё считается на клиенте; в релизе крутки считает сервер.

use std::time::{SystemTime, UNIX_EPOCH};

use battle_core::Rng;
use bevy::prelude::*;
use game_data::GameData;
use serde::{Deserialize, Serialize};

use crate::Screen;
use crate::battle_screen::{BattleScreen, random_seed};
use crate::crew::{CrewData, Roster};
use crate::menu::{GOLD, PARCHMENT, Profile, Toast, bottom_nav, pressable, spawn_toast, text};
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
}

impl TavernData {
    pub fn bundled() -> Self {
        serde_json::from_str(include_str!("../../../data/tavern.json"))
            .expect("data/tavern.json корректен")
    }
}

/// Один найм: копия одного из героев команды, все поровну (`pick` в [0, 1)).
/// Новых героев Таверна не даёт — они приходят за главы (решения №69, №70).
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
    roster: Res<Roster>,
    screen: Res<BattleScreen>,
    crew: Res<CrewData>,
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
        &crew,
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
    crew: Res<CrewData>,
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
        &crew,
        &screen.data,
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
    roster: &Roster,
    crew: &CrewData,
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
        ))
        .with_children(|root| {
            style::top_bar(
                root,
                assets,
                fonts,
                &[("doubloons", profile.doubloons), ("silver", profile.silver)],
            );
            style::title(root, fonts, "Таверна · найм героев", 56.0);
            poster(root, assets, fonts, roster, crew, data);
            rules_strip(root, fonts);
            hire_buttons(
                root,
                assets,
                fonts,
                cfg,
                state.free_left(free_per_day),
                profile,
            );
            pool_row(root, assets, fonts, roster, crew);
            bottom_nav(root, assets, fonts, Screen::Tavern);
            spawn_toast(root, fonts);
        });
}

/// Баннер: герой, которому меньше всего не хватает копий до следующей звезды.
fn poster(
    root: &mut ChildSpawnerCommands,
    assets: &AssetServer,
    fonts: &Fonts,
    roster: &Roster,
    crew: &CrewData,
    data: &GameData,
) {
    let need = |id: &str| {
        let stars = roster.stars(id);
        if stars >= crew.max_stars() {
            return u32::MAX;
        }
        crew.rule(stars + 1)
            .copies
            .saturating_sub(roster.copies(id))
    };
    let owned = roster.owned();
    let Some(hero) = owned
        .iter()
        .min_by_key(|id| need(id))
        .and_then(|id| data.hero(id))
    else {
        return;
    };
    let glow = crate::view::element_color(hero.element);
    let stars = roster.stars(&hero.id);
    let left = need(&hero.id);
    root.spawn((
        Node {
            position_type: PositionType::Absolute,
            top: px(108),
            left: px(10),
            right: px(10),
            height: px(410),
            overflow: Overflow::clip(),
            ..default()
        },
        Skin(Look::Tile),
        BackgroundGradient(vec![
            RadialGradient::new(
                UiPosition::new(Vec2::new(0.25, 0.0), Val::ZERO, Val::ZERO),
                RadialGradientShape::FarthestCorner,
                vec![
                    ColorStop::auto(glow.with_alpha(0.8)),
                    ColorStop::auto(Color::srgb(0.07, 0.05, 0.03)),
                ],
            )
            .into(),
        ]),
    ))
    .with_children(|banner| {
        // Герой во весь рост — справа.
        banner.spawn((
            Node {
                position_type: PositionType::Absolute,
                right: px(-10),
                bottom: px(0),
                height: px(400),
                ..default()
            },
            ImageNode::new(assets.load(format!("ui/full_{}.png", hero.id))),
            Pickable::IGNORE,
        ));
        banner
            .spawn(Node {
                position_type: PositionType::Absolute,
                left: px(22),
                top: px(22),
                width: px(250),
                flex_direction: FlexDirection::Column,
                row_gap: px(6),
                ..default()
            })
            .with_children(|col| {
                col.spawn((
                    Node {
                        padding: UiRect::axes(px(12), px(3)),
                        align_self: AlignSelf::FlexStart,
                        ..default()
                    },
                    Skin(Look::Go),
                ))
                .with_child(style::heading(
                    fonts,
                    "Ближе всех к звезде",
                    14.0,
                    Color::WHITE,
                ));
                col.spawn(style::heading(fonts, hero.name.clone(), 40.0, BRIGHT));
                col.spawn(style::heading(fonts, hero.title.clone(), 18.0, PARCHMENT));
                col.spawn(text(
                    fonts,
                    if left == u32::MAX {
                        "все звёзды уже собраны".to_string()
                    } else if left == 0 {
                        format!(
                            "копий хватает на {}-ю звезду — слияние в Экипаже",
                            stars + 1
                        )
                    } else {
                        format!("до {}-й звезды не хватает копий: {left}", stars + 1)
                    },
                    false,
                    13.0,
                    PARCHMENT,
                ));
            });
    });
}

/// Герои команды: звёзды и копии к следующей звезде — их и даёт Таверна.
fn pool_row(
    root: &mut ChildSpawnerCommands,
    assets: &AssetServer,
    fonts: &Fonts,
    roster: &Roster,
    crew: &CrewData,
) {
    root.spawn((
        Node {
            position_type: PositionType::Absolute,
            bottom: px(86),
            left: px(6),
            right: px(6),
            height: px(84),
            padding: UiRect::axes(px(14), px(6)),
            justify_content: JustifyContent::SpaceEvenly,
            align_items: AlignItems::Center,
            ..default()
        },
        Skin(Look::Tile),
    ))
    .with_children(|rail| {
        for id in roster.owned() {
            let stars = roster.stars(&id);
            let line = if stars >= crew.max_stars() {
                format!("зв. {stars}")
            } else {
                format!(
                    "зв. {stars} · {}/{}",
                    roster.copies(&id),
                    crew.rule(stars + 1).copies
                )
            };
            let ready = roster.star_copies_ready(&id, crew);
            rail.spawn(Node {
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                ..default()
            })
            .with_children(|cell| {
                cell.spawn(style::medallion(assets, &id, 50.0, false));
                cell.spawn(text(
                    fonts,
                    line,
                    true,
                    11.0,
                    if ready { GOLD } else { PARCHMENT },
                ));
            });
        }
    });
}

/// Правила найма одной строкой.
fn rules_strip(root: &mut ChildSpawnerCommands, fonts: &Fonts) {
    root.spawn((
        Node {
            position_type: PositionType::Absolute,
            top: px(528),
            left: px(10),
            right: px(10),
            height: px(52),
            padding: UiRect::axes(px(16), px(6)),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            ..default()
        },
        Skin(Look::Tile),
    ))
    .with_child(text(
        fonts,
        "Найм даёт копии героев команды — все поровну. Копии нужны для звёзд. Новые герои приходят за главы.",
        false,
        13.0,
        PARCHMENT,
    ));
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
        left: px(10),
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
    let list = (0..count)
        .map(|_| {
            let id = pull(state.rng.next_f32(), &owned);
            // Порог звезды пересекла именно эта копия — подсветить её в показе.
            let before = roster.star_copies_ready(&id, &crew);
            roster.recruit(&id);
            let star = !before && roster.star_copies_ready(&id, &crew);
            crate::hire_reveal::Pulled { id, star }
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
}
