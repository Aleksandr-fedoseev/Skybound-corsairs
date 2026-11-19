//! Экипаж: герои на борту, их уровни и отряд из четырёх на бой.
//!
//! Уровень покупается за судовые журналы и серебро — так добыча из боёв превращается в силу.
//! Потолок уровня пока один для всех (1★); звёзды и слияние будут позже.

use std::collections::HashMap;

use battle_core::{Effect, Element, Role, UnitDef};
use bevy::prelude::*;
use game_data::{GameData, HeroDef, Rarity};
use serde::{Deserialize, Serialize};

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
const STAR_FILL: Color = Color::srgb(0.62, 0.42, 0.10);
const BETTER: Color = Color::srgb(0.55, 0.88, 0.55);
const DARK_TEXT: Color = Color::srgb(0.08, 0.10, 0.16);

/// Настройки экипажа: `data/crew.json`.
#[derive(Resource, Deserialize, Clone)]
pub struct CrewData {
    pub start_team: Vec<String>,
    pub team_size: usize,
    /// Опорные точки «уровень → журналов за следующий уровень» (из концепта); между ними
    /// цена растёт плавно, в одно и то же число раз за уровень.
    pub journal_steps: Vec<(u32, f32)>,
    /// Серебро за уровень: `silver_per_level × текущий уровень`.
    pub silver_per_level: u32,
    /// Звёзды 1★–6★: потолок уровня, цена слияния, прибавка к характеристикам.
    pub stars: Vec<StarRule>,
    /// С этой звезды ульта сильнее во столько раз.
    pub ult_boost_from_star: u32,
    pub ult_boost: f32,
    /// С этой звезды перезарядка спецатаки на ход короче (но не меньше минимума).
    pub skill_cut_from_star: u32,
    pub skill_min_cooldown: u32,
}

#[derive(Deserialize, Clone, Copy)]
pub struct StarRule {
    pub star: u32,
    pub level_cap: u32,
    /// Сколько копий самого героя нужно, чтобы дойти до этой звезды.
    pub copies: u32,
    /// Сколько копий других героев той же стихии (корма).
    pub fodder: u32,
    /// Прибавка ко всем характеристикам (0.2 = +20%).
    pub stat_bonus: f32,
}

/// Что уйдёт на слияние: свои копии и копии других героев той же стихии.
#[derive(Debug, PartialEq, Eq)]
pub struct MergePlan {
    pub to: u32,
    pub own: u32,
    pub fodder: Vec<(String, u32)>,
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

    pub fn max_stars(&self) -> u32 {
        self.stars.last().map_or(1, |r| r.star)
    }

    fn rule(&self, star: u32) -> StarRule {
        self.stars
            .iter()
            .find(|r| r.star == star)
            .or(self.stars.last())
            .copied()
            .expect("в data/crew.json есть звёзды")
    }

    pub fn level_cap(&self, stars: u32) -> u32 {
        self.rule(stars).level_cap
    }

    /// Боец для боя: уровень, затем звёзды — прибавка к характеристикам, ульта, перезарядка.
    pub fn hero_unit(&self, hero: &HeroDef, level: u32, stars: u32) -> UnitDef {
        let mut unit = hero.to_unit(level);
        let k = 1.0 + self.rule(stars).stat_bonus;
        let boost = |v: u32| (v as f32 * k).round() as u32;
        unit.stats.max_hp = boost(unit.stats.max_hp);
        unit.stats.atk = boost(unit.stats.atk);
        unit.stats.def = boost(unit.stats.def);
        if stars >= self.ult_boost_from_star
            && let Some(ult) = &mut unit.ultimate
        {
            for e in &mut ult.effects {
                if let Effect::Damage { multiplier } = &mut e.effect {
                    *multiplier *= self.ult_boost;
                }
            }
        }
        if stars >= self.skill_cut_from_star
            && let Some(skill) = &mut unit.skill
        {
            skill.cooldown = skill
                .cooldown
                .saturating_sub(1)
                .max(self.skill_min_cooldown);
        }
        unit
    }

    /// Сколько стоит поднять героя с уровня `level` на `level + 1`.
    pub fn level_cost(&self, level: u32) -> LevelCost {
        let steps = &self.journal_steps;
        let journals = match steps.windows(2).find(|w| level <= w[1].0) {
            Some(w) => {
                let ((l0, j0), (l1, j1)) = (w[0], w[1]);
                let t = (level.max(l0) - l0) as f32 / (l1 - l0) as f32;
                j0 * (j1 / j0).powf(t)
            }
            None => steps.last().map_or(0.0, |s| s.1),
        };
        LevelCost {
            journals: journals.round() as u32,
            silver: self.silver_per_level * level,
        }
    }
}

/// Нанятый герой: уровень, звёзды и лишние копии (идут на слияние).
#[derive(Clone, Copy, Serialize, Deserialize)]
struct Owned {
    level: u32,
    copies: u32,
    #[serde(default = "first_star")]
    stars: u32,
}

fn first_star() -> u32 {
    1
}

/// Боец отряда для боя.
#[derive(Clone, Debug)]
pub struct Member {
    pub id: String,
    pub level: u32,
    pub stars: u32,
}

/// Герои на борту и отряд. Стартовый отряд уже в команде, остальных нанимают в Таверне.
#[derive(Resource, Clone, Serialize, Deserialize)]
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
                            stars: 1,
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
                        stars: 1,
                    },
                );
                true
            }
        }
    }

    pub fn stars(&self, id: &str) -> u32 {
        self.heroes.get(id).map_or(1, |h| h.stars)
    }

    pub fn team_levels(&self) -> Vec<(String, u32)> {
        self.team
            .iter()
            .map(|id| (id.clone(), self.level(id)))
            .collect()
    }

    pub fn team_members(&self) -> Vec<Member> {
        self.team
            .iter()
            .map(|id| Member {
                id: id.clone(),
                level: self.level(id),
                stars: self.stars(id),
            })
            .collect()
    }

    /// Что нужно для следующей звезды. Корм: сначала копии R-героев той же стихии, потом
    /// реже, потом лишние свои копии.
    pub fn merge_plan(
        &self,
        data: &GameData,
        crew: &CrewData,
        id: &str,
    ) -> Result<MergePlan, String> {
        let hero = data.hero(id).ok_or("нет такого героя")?;
        let stars = self.stars(id);
        if stars >= crew.max_stars() {
            return Err("Максимум звёзд".into());
        }
        let rule = crew.rule(stars + 1);
        let own = self.copies(id);
        if own < rule.copies {
            return Err(format!("Нужно копий героя: {}, есть {own}", rule.copies));
        }
        let mut spare = own - rule.copies;
        let mut need = rule.fodder;
        let mut others: Vec<(&String, Rarity, u32)> = data
            .heroes
            .iter()
            .filter(|h| h.id != id && h.element == hero.element)
            .filter_map(|h| {
                let copies = self.copies(&h.id);
                (copies > 0).then_some((&h.id, h.rarity, copies))
            })
            .collect();
        others.sort_by_key(|(_, rarity, copies)| (*rarity, u32::MAX - copies));
        let mut fodder = Vec::new();
        for (other, _, copies) in others {
            if need == 0 {
                break;
            }
            let take = copies.min(need);
            fodder.push((other.clone(), take));
            need -= take;
        }
        let from_spare = spare.min(need);
        spare -= from_spare;
        need -= from_spare;
        let _ = spare;
        if need > 0 {
            return Err(format!(
                "Не хватает корма: ещё {need} копий героев стихии «{}» или своих",
                element_label(hero.element)
            ));
        }
        Ok(MergePlan {
            to: stars + 1,
            own: rule.copies + from_spare,
            fodder,
        })
    }

    /// Провести слияние по плану.
    pub fn merge(&mut self, id: &str, plan: &MergePlan) {
        if let Some(hero) = self.heroes.get_mut(id) {
            hero.copies -= plan.own;
            hero.stars = plan.to;
        }
        for (other, n) in &plan.fodder {
            if let Some(h) = self.heroes.get_mut(other) {
                h.copies -= n;
            }
        }
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
                        assets,
                        fonts,
                        hero,
                        roster,
                        selected == Some(hero.id.as_str()),
                    );
                }
            });
            if let Some(hero) = selected.and_then(|id| data.hero(id)) {
                let plan = roster.merge_plan(data, crew, &hero.id);
                let merge = (
                    plan.as_ref().ok().map(|p| p.to),
                    plan.as_ref()
                        .map(|p| merge_line(p, data, hero))
                        .map_err(|e| e.clone()),
                );
                details(root, assets, fonts, crew, roster, profile, hero, merge);
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
    assets: &AssetServer,
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
        if owned {
            stars_row(c, assets, roster.stars(&hero.id), 12.0);
        }
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

#[allow(clippy::too_many_arguments)]
fn details(
    root: &mut ChildSpawnerCommands,
    assets: &AssetServer,
    fonts: &Fonts,
    crew: &CrewData,
    roster: &Roster,
    profile: &Profile,
    hero: &HeroDef,
    merge: (Option<u32>, Result<String, String>),
) {
    let (merge_to, merge_info) = merge;
    let level = roster.level(&hero.id);
    let cap = crew.level_cap(roster.stars(&hero.id));
    let at_cap = level >= cap;
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
                if owned {
                    stars_row(row, assets, roster.stars(&hero.id), 16.0);
                }
            });
        panel.spawn(text(
            fonts,
            format!(
                "{} · {} · стихия: {} · {}",
                rarity_label(hero.rarity),
                role_label(hero.role),
                element_label(hero.element),
                if owned {
                    format!("уровень {level} из {cap} · копий {copies}")
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

        panel.spawn(text(
            fonts,
            format!(
                "Умения: {} · {} (раз в {} хода) · {}",
                hero.basic.name, hero.skill.name, hero.skill.cooldown, hero.ultimate.name
            ),
            false,
            13.0,
            PARCHMENT,
        ));
        if owned {
            let (line, color) = match &merge_info {
                Ok(text) => (text.clone(), BETTER),
                Err(reason) => (reason.clone(), MUTED),
            };
            panel.spawn(text(fonts, line, false, 13.0, color));
        }

        panel
            .spawn(Node {
                margin: UiRect::top(px(10)),
                column_gap: px(10),
                ..default()
            })
            .with_children(|buttons| {
                if owned {
                    level_button(buttons, fonts, cost, at_cap, affordable, cap);
                    merge_button(buttons, fonts, merge_to);
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
    if level >= crew.level_cap(roster.stars(&id)) {
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

/// Звёзды героя картинками (в шрифте нет символа звезды).
fn stars_row(parent: &mut ChildSpawnerCommands, assets: &AssetServer, stars: u32, size: f32) {
    parent
        .spawn((
            Node {
                column_gap: px(1),
                ..default()
            },
            Pickable::IGNORE,
        ))
        .with_children(|row| {
            for i in 1..=6 {
                let icon = if i <= stars {
                    "ui/star.png"
                } else {
                    "ui/star_empty.png"
                };
                row.spawn((
                    Node {
                        width: px(size),
                        height: px(size),
                        ..default()
                    },
                    ImageNode::new(assets.load(icon)),
                    Pickable::IGNORE,
                ));
            }
        });
}

fn merge_line(plan: &MergePlan, data: &GameData, hero: &HeroDef) -> String {
    let mut parts = vec![format!("копии {} ×{}", hero.name, plan.own)];
    for (id, n) in &plan.fodder {
        let name = data.hero(id).map_or(id.as_str(), |h| h.name.as_str());
        parts.push(format!("копии {name} ×{n}"));
    }
    format!("Слияние до {} звёзд: уйдут {}", plan.to, parts.join(", "))
}

fn merge_button(buttons: &mut ChildSpawnerCommands, fonts: &Fonts, to: Option<u32>) {
    let ready = to.is_some();
    let mut button = buttons.spawn((
        Node {
            width: px(120),
            height: px(64),
            flex_direction: FlexDirection::Column,
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            border: UiRect::all(px(2)),
            border_radius: BorderRadius::all(px(12)),
            ..default()
        },
        BackgroundColor(if ready { STAR_FILL } else { DISABLED }),
        BorderColor::all(if ready { GOLD } else { MUTED }),
    ));
    button.with_children(|b| {
        b.spawn(text(fonts, "Слияние", true, 15.0, Color::WHITE));
        b.spawn(text(
            fonts,
            to.map_or("звезда".to_string(), |t| format!("до {t} звёзд")),
            false,
            12.0,
            PARCHMENT,
        ));
    });
    pressable(&mut button);
    button.observe(merge_hero);
}

/// Слить копии в звезду героя из карточки.
#[allow(clippy::too_many_arguments)]
fn merge_hero(
    _: On<PointerClick>,
    crew: Res<CrewData>,
    selected: Res<Selected>,
    screen: Res<BattleScreen>,
    mut roster: ResMut<Roster>,
    mut toast: ResMut<Toast>,
    mut dirty: ResMut<CrewDirty>,
    mut sfx: MessageWriter<Sfx>,
) {
    let Some(id) = selected.0.clone() else {
        return;
    };
    match roster.merge_plan(&screen.data, &crew, &id) {
        Err(reason) => toast.show(&reason),
        Ok(plan) => {
            roster.merge(&id, &plan);
            let name = screen
                .data
                .hero(&id)
                .map_or(id.as_str(), |h| h.name.as_str());
            toast.show(&format!(
                "{name}: {} звёзд! Потолок уровня {}",
                plan.to,
                crew.level_cap(plan.to)
            ));
            sfx.write(Sfx("ssr"));
            dirty.0 = true;
        }
    }
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
            width: px(120),
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
            "Из отряда"
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

    fn data() -> GameData {
        GameData::bundled().expect("данные корректны")
    }

    #[test]
    fn merge_needs_copies() {
        let (data, crew) = (data(), crew());
        let mut roster = Roster::new(&crew);
        assert!(roster.merge_plan(&data, &crew, "ren").is_err());
        roster.recruit("ren"); // копия
        let plan = roster
            .merge_plan(&data, &crew, "ren")
            .expect("до 2★ хватает одной копии");
        roster.merge("ren", &plan);
        assert_eq!(roster.stars("ren"), 2);
        assert_eq!(roster.copies("ren"), 0);
    }

    #[test]
    fn fodder_comes_from_same_element_first() {
        let (data, crew) = (data(), crew());
        let mut roster = Roster::new(&crew);
        // Мэй и Сора — ветер. Мэй до 2★, потом до 3★: 1 своя копия + 2 корма.
        roster.recruit("mei");
        let p = roster.merge_plan(&data, &crew, "mei").unwrap();
        roster.merge("mei", &p);
        roster.recruit("sora");
        roster.recruit("sora");
        roster.recruit("sora"); // у Соры 2 копии
        roster.recruit("mei");
        let plan = roster.merge_plan(&data, &crew, "mei").unwrap();
        assert_eq!(plan.own, 1);
        assert_eq!(plan.fodder, vec![("sora".to_string(), 2)]);
        roster.merge("mei", &plan);
        assert_eq!(roster.stars("mei"), 3);
        assert_eq!(roster.copies("sora"), 0);
    }

    #[test]
    fn spare_own_copies_replace_fodder() {
        let (data, crew) = (data(), crew());
        let mut roster = Roster::new(&crew);
        roster.recruit("borg");
        let p = roster.merge_plan(&data, &crew, "borg").unwrap();
        roster.merge("borg", &p);
        // У стали других героев нет: 3★ — 1 копия + 2 своих вместо корма.
        for _ in 0..3 {
            roster.recruit("borg");
        }
        let plan = roster.merge_plan(&data, &crew, "borg").unwrap();
        assert_eq!(plan.own, 3);
        assert!(plan.fodder.is_empty());
    }

    #[test]
    fn stars_make_hero_stronger() {
        let (data, crew) = (data(), crew());
        let ren = data.hero("ren").unwrap();
        let one = crew.hero_unit(ren, 10, 1);
        let three = crew.hero_unit(ren, 10, 3);
        assert!(three.stats.atk > one.stats.atk && three.stats.max_hp > one.stats.max_hp);
    }

    #[test]
    fn first_level_costs_base() {
        let crew = crew();
        let cost = crew.level_cost(1);
        assert_eq!(cost.journals, crew.journal_steps[0].1.round() as u32);
        assert_eq!(cost.silver, crew.silver_per_level);
    }

    #[test]
    fn each_level_costs_more() {
        let crew = crew();
        for level in 1..crew.level_cap(crew.max_stars()) {
            let (a, b) = (crew.level_cost(level), crew.level_cost(level + 1));
            assert!(
                b.journals > a.journals && b.silver > a.silver,
                "уровень {level}"
            );
        }
    }
}
