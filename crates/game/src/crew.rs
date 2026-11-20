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
use crate::cabins::{CabinData, Cabins};
use crate::contracts::Deed;
use crate::gear::{Gear, GearData};
use crate::menu::{
    GOLD, MUTED, PARCHMENT, Profile, Toast, bottom_nav, pressable, spawn_toast, text,
};
use crate::style::{self, INK, INK_GOOD, INK_SOFT, Look, Skin};
use crate::view::Fonts;

const CARD_W: f32 = 164.0;
const CARD_H: f32 = 150.0;
const PORTRAIT: f32 = 86.0;
/// Верх панели героя: под сеткой карточек.
const DETAILS_TOP: f32 = 392.0;
const BRIGHT: Color = Color::srgb(1.0, 0.93, 0.78);
const BETTER: Color = Color::srgb(0.55, 0.88, 0.55);

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
    pub skills: SkillRules,
}

/// Умения: атака, спецатака, ульта — уровни 1–10.
#[derive(Deserialize, Clone)]
pub struct SkillRules {
    /// Прибавка силы за уровень: атака, спецатака, ульта.
    pub bonus_per_level: [f32; 3],
    /// На этих уровнях спецатака перезаряжается на ход быстрее.
    pub cooldown_cut_at: Vec<u32>,
    /// Лоций за переход с уровня N на N+1 (индекс N−1).
    pub charts: Vec<u32>,
    pub silver_per_level: u32,
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
    /// Потолок уровня умений.
    pub skill_cap: u32,
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

    pub fn skill_cap(&self, stars: u32) -> u32 {
        self.rule(stars).skill_cap
    }

    /// Цена умения с уровня `level` на следующий: лоции и серебро.
    pub fn skill_cost(&self, level: u32) -> (u32, u32) {
        let charts = self
            .skills
            .charts
            .get(level.saturating_sub(1) as usize)
            .or(self.skills.charts.last())
            .copied()
            .unwrap_or(1);
        (charts, self.skills.silver_per_level * level)
    }

    /// Боец для боя: уровень, затем звёзды — прибавка к характеристикам, ульта, перезарядка.
    pub fn hero_unit(&self, hero: &HeroDef, level: u32, stars: u32, skills: [u32; 3]) -> UnitDef {
        let mut unit = hero.to_unit(level);
        let power =
            |i: usize| 1.0 + self.skills.bonus_per_level[i] * skills[i].saturating_sub(1) as f32;
        scale_ability(&mut unit.basic, power(0));
        if let Some(skill) = &mut unit.skill {
            scale_ability(skill, power(1));
            let cuts = self
                .skills
                .cooldown_cut_at
                .iter()
                .filter(|l| skills[1] >= **l)
                .count() as u32;
            skill.cooldown = skill
                .cooldown
                .saturating_sub(cuts)
                .max(self.skill_min_cooldown);
        }
        if let Some(ult) = &mut unit.ultimate {
            scale_ability(ult, power(2));
        }
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

/// Сила урона и лечения умения растёт в `k` раз.
fn scale_ability(ability: &mut battle_core::Ability, k: f32) {
    for e in &mut ability.effects {
        match &mut e.effect {
            Effect::Damage { multiplier } => *multiplier *= k,
            Effect::Heal { percent } => *percent *= k,
            _ => {}
        }
    }
}

/// Нанятый герой: уровень, звёзды, умения и лишние копии (идут на слияние).
#[derive(Clone, Copy, Serialize, Deserialize)]
struct Owned {
    level: u32,
    copies: u32,
    #[serde(default = "first_star")]
    stars: u32,
    /// Уровни умений: атака, спецатака, ульта.
    #[serde(default = "first_skills")]
    skills: [u32; 3],
}

fn first_skills() -> [u32; 3] {
    [1, 1, 1]
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
    pub skills: [u32; 3],
    /// Прибавка к атаке от соседа по каюте в отряде.
    pub atk_bonus: f32,
    /// Прибавка от надетого снаряжения.
    pub gear: battle_core::Stats,
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
                            skills: [1, 1, 1],
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
                        skills: [1, 1, 1],
                    },
                );
                true
            }
        }
    }

    pub fn stars(&self, id: &str) -> u32 {
        self.heroes.get(id).map_or(1, |h| h.stars)
    }

    pub fn skills(&self, id: &str) -> [u32; 3] {
        self.heroes.get(id).map_or([1, 1, 1], |h| h.skills)
    }

    fn raise_skill(&mut self, id: &str, i: usize) {
        if let Some(h) = self.heroes.get_mut(id) {
            h.skills[i] += 1;
        }
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
                skills: self.skills(id),
                atk_bonus: 0.0,
                gear: battle_core::Stats {
                    max_hp: 0,
                    atk: 0,
                    def: 0,
                    spd: 0,
                },
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
        .init_resource::<CrewTab>()
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
    cabin: CabinView,
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
        &cabin,
    );
}

/// Всё про каюты для карточки героя.
#[derive(bevy::ecs::system::SystemParam)]
struct CabinView<'w> {
    cabins: Res<'w, Cabins>,
    data: Res<'w, CabinData>,
    ship: Res<'w, crate::ship::Ship>,
    ship_data: Res<'w, crate::ship::ShipData>,
    gear: Res<'w, Gear>,
    gear_data: Res<'w, GearData>,
    tab: Res<'w, CrewTab>,
}

/// Вкладка карточки героя.
#[derive(Resource, Default, Clone, Copy, PartialEq, Eq)]
enum CrewTab {
    #[default]
    Hero,
    Gear,
}

/// Надетый предмет: id, описание, цена усиления (серебро, детали), достиг ли предела.
type WornItem = (u32, String, (u32, u32), bool);

/// Снаряжение героя для вкладки.
struct GearInfo {
    tab: CrewTab,
    /// По слотам: имя слота и надетый предмет.
    slots: Vec<(String, Option<WornItem>)>,
    details: u32,
    stock: usize,
}

fn gear_info(view: &CabinView, profile: &Profile, hero: &str) -> GearInfo {
    let worn = view.gear.worn_by(hero);
    let data = &view.gear_data;
    GearInfo {
        tab: *view.tab,
        slots: data
            .slots
            .iter()
            .enumerate()
            .map(|(i, slot)| {
                let item = worn[i].and_then(|id| view.gear.item(id)).map(|item| {
                    (
                        item.id,
                        data.describe(item),
                        data.enhance_cost(item),
                        item.level >= data.enhance.max,
                    )
                });
                (slot.name.clone(), item)
            })
            .collect(),
        details: profile.details,
        stock: view.gear.items.len(),
    }
}

/// Что показать в разделе «Каюта» у героя.
enum CabinInfo {
    Locked,
    Paired {
        partner: String,
        level: u32,
        max: u32,
        bonus: u32,
        talked: bool,
    },
    Free {
        free: u32,
        total: u32,
        candidates: Vec<(String, String)>,
    },
}

fn cabin_info(view: &CabinView, roster: &Roster, data: &GameData, hero: &str) -> CabinInfo {
    let total = view.ship.cabins(&view.ship_data);
    if total == 0 {
        return CabinInfo::Locked;
    }
    let name = |id: &str| data.hero(id).map_or(id.to_string(), |h| h.name.clone());
    match view.cabins.pair_of(hero) {
        Some(p) => CabinInfo::Paired {
            partner: name(p.partner(hero)),
            level: view.data.level(p.points),
            max: view.data.max_level,
            bonus: (view.data.bonus(p.points) * 100.0).round() as u32,
            talked: p.talked_on == crate::cabins::today(),
        },
        None => CabinInfo::Free {
            free: total.saturating_sub(view.cabins.pairs.len() as u32),
            total,
            candidates: data
                .heroes
                .iter()
                .filter(|h| {
                    h.id != hero && roster.owns(&h.id) && view.cabins.pair_of(&h.id).is_none()
                })
                .map(|h| (h.id.clone(), h.name.clone()))
                .collect(),
        },
    }
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
    cabin: CabinView,
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
        &cabin,
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
    cabin: &CabinView,
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
            BackgroundColor(Color::srgb(0.10, 0.07, 0.05)),
        ))
        .with_children(|root| {
            // Палуба «Ласточки» под тёмной вуалью — экипаж живёт на корабле.
            style::backdrop(root, assets, "battle/deck.jpg", 0.55);
            style::header(
                root,
                assets,
                fonts,
                &format!("Экипаж · отряд {}/{}", roster.team.len(), crew.team_size),
                &[
                    ("journals", profile.journals),
                    ("charts", profile.charts),
                    ("silver", profile.silver),
                ],
            );
            root.spawn(Node {
                position_type: PositionType::Absolute,
                top: px(70),
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
                let gear = gear_info(cabin, profile, &hero.id);
                let cabin = cabin_info(cabin, roster, data, &hero.id);
                details(
                    root, assets, fonts, crew, roster, profile, hero, merge, cabin, gear,
                );
            }
            bottom_nav(root, assets, fonts, Screen::Crew);
            spawn_toast(root, fonts);
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
            padding: UiRect::top(px(10)),
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::Center,
            row_gap: px(2),
            border_radius: BorderRadius::all(px(18)),
            ..default()
        },
        Skin(Look::Wood),
    ));
    if selected {
        card.insert(Outline::new(px(3), px(1), GOLD));
    }
    card.with_children(|c| {
        c.spawn(style::medallion(assets, &hero.id, PORTRAIT, !owned));
        c.spawn(text(fonts, hero.name.clone(), true, 16.0, BRIGHT));
        if owned {
            stars_row(c, assets, roster.stars(&hero.id), 12.0);
            c.spawn(text(
                fonts,
                format!("ур. {}", roster.level(&hero.id)),
                false,
                12.0,
                rarity_color(hero.rarity),
            ));
        } else {
            c.spawn(text(fonts, "в Таверне", false, 12.0, MUTED));
        }
        c.spawn((
            Node {
                position_type: PositionType::Absolute,
                top: px(8),
                right: px(10),
                ..default()
            },
            text(
                fonts,
                rarity_label(hero.rarity),
                true,
                13.0,
                rarity_color(hero.rarity),
            ),
        ));
        if in_team {
            c.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    top: px(8),
                    left: px(8),
                    padding: UiRect::axes(px(8), px(3)),
                    ..default()
                },
                Skin(Look::Go),
                Pickable::IGNORE,
            ))
            .with_child((
                Text::new("в отряде"),
                fonts.text(true, 11.0),
                TextColor(Color::WHITE),
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
    cabin: CabinInfo,
    gear: GearInfo,
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

    let show_gear = owned && gear.tab == CrewTab::Gear;
    root.spawn((
        Node {
            position_type: PositionType::Absolute,
            top: px(DETAILS_TOP),
            left: px(8),
            right: px(8),
            padding: UiRect::axes(px(28), px(22)),
            flex_direction: FlexDirection::Column,
            row_gap: px(7),
            ..default()
        },
        Skin(Look::Wood),
    ))
    .with_children(|panel| {
        panel
            .spawn(Node {
                column_gap: px(8),
                align_items: AlignItems::Center,
                ..default()
            })
            .with_children(|row| {
                row.spawn(text(fonts, hero.name.clone(), true, 22.0, BRIGHT));
                row.spawn(text(fonts, hero.title.clone(), false, 14.0, MUTED));
                if owned {
                    stars_row(row, assets, roster.stars(&hero.id), 16.0);
                }
            });
        if show_gear {
            tabs_row(panel, fonts, gear.tab);
            gear_tab(panel, fonts, gear);
            return;
        }
        // Слева — вкладки, сведения и характеристики, справа — герой во весь рост.
        panel
            .spawn(Node {
                column_gap: px(10),
                ..default()
            })
            .with_children(|top| {
                top.spawn(Node {
                    flex_grow: 1.0,
                    flex_direction: FlexDirection::Column,
                    row_gap: px(7),
                    ..default()
                })
                .with_children(|left| {
                    if owned {
                        tabs_row(left, fonts, gear.tab);
                    }
                    left.spawn(text(
                        fonts,
                        format!(
                            "{} · {} · {}",
                            rarity_label(hero.rarity),
                            role_label(hero.role),
                            element_label(hero.element),
                        ),
                        false,
                        13.0,
                        PARCHMENT,
                    ));
                    left.spawn(text(
                        fonts,
                        if owned {
                            format!("Уровень {level} из {cap} · копий {copies}")
                        } else {
                            "Ещё не в команде".to_string()
                        },
                        true,
                        13.0,
                        rarity_color(hero.rarity),
                    ));
                    stats_card(left, fonts, now, next, owned && !at_cap);
                });
                top.spawn((
                    Node {
                        width: px(120),
                        height: px(178),
                        flex_shrink: 0.0,
                        ..default()
                    },
                    ImageNode {
                        image: assets.load(format!("ui/full_{}.png", hero.id)),
                        image_mode: NodeImageMode::Auto,
                        color: if owned {
                            Color::WHITE
                        } else {
                            Color::srgba(0.4, 0.4, 0.45, 0.8)
                        },
                        ..default()
                    },
                    Pickable::IGNORE,
                ));
            });

        skills_row(panel, assets, fonts, crew, roster, profile, hero, owned);
        if owned {
            let (line, color) = match &merge_info {
                Ok(text) => (text.clone(), BETTER),
                Err(reason) => (reason.clone(), MUTED),
            };
            panel.spawn(text(fonts, line, false, 12.0, color));
        }

        panel
            .spawn(Node {
                margin: UiRect::top(px(2)),
                column_gap: px(8),
                ..default()
            })
            .with_children(|buttons| {
                if owned {
                    level_button(buttons, assets, fonts, cost, at_cap, affordable, cap);
                    merge_button(buttons, fonts, merge_to);
                    team_button(buttons, fonts, &hero.id, in_team);
                } else {
                    to_tavern_button(buttons, fonts);
                }
            });
        if owned {
            cabin_row(panel, fonts, cabin);
        }
    });
}

/// Характеристики сейчас и на следующем уровне — на пергаменте.
fn stats_card(
    parent: &mut ChildSpawnerCommands,
    fonts: &Fonts,
    now: battle_core::Stats,
    next: battle_core::Stats,
    growing: bool,
) {
    parent
        .spawn((
            Node {
                padding: UiRect::axes(px(16), px(12)),
                flex_wrap: FlexWrap::Wrap,
                row_gap: px(4),
                ..default()
            },
            Skin(Look::Parchment),
        ))
        .with_children(|stats| {
            for (name, a, b) in [
                ("Здоровье", now.max_hp, next.max_hp),
                ("Атака", now.atk, next.atk),
                ("Защита", now.def, next.def),
                ("Скорость", now.spd, next.spd),
            ] {
                stats
                    .spawn(Node {
                        width: percent(50),
                        flex_direction: FlexDirection::Column,
                        ..default()
                    })
                    .with_children(|cell| {
                        cell.spawn(text(fonts, name, false, 12.0, INK_SOFT));
                        cell.spawn(Node {
                            column_gap: px(6),
                            align_items: AlignItems::Baseline,
                            ..default()
                        })
                        .with_children(|row| {
                            row.spawn(text(fonts, a.to_string(), true, 17.0, INK));
                            if growing && b > a {
                                row.spawn(text(fonts, format!("+{}", b - a), true, 13.0, INK_GOOD));
                            }
                        });
                    });
            }
        });
}

fn tabs_row(panel: &mut ChildSpawnerCommands, fonts: &Fonts, tab: CrewTab) {
    panel
        .spawn(Node {
            column_gap: px(6),
            ..default()
        })
        .with_children(|row| {
            for (label, t) in [("Герой", CrewTab::Hero), ("Снаряжение", CrewTab::Gear)]
            {
                let active = t == tab;
                let mut button = row.spawn((
                    Node {
                        min_height: px(34),
                        padding: UiRect::axes(px(16), px(6)),
                        align_items: AlignItems::Center,
                        ..default()
                    },
                    Skin(if active { Look::TabOn } else { Look::TabOff }),
                ));
                button.with_child(text(
                    fonts,
                    label,
                    true,
                    13.0,
                    if active { Color::WHITE } else { MUTED },
                ));
                pressable(&mut button);
                button.observe(
                    move |_: On<PointerClick>,
                          mut tab: ResMut<CrewTab>,
                          mut dirty: ResMut<CrewDirty>| {
                        *tab = t;
                        dirty.0 = true;
                    },
                );
            }
        });
}

/// Вкладка «Снаряжение»: четыре слота, усиление, «Надеть лучшее» и «Снять всё».
fn gear_tab(panel: &mut ChildSpawnerCommands, fonts: &Fonts, info: GearInfo) {
    panel.spawn(text(
        fonts,
        format!(
            "Деталей {} · предметов на складе {} · создать — в Мастерской на корабле",
            info.details, info.stock
        ),
        false,
        12.0,
        MUTED,
    ));
    for (name, item) in info.slots {
        panel
            .spawn((
                Node {
                    padding: UiRect::axes(px(12), px(6)),
                    column_gap: px(8),
                    align_items: AlignItems::Center,
                    ..default()
                },
                Skin(Look::Parchment),
            ))
            .with_children(|row| {
                row.spawn(Node {
                    flex_grow: 1.0,
                    flex_shrink: 1.0,
                    flex_direction: FlexDirection::Column,
                    ..default()
                })
                .with_children(|c| {
                    c.spawn(text(fonts, name, true, 13.0, INK));
                    let line = item
                        .as_ref()
                        .map_or("пусто".to_string(), |(_, d, _, _)| d.clone());
                    c.spawn(text(
                        fonts,
                        line,
                        false,
                        12.0,
                        if item.is_some() { INK_GOOD } else { INK_SOFT },
                    ));
                });
                if let Some((id, _, (silver, details), maxed)) = item {
                    let label = if maxed {
                        "предел".to_string()
                    } else {
                        format!("+1: {silver} сер., {details} дет.")
                    };
                    gear_hero_button(row, fonts, &label, !maxed, HeroGear::Enhance(id));
                }
            });
    }
    panel
        .spawn(Node {
            margin: UiRect::top(px(4)),
            column_gap: px(8),
            ..default()
        })
        .with_children(|row| {
            gear_hero_button(row, fonts, "Надеть лучшее", true, HeroGear::Best);
            gear_hero_button(row, fonts, "Снять всё", true, HeroGear::Clear);
        });
}

#[derive(Clone, Copy)]
enum HeroGear {
    Best,
    Clear,
    Enhance(u32),
}

fn gear_hero_button(
    row: &mut ChildSpawnerCommands,
    fonts: &Fonts,
    label: &str,
    active: bool,
    act: HeroGear,
) {
    let mut button = row.spawn((
        Node {
            min_height: px(34),
            padding: UiRect::axes(px(12), px(6)),
            align_items: AlignItems::Center,
            ..default()
        },
        Skin(if active { Look::Go } else { Look::Off }),
    ));
    button.with_child(text(fonts, label, true, 12.0, Color::WHITE));
    if active {
        pressable(&mut button);
        button.observe(move |_: On<PointerClick>, mut commands: Commands| {
            commands.run_system_cached_with(hero_gear_act, act);
        });
    }
}

#[allow(clippy::too_many_arguments)]
fn hero_gear_act(
    In(act): In<HeroGear>,
    selected: Res<Selected>,
    data: Res<GearData>,
    mut gear: ResMut<Gear>,
    mut profile: ResMut<Profile>,
    mut toast: ResMut<Toast>,
    mut dirty: ResMut<CrewDirty>,
    mut sfx: MessageWriter<Sfx>,
) {
    let Some(hero) = selected.0.clone() else {
        return;
    };
    match act {
        HeroGear::Best => {
            gear.equip_best(&hero, &data);
            if gear.worn_by(&hero).iter().all(|s| s.is_none()) {
                toast.show("Надеть нечего — создайте снаряжение в Мастерской");
            } else {
                sfx.write(Sfx("done"));
            }
        }
        HeroGear::Clear => gear.unequip_all(&hero),
        HeroGear::Enhance(id) => {
            let Some(item) = gear.item(id) else {
                return;
            };
            let (silver, details) = data.enhance_cost(item);
            if profile.silver < silver || profile.details < details {
                toast.show(&format!("Нужно {silver} серебра и {details} деталей"));
                return;
            }
            if gear.enhance(id, &data) {
                profile.silver -= silver;
                profile.details -= details;
                sfx.write(Sfx("hammer"));
            }
        }
    }
    dirty.0 = true;
}

/// Раздел «Каюта»: сосед, близость и бонус, или кого поселить рядом.
fn cabin_row(panel: &mut ChildSpawnerCommands, fonts: &Fonts, info: CabinInfo) {
    panel
        .spawn(Node {
            margin: UiRect::top(px(4)),
            column_gap: px(6),
            row_gap: px(4),
            flex_wrap: FlexWrap::Wrap,
            align_items: AlignItems::Center,
            ..default()
        })
        .with_children(|row| match info {
            CabinInfo::Locked => {
                row.spawn(text(
                    fonts,
                    "Каюты откроются на Корпусе 4: соседи в одном отряде сильнее",
                    false,
                    12.0,
                    MUTED,
                ));
            }
            CabinInfo::Paired {
                partner,
                level,
                max,
                bonus,
                talked,
            } => {
                row.spawn(text(
                    fonts,
                    format!("Сосед по каюте: {partner} · близость {level}/{max} · +{bonus}% атаки вместе"),
                    false,
                    12.0,
                    BETTER,
                ));
                cabin_button(row, fonts, if talked { "сегодня уже говорили" } else { "Поговорить" }, !talked, CabinAct::Talk);
                cabin_button(row, fonts, "Выселить", true, CabinAct::Evict);
            }
            CabinInfo::Free {
                free,
                total,
                candidates,
            } => {
                row.spawn(text(
                    fonts,
                    format!("Без каюты · свободно {free} из {total} · поселить с:"),
                    false,
                    12.0,
                    MUTED,
                ));
                if free > 0 {
                    for (id, name) in candidates {
                        cabin_button(row, fonts, &name, true, CabinAct::Settle(id));
                    }
                }
            }
        });
}

#[derive(Clone)]
enum CabinAct {
    Talk,
    Evict,
    Settle(String),
}

fn cabin_button(
    row: &mut ChildSpawnerCommands,
    fonts: &Fonts,
    label: &str,
    active: bool,
    act: CabinAct,
) {
    let mut button = row.spawn((
        Node {
            min_height: px(34),
            padding: UiRect::axes(px(12), px(6)),
            align_items: AlignItems::Center,
            ..default()
        },
        Skin(if active { Look::Go } else { Look::Off }),
    ));
    button.with_child(text(fonts, label, true, 12.0, Color::WHITE));
    if active {
        pressable(&mut button);
        button.observe(move |_: On<PointerClick>, mut commands: Commands| {
            commands.run_system_cached_with(cabin_act, act.clone());
        });
    }
}

#[allow(clippy::too_many_arguments)]
fn cabin_act(
    In(act): In<CabinAct>,
    selected: Res<Selected>,
    data: Res<CabinData>,
    ship: (Res<crate::ship::Ship>, Res<crate::ship::ShipData>),
    mut cabins: ResMut<Cabins>,
    mut toast: ResMut<Toast>,
    mut dirty: ResMut<CrewDirty>,
    mut sfx: MessageWriter<Sfx>,
) {
    let Some(hero) = selected.0.clone() else {
        return;
    };
    match act {
        CabinAct::Talk => {
            if cabins.talk(&hero, &data, crate::cabins::today()) {
                toast.show("Разговор по душам: соседи стали ближе");
                sfx.write(Sfx("coin"));
            }
        }
        CabinAct::Evict => cabins.evict(&hero),
        CabinAct::Settle(partner) => match cabins.settle(&hero, &partner, ship.0.cabins(&ship.1)) {
            Ok(()) => {
                toast.show("Соседи заселились в каюту");
                sfx.write(Sfx("done"));
            }
            Err(reason) => toast.show(&reason),
        },
    }
    dirty.0 = true;
}

fn level_button(
    buttons: &mut ChildSpawnerCommands,
    assets: &AssetServer,
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
            height: px(60),
            flex_direction: FlexDirection::Column,
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            row_gap: px(2),
            ..default()
        },
        Skin(if ready { Look::Go } else { Look::Off }),
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
            b.spawn(text(fonts, "Повысить уровень", true, 17.0, Color::WHITE));
            style::costs(
                b,
                assets,
                fonts,
                &[("journals", cost.journals), ("silver", cost.silver)],
                12.0,
                Color::WHITE,
            );
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
    mut deeds: MessageWriter<Deed>,
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
    deeds.write(Deed("levelup"));
    dirty.0 = true;
}

/// Три умения героя: уровень, потолок от звёзд, цена следующего уровня; тап — повысить.
#[allow(clippy::too_many_arguments)]
fn skills_row(
    panel: &mut ChildSpawnerCommands,
    assets: &AssetServer,
    fonts: &Fonts,
    crew: &CrewData,
    roster: &Roster,
    profile: &Profile,
    hero: &HeroDef,
    owned: bool,
) {
    let levels = roster.skills(&hero.id);
    let cap = crew.skill_cap(roster.stars(&hero.id));
    let names = [&hero.basic.name, &hero.skill.name, &hero.ultimate.name];
    let kinds = ["атака", "спецатака", "ульта"];
    panel
        .spawn(Node {
            column_gap: px(8),
            ..default()
        })
        .with_children(|row| {
            for i in 0..3 {
                let level = levels[i];
                let (charts, silver) = crew.skill_cost(level);
                let can =
                    owned && level < cap && profile.charts >= charts && profile.silver >= silver;
                let sub = if !owned {
                    kinds[i].to_string()
                } else if level >= cap {
                    format!("ур. {level} · потолок")
                } else {
                    format!("ур. {level}")
                };
                let mut chip = row.spawn((
                    Node {
                        flex_grow: 1.0,
                        flex_basis: px(0),
                        padding: UiRect::axes(px(6), px(6)),
                        flex_direction: FlexDirection::Column,
                        align_items: AlignItems::Center,
                        row_gap: px(1),
                        border_radius: BorderRadius::all(px(12)),
                        ..default()
                    },
                    Skin(Look::Parchment),
                ));
                if can {
                    chip.insert(Outline::new(px(2), px(0), GOLD));
                }
                chip.with_children(|c| {
                    c.spawn((
                        text(fonts, names[i].clone(), true, 12.0, INK),
                        TextLayout::justify(Justify::Center),
                    ));
                    c.spawn(text(fonts, sub, false, 11.0, INK_SOFT));
                    if owned && level < cap {
                        style::costs(
                            c,
                            assets,
                            fonts,
                            &[("charts", charts), ("silver", silver)],
                            11.0,
                            if can { INK_GOOD } else { INK_SOFT },
                        );
                    }
                });
                if owned {
                    pressable(&mut chip);
                    chip.observe(move |_: On<PointerClick>, mut commands: Commands| {
                        commands.run_system_cached_with(upgrade_skill, i);
                    });
                }
            }
        });
}

/// Повысить умение героя из карточки за лоции и серебро.
#[allow(clippy::too_many_arguments)]
fn upgrade_skill(
    In(i): In<usize>,
    crew: Res<CrewData>,
    selected: Res<Selected>,
    screen: Res<BattleScreen>,
    mut roster: ResMut<Roster>,
    mut profile: ResMut<Profile>,
    mut toast: ResMut<Toast>,
    mut dirty: ResMut<CrewDirty>,
    mut sfx: MessageWriter<Sfx>,
) {
    let Some(id) = selected.0.clone() else {
        return;
    };
    let level = roster.skills(&id)[i];
    let cap = crew.skill_cap(roster.stars(&id));
    if level >= cap {
        toast.show("Потолок умений — поднимите звёзды героя");
        return;
    }
    let (charts, silver) = crew.skill_cost(level);
    if profile.charts < charts || profile.silver < silver {
        toast.show(&format!(
            "Нужно {charts} лоций и {silver} серебра. Лоции дают Бездна облаков и контракты"
        ));
        return;
    }
    profile.charts -= charts;
    profile.silver -= silver;
    roster.raise_skill(&id, i);
    let hero = screen.data.hero(&id);
    let name = hero.map_or("", |h| match i {
        0 => h.basic.name.as_str(),
        1 => h.skill.name.as_str(),
        _ => h.ultimate.name.as_str(),
    });
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
            width: px(112),
            height: px(60),
            flex_direction: FlexDirection::Column,
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            ..default()
        },
        Skin(if ready { Look::Go } else { Look::Off }),
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
            height: px(60),
            flex_direction: FlexDirection::Column,
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            ..default()
        },
        Skin(Look::Go),
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
            width: px(112),
            height: px(60),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            ..default()
        },
        Skin(Look::Dark),
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
              exps: Res<crate::expeditions::Expeditions>,
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
            } else if exps.is_away(&id) {
                toast.show("Герой в экспедиции — вернётся, тогда возьмём в отряд");
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
        let one = crew.hero_unit(ren, 10, 1, [1, 1, 1]);
        let three = crew.hero_unit(ren, 10, 3, [1, 1, 1]);
        assert!(three.stats.atk > one.stats.atk && three.stats.max_hp > one.stats.max_hp);
    }

    #[test]
    fn skills_raise_power_and_cut_cooldown() {
        let (data, crew) = (data(), crew());
        let ren = data.hero("ren").unwrap();
        let base = crew.hero_unit(ren, 1, 1, [1, 1, 1]);
        let up = crew.hero_unit(ren, 1, 1, [3, 5, 2]);
        let dmg = |u: &UnitDef| match u.basic.effects[0].effect {
            Effect::Damage { multiplier } => multiplier,
            _ => 0.0,
        };
        assert!(dmg(&up) > dmg(&base));
        let cd = |u: &UnitDef| u.skill.as_ref().unwrap().cooldown;
        assert_eq!(cd(&up), cd(&base) - 1);
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
