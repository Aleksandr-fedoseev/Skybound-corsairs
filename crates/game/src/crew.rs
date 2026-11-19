//! Экипаж: герои на борту, их уровни и отряд из четырёх на бой.
//!
//! Уровень покупается за судовые журналы и серебро — так добыча из боёв превращается в силу.
//! Потолок уровня пока один для всех (1★); звёзды и слияние будут позже.

use std::collections::HashMap;

use battle_core::{Element, Role};
use bevy::prelude::*;
use game_data::{GameData, HeroDef, Rarity};
use serde::Deserialize;

use crate::Screen;
use crate::audio::Sfx;
use crate::battle_screen::BattleScreen;
use crate::menu::{
    BRASS, GO, GO_EDGE, GOLD, MUTED, PARCHMENT, Profile, Toast, WOOD, bottom_nav, pressable,
    spawn_toast, text,
};
use crate::view::{Fonts, element_color};

const CARD_W: f32 = 164.0;
const CARD_H: f32 = 182.0;
const PORTRAIT: f32 = 72.0;
const DISABLED: Color = Color::srgb(0.36, 0.38, 0.43);
const DARK_TEXT: Color = Color::srgb(0.08, 0.10, 0.16);

/// Настройки экипажа: `data/crew.json`.
#[derive(Resource, Deserialize)]
pub struct CrewData {
    pub start_team: Vec<String>,
    pub team_size: usize,
    /// Потолок уровня на 1★.
    pub level_cap: u32,
    /// Журналов за переход с уровня 1 на 2; дальше каждый уровень дороже в `journals_growth` раз.
    pub journals_base: f32,
    pub journals_growth: f32,
    /// Серебро за уровень: `silver_per_level × текущий уровень`.
    pub silver_per_level: u32,
}

/// Цена перехода на следующий уровень.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LevelCost {
    pub journals: u32,
    pub silver: u32,
}

impl CrewData {
    pub fn bundled() -> Self {
        serde_json::from_str(include_str!("../../../data/crew.json"))
            .expect("data/crew.json корректен")
    }

    /// Сколько стоит поднять героя с уровня `level` на `level + 1`.
    pub fn level_cost(&self, level: u32) -> LevelCost {
        let steps = level.saturating_sub(1) as i32;
        LevelCost {
            journals: (self.journals_base * self.journals_growth.powi(steps)).round() as u32,
            silver: self.silver_per_level * level,
        }
    }
}

/// Нанятый герой: уровень и лишние копии (пойдут на звёзды через слияние).
#[derive(Clone, Copy)]
struct Owned {
    level: u32,
    copies: u32,
}

/// Герои на борту и отряд. Стартовый отряд уже в команде, остальных нанимают в Таверне.
#[derive(Resource)]
pub struct Roster {
    heroes: HashMap<String, Owned>,
    pub team: Vec<String>,
}

impl Roster {
    pub fn new(crew: &CrewData) -> Self {
        Self {
            heroes: crew
                .start_team
                .iter()
                .map(|id| {
                    (
                        id.clone(),
                        Owned {
                            level: 1,
                            copies: 0,
                        },
                    )
                })
                .collect(),
            team: crew.start_team.clone(),
        }
    }

    pub fn owns(&self, id: &str) -> bool {
        self.heroes.contains_key(id)
    }

    pub fn level(&self, id: &str) -> u32 {
        self.heroes.get(id).map_or(1, |h| h.level)
    }

    pub fn copies(&self, id: &str) -> u32 {
        self.heroes.get(id).map_or(0, |h| h.copies)
    }

    /// Герой из Таверны: новый — в команду, уже нанятый — копией. `true`, если новый.
    pub fn recruit(&mut self, id: &str) -> bool {
        match self.heroes.get_mut(id) {
            Some(hero) => {
                hero.copies += 1;
                false
            }
            None => {
                self.heroes.insert(
                    id.to_string(),
                    Owned {
                        level: 1,
                        copies: 0,
                    },
                );
                true
            }
        }
    }

    pub fn team_levels(&self) -> Vec<(String, u32)> {
        self.team
            .iter()
            .map(|id| (id.clone(), self.level(id)))
            .collect()
    }
}

/// Герой, открытый в карточке.
#[derive(Resource, Default)]
struct Selected(Option<String>);

/// Экран надо собрать заново: уровень вырос или отряд изменился.
#[derive(Resource, Default)]
struct CrewDirty(bool);

#[derive(Component)]
struct CrewRoot;

pub fn plugin(app: &mut App) {
    app.init_resource::<Selected>()
        .init_resource::<CrewDirty>()
        .add_systems(OnEnter(Screen::Crew), spawn_crew)
        .add_systems(Update, rebuild_crew.run_if(in_state(Screen::Crew)));
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
        Rarity::R => Color::srgb(0.70, 0.74, 0.80),
        Rarity::Sr => Color::srgb(0.70, 0.55, 1.0),
        Rarity::Ssr => GOLD,
    }
}

fn role_label(role: Role) -> &'static str {
    match role {
        Role::Gunner => "Канонир",
        Role::Duelist => "Дуэлянт",
        Role::Boarder => "Абордажник",
        Role::Navigator => "Штурман",
        Role::Medic => "Корабельный врач",
        Role::WindShaman => "Шаманка ветров",
    }
}

fn element_label(element: Element) -> &'static str {
    match element {
        Element::Wind => "ветер",
        Element::Lightning => "молния",
        Element::Powder => "порох",
        Element::Steel => "сталь",
        Element::Mist => "туман",
    }
}

#[allow(clippy::too_many_arguments)]
fn spawn_crew(
    mut commands: Commands,
    assets: Res<AssetServer>,
    fonts: Res<Fonts>,
    crew: Res<CrewData>,
    roster: Res<Roster>,
    profile: Res<Profile>,
    screen: Res<BattleScreen>,
    mut selected: ResMut<Selected>,
) {
    if selected.0.is_none() {
        selected.0 = roster.team.first().cloned();
    }
    build(
        &mut commands,
        &assets,
        &fonts,
        &crew,
        &roster,
        &profile,
        &screen.data,
        selected.0.as_deref(),
    );
}

#[allow(clippy::too_many_arguments)]
fn rebuild_crew(
    mut commands: Commands,
    mut dirty: ResMut<CrewDirty>,
    roots: Query<Entity, With<CrewRoot>>,
    assets: Res<AssetServer>,
    fonts: Res<Fonts>,
    crew: Res<CrewData>,
    roster: Res<Roster>,
    profile: Res<Profile>,
    screen: Res<BattleScreen>,
    selected: Res<Selected>,
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
        &crew,
        &roster,
        &profile,
        &screen.data,
        selected.0.as_deref(),
    );
}

#[allow(clippy::too_many_arguments)]
fn build(
    commands: &mut Commands,
    assets: &AssetServer,
    fonts: &Fonts,
    crew: &CrewData,
    roster: &Roster,
    profile: &Profile,
    data: &GameData,
    selected: Option<&str>,
) {
    commands
        .spawn((
            CrewRoot,
            DespawnOnExit(Screen::Crew),
            Node {
                width: percent(100),
                height: percent(100),
                ..default()
            },
            // Каюты экипажа — тёмное дерево.
            BackgroundColor(Color::srgb(0.17, 0.12, 0.08)),
        ))
        .with_children(|root| {
            header(root, fonts, profile, roster.team.len(), crew.team_size);
            root.spawn(Node {
                position_type: PositionType::Absolute,
                top: px(66),
                left: px(10),
                width: px(520),
                flex_wrap: FlexWrap::Wrap,
                justify_content: JustifyContent::SpaceBetween,
                row_gap: px(10),
                ..default()
            })
            .with_children(|grid| {
                for hero in &data.heroes {
                    hero_card(
                        grid,
                        fonts,
                        hero,
                        roster,
                        selected == Some(hero.id.as_str()),
                    );
                }
            });
            if let Some(hero) = selected.and_then(|id| data.hero(id)) {
                details(root, fonts, crew, roster, profile, hero);
            }
            bottom_nav(root, assets, fonts, Screen::Crew);
            spawn_toast(root, fonts);
        });
}

fn header(
    root: &mut ChildSpawnerCommands,
    fonts: &Fonts,
    profile: &Profile,
    in_team: usize,
    team_size: usize,
) {
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
        bar.spawn(text(
            fonts,
            format!("Экипаж · отряд {in_team}/{team_size}"),
            true,
            19.0,
            PARCHMENT,
        ));
        bar.spawn(text(
            fonts,
            format!("журналы {} · серебро {}", profile.journals, profile.silver),
            false,
            14.0,
            MUTED,
        ));
    });
}

fn hero_card(
    grid: &mut ChildSpawnerCommands,
    fonts: &Fonts,
    hero: &HeroDef,
    roster: &Roster,
    selected: bool,
) {
    let in_team = roster.team.contains(&hero.id);
    let owned = roster.owns(&hero.id);
    let id = hero.id.clone();
    let mut card = grid.spawn((
        Node {
            width: px(CARD_W),
            height: px(CARD_H),
            padding: UiRect::top(px(24)),
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::Center,
            row_gap: px(1),
            border: UiRect::all(px(if selected { 3.0 } else { 1.0 })),
            border_radius: BorderRadius::all(px(14)),
            ..default()
        },
        BackgroundColor(WOOD),
        BorderColor::all(if selected { GOLD } else { BRASS }),
    ));
    card.with_children(|c| {
        c.spawn((
            Node {
                width: px(PORTRAIT),
                height: px(PORTRAIT),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                border: UiRect::all(px(3)),
                border_radius: BorderRadius::MAX,
                ..default()
            },
            BackgroundColor(if owned {
                element_color(hero.element)
            } else {
                element_color(hero.element).with_alpha(0.3)
            }),
            BorderColor::all(rarity_color(hero.rarity)),
            Pickable::IGNORE,
        ))
        .with_child((
            Text::new(
                hero.name
                    .chars()
                    .next()
                    .map(String::from)
                    .unwrap_or_default(),
            ),
            fonts.text(true, 36.0),
            TextColor(DARK_TEXT),
            Pickable::IGNORE,
        ));
        c.spawn(text(fonts, hero.name.clone(), true, 16.0, PARCHMENT));
        c.spawn(text(
            fonts,
            if owned {
                format!(
                    "ур. {} · {}",
                    roster.level(&hero.id),
                    rarity_label(hero.rarity)
                )
            } else {
                format!("в Таверне · {}", rarity_label(hero.rarity))
            },
            false,
            13.0,
            rarity_color(hero.rarity),
        ));
        c.spawn(text(fonts, role_label(hero.role), false, 12.0, MUTED));
        if in_team {
            c.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    top: px(5),
                    left: px(5),
                    padding: UiRect::axes(px(6), px(1)),
                    border_radius: BorderRadius::all(px(6)),
                    ..default()
                },
                BackgroundColor(GOLD),
                Pickable::IGNORE,
            ))
            .with_child((
                Text::new("в отряде"),
                fonts.text(true, 11.0),
                TextColor(DARK_TEXT),
                Pickable::IGNORE,
            ));
        }
    });
    pressable(&mut card);
    card.observe(
        move |_: On<PointerClick>, mut selected: ResMut<Selected>, mut dirty: ResMut<CrewDirty>| {
            selected.0 = Some(id.clone());
            dirty.0 = true;
        },
    );
}

fn details(
    root: &mut ChildSpawnerCommands,
    fonts: &Fonts,
    crew: &CrewData,
    roster: &Roster,
    profile: &Profile,
    hero: &HeroDef,
) {
    let level = roster.level(&hero.id);
    let at_cap = level >= crew.level_cap;
    let now = hero.to_unit(level).stats;
    let next = hero.to_unit(level + 1).stats;
    let cost = crew.level_cost(level);
    let affordable = profile.journals >= cost.journals && profile.silver >= cost.silver;
    let in_team = roster.team.contains(&hero.id);
    let owned = roster.owns(&hero.id);
    let copies = roster.copies(&hero.id);

    root.spawn((
        Node {
            position_type: PositionType::Absolute,
            top: px(456),
            left: px(10),
            width: px(520),
            padding: UiRect::all(px(14)),
            flex_direction: FlexDirection::Column,
            row_gap: px(6),
            border: UiRect::all(px(2)),
            border_radius: BorderRadius::all(px(14)),
            ..default()
        },
        BackgroundColor(Color::srgba(0.11, 0.075, 0.05, 0.96)),
        BorderColor::all(BRASS),
    ))
    .with_children(|panel| {
        panel
            .spawn(Node {
                column_gap: px(8),
                align_items: AlignItems::Baseline,
                ..default()
            })
            .with_children(|row| {
                row.spawn(text(fonts, hero.name.clone(), true, 22.0, PARCHMENT));
                row.spawn(text(fonts, hero.title.clone(), false, 15.0, MUTED));
            });
        panel.spawn(text(
            fonts,
            format!(
                "{} · {} · стихия: {} · {}",
                rarity_label(hero.rarity),
                role_label(hero.role),
                element_label(hero.element),
                if owned {
                    format!("уровень {level} из {} · копий {copies}", crew.level_cap)
                } else {
                    "ещё не в команде".to_string()
                }
            ),
            false,
            13.0,
            PARCHMENT,
        ));

        // Характеристики сейчас и на следующем уровне.
        panel
            .spawn(Node {
                margin: UiRect::vertical(px(4)),
                flex_direction: FlexDirection::Column,
                row_gap: px(3),
                ..default()
            })
            .with_children(|stats| {
                for (name, a, b) in [
                    ("Здоровье", now.max_hp, next.max_hp),
                    ("Атака", now.atk, next.atk),
                    ("Защита", now.def, next.def),
                    ("Скорость", now.spd, next.spd),
                ] {
                    stats
                        .spawn(Node {
                            column_gap: px(10),
                            ..default()
                        })
                        .with_children(|row| {
                            row.spawn((
                                Node {
                                    width: px(110),
                                    ..default()
                                },
                                text(fonts, name, false, 14.0, MUTED),
                            ));
                            row.spawn(text(fonts, a.to_string(), true, 14.0, PARCHMENT));
                            if owned && !at_cap && b > a {
                                row.spawn(text(
                                    fonts,
                                    format!("» {b}  (+{})", b - a),
                                    false,
                                    14.0,
                                    Color::srgb(0.55, 0.88, 0.55),
                                ));
                            }
                        });
                }
            });

        for line in [
            format!("Атака: {}", hero.basic.name),
            format!(
                "Спецатака: {} — раз в {} хода",
                hero.skill.name, hero.skill.cooldown
            ),
            format!("Ульта: {}", hero.ultimate.name),
        ] {
            panel.spawn(text(fonts, line, false, 13.0, PARCHMENT));
        }

        panel
            .spawn(Node {
                margin: UiRect::top(px(10)),
                column_gap: px(10),
                ..default()
            })
            .with_children(|buttons| {
                if owned {
                    level_button(buttons, fonts, cost, at_cap, affordable, crew.level_cap);
                    team_button(buttons, fonts, &hero.id, in_team);
                } else {
                    to_tavern_button(buttons, fonts);
                }
            });
    });
}

fn level_button(
    buttons: &mut ChildSpawnerCommands,
    fonts: &Fonts,
    cost: LevelCost,
    at_cap: bool,
    affordable: bool,
    cap: u32,
) {
    let ready = !at_cap && affordable;
    let mut button = buttons.spawn((
        Node {
            flex_grow: 1.0,
            height: px(64),
            flex_direction: FlexDirection::Column,
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            border: UiRect::all(px(2)),
            border_radius: BorderRadius::all(px(12)),
            ..default()
        },
        BackgroundColor(if ready { GO } else { DISABLED }),
        BorderColor::all(if ready { GO_EDGE } else { MUTED }),
    ));
    button.with_children(|b| {
        if at_cap {
            b.spawn(text(
                fonts,
                format!("Потолок: уровень {cap}"),
                true,
                18.0,
                Color::WHITE,
            ));
            b.spawn(text(fonts, "дальше — звёзды героя", false, 12.0, PARCHMENT));
        } else {
            b.spawn(text(fonts, "Повысить уровень", true, 18.0, Color::WHITE));
            b.spawn(text(
                fonts,
                format!("{} журналов · {} серебра", cost.journals, cost.silver),
                false,
                12.0,
                PARCHMENT,
            ));
        }
    });
    pressable(&mut button);
    button.observe(level_up);
}

/// Поднять уровень героя в карточке, если хватает журналов и серебра.
#[allow(clippy::too_many_arguments)]
fn level_up(
    _: On<PointerClick>,
    crew: Res<CrewData>,
    selected: Res<Selected>,
    mut roster: ResMut<Roster>,
    mut profile: ResMut<Profile>,
    mut toast: ResMut<Toast>,
    mut dirty: ResMut<CrewDirty>,
    screen: Res<BattleScreen>,
    mut sfx: MessageWriter<Sfx>,
) {
    let Some(id) = selected.0.clone() else {
        return;
    };
    let level = roster.level(&id);
    if level >= crew.level_cap {
        toast.show("Потолок уровня — дальше растить звёздами");
        return;
    }
    let cost = crew.level_cost(level);
    if profile.journals < cost.journals || profile.silver < cost.silver {
        toast.show(&format!(
            "Не хватает: нужно {} журналов и {} серебра — добудьте в боях",
            cost.journals, cost.silver
        ));
        return;
    }
    profile.journals -= cost.journals;
    profile.silver -= cost.silver;
    if let Some(hero) = roster.heroes.get_mut(&id) {
        hero.level += 1;
    }
    let name = screen
        .data
        .hero(&id)
        .map_or(id.as_str(), |h| h.name.as_str());
    toast.show(&format!("{name}: уровень {}", level + 1));
    sfx.write(Sfx("levelup"));
    dirty.0 = true;
}

/// Героя ещё нет — кнопка ведёт в Таверну.
fn to_tavern_button(buttons: &mut ChildSpawnerCommands, fonts: &Fonts) {
    let mut button = buttons.spawn((
        Node {
            flex_grow: 1.0,
            height: px(64),
            flex_direction: FlexDirection::Column,
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            border: UiRect::all(px(2)),
            border_radius: BorderRadius::all(px(12)),
            ..default()
        },
        BackgroundColor(GO),
        BorderColor::all(GO_EDGE),
    ));
    button.with_children(|b| {
        b.spawn(text(fonts, "Нанять в Таверне", true, 18.0, Color::WHITE));
        b.spawn(text(
            fonts,
            "герой выпадает при найме",
            false,
            12.0,
            PARCHMENT,
        ));
    });
    pressable(&mut button);
    button.observe(|_: On<PointerClick>, mut next: ResMut<NextState<Screen>>| {
        next.set(Screen::Tavern);
    });
}

fn team_button(buttons: &mut ChildSpawnerCommands, fonts: &Fonts, id: &str, in_team: bool) {
    let id = id.to_string();
    let mut button = buttons.spawn((
        Node {
            width: px(170),
            height: px(64),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            border: UiRect::all(px(2)),
            border_radius: BorderRadius::all(px(12)),
            ..default()
        },
        BackgroundColor(Color::srgba(0.10, 0.18, 0.30, 0.9)),
        BorderColor::all(BRASS),
    ));
    button.with_child(text(
        fonts,
        if in_team {
            "Убрать из отряда"
        } else {
            "В отряд"
        },
        true,
        16.0,
        PARCHMENT,
    ));
    pressable(&mut button);
    button.observe(
        move |_: On<PointerClick>,
              crew: Res<CrewData>,
              mut roster: ResMut<Roster>,
              mut toast: ResMut<Toast>,
              mut dirty: ResMut<CrewDirty>| {
            if let Some(i) = roster.team.iter().position(|h| *h == id) {
                if roster.team.len() > 1 {
                    roster.team.remove(i);
                    dirty.0 = true;
                } else {
                    toast.show("В отряде нужен хотя бы один герой");
                }
            } else if roster.team.len() < crew.team_size {
                roster.team.push(id.clone());
                dirty.0 = true;
            } else {
                toast.show("Отряд полон — сначала уберите кого-нибудь");
            }
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn crew() -> CrewData {
        CrewData::bundled()
    }

    #[test]
    fn first_level_costs_base() {
        let crew = crew();
        let cost = crew.level_cost(1);
        assert_eq!(cost.journals, crew.journals_base.round() as u32);
        assert_eq!(cost.silver, crew.silver_per_level);
    }

    #[test]
    fn each_level_costs_more() {
        let crew = crew();
        for level in 1..crew.level_cap {
            let (a, b) = (crew.level_cost(level), crew.level_cost(level + 1));
            assert!(
                b.journals > a.journals && b.silver > a.silver,
                "уровень {level}"
            );
        }
    }
}
