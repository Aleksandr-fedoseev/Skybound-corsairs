//! Экипаж: герои на борту, их уровни и отряд из четырёх на бой.
//!
//! Уровень покупается за судовые журналы и серебро — так добыча из боёв превращается в силу.
//! Потолок уровня пока один для всех (1★); звёзды и слияние будут позже.

use std::collections::HashMap;

use battle_core::{Effect, Element, Role, UnitDef};
use bevy::prelude::*;
use game_data::{GameData, HeroDef};
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

/// Медальон героя в ленте отряда.
const CHIP: f32 = 60.0;
/// Иконка умения в латунном кольце.
const SKILL_RING: f32 = 76.0;
/// Верх области героя: под заголовком экрана.
const HERO_TOP: f32 = 108.0;
const BRIGHT: Color = Color::srgb(1.0, 0.93, 0.78);
const BETTER: Color = Color::srgb(0.55, 0.88, 0.55);

/// Настройки экипажа: `data/crew.json`.
#[derive(Resource, Deserialize, Clone)]
pub struct CrewData {
    pub start_team: Vec<String>,
    /// Кто за какую главу приходит в команду (решение №76); вступает он сценой после босса.
    #[serde(default)]
    pub arrivals: HashMap<String, u32>,
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

    pub fn rule(&self, star: u32) -> StarRule {
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

    /// Герои в команде, по алфавиту id (порядок не зависит от HashMap).
    pub fn owned(&self) -> Vec<String> {
        let mut ids: Vec<String> = self.heroes.keys().cloned().collect();
        ids.sort();
        ids
    }

    /// Копий хватает на следующую звезду (корм для 3★+ проверяет `merge_plan`).
    pub fn star_copies_ready(&self, id: &str, crew: &CrewData) -> bool {
        let stars = self.stars(id);
        stars < crew.max_stars() && self.copies(id) >= crew.rule(stars + 1).copies
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
        let mut others: Vec<(&String, u32)> = data
            .heroes
            .iter()
            .filter(|h| h.id != id && h.element == hero.element)
            .filter_map(|h| {
                let copies = self.copies(&h.id);
                (copies > 0).then_some((&h.id, copies))
            })
            .collect();
        others.sort_by_key(|(id, copies)| (u32::MAX - copies, (*id).clone()));
        let mut fodder = Vec::new();
        for (other, copies) in others {
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
pub(crate) struct Selected(pub Option<String>);

/// Экран надо собрать заново: уровень вырос или отряд изменился.
#[derive(Resource, Default)]
struct CrewDirty(bool);

/// Экран расстановки открыт; `slot` — выбранное место в отряде (0–3).
#[derive(Resource, Default)]
struct Formation {
    open: bool,
    slot: Option<usize>,
}

#[derive(Component)]
struct CrewRoot;

pub fn plugin(app: &mut App) {
    app.init_resource::<Selected>()
        .init_resource::<CrewDirty>()
        .init_resource::<Formation>()
        .init_resource::<Dragging>()
        .init_resource::<CrewTab>()
        .add_systems(OnEnter(Screen::Crew), spawn_crew)
        .add_systems(OnExit(Screen::Crew), |mut f: ResMut<Formation>| {
            *f = Formation::default()
        })
        .add_systems(Update, rebuild_crew.run_if(in_state(Screen::Crew)));
}

pub(crate) fn role_label(role: Role) -> &'static str {
    match role {
        Role::Gunner => "Канонир",
        Role::Duelist => "Дуэлянт",
        Role::Boarder => "Абордажник",
        Role::Navigator => "Штурман",
        Role::Medic => "Корабельный врач",
        Role::WindShaman => "Шаманка ветров",
    }
}

pub(crate) fn element_label(element: Element) -> &'static str {
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
    power: crate::power::Power,
) {
    // Экипаж открывается сеткой героев; страница героя — по тапу.
    selected.0 = None;
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
        &cabin.formation,
        &|id: &str| cabin.exps.is_away(id),
        &PowerInfo::new(&power, &roster),
    );
}

/// Мощь для экрана Экипажа: отряд с кораблём, корабль и каждый нанятый герой.
struct PowerInfo {
    team: u32,
    ship: u32,
    heroes: HashMap<String, u32>,
}

impl PowerInfo {
    fn new(power: &crate::power::Power, roster: &Roster) -> Self {
        Self {
            team: power.team(),
            ship: power.ship(),
            heroes: roster
                .owned()
                .into_iter()
                .map(|id| {
                    let p = power.hero(&id);
                    (id, p)
                })
                .collect(),
        }
    }

    fn hero(&self, id: &str) -> u32 {
        self.heroes.get(id).copied().unwrap_or(0)
    }
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
    formation: Res<'w, Formation>,
    exps: Res<'w, crate::expeditions::Expeditions>,
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
    power: crate::power::Power,
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
        &cabin.formation,
        &|id: &str| cabin.exps.is_away(id),
        &PowerInfo::new(&power, &roster),
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
    formation: &Formation,
    away: &dyn Fn(&str) -> bool,
    power: &PowerInfo,
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
        ))
        .with_children(|root| {
            style::top_bar(
                root,
                assets,
                fonts,
                &[
                    ("journals", profile.journals),
                    ("charts", profile.charts),
                    ("silver", profile.silver),
                ],
            );
            if formation.open {
                formation_view(root, assets, fonts, data, roster, formation.slot, away);
                bottom_nav(root, assets, fonts, Screen::Crew);
                spawn_toast(root, fonts);
                return;
            }
            let Some(hero) = selected.and_then(|id| data.hero(id)) else {
                // Сетка героев и отряд в бой.
                style::title(
                    root,
                    fonts,
                    &format!("Экипаж · отряд {}/{}", roster.team.len(), crew.team_size),
                    56.0,
                );
                hero_grid(root, assets, fonts, data, roster, crew, power);
                crate::help::corner(root, fonts, "crew", 60.0, 12.0);
                bottom_nav(root, assets, fonts, Screen::Crew);
                spawn_toast(root, fonts);
                return;
            };
            style::title(root, fonts, "Экипаж", 56.0);
            crate::help::corner(root, fonts, "hero", 60.0, 12.0);
            back_button(root, fonts);
            {
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
                    root,
                    assets,
                    fonts,
                    crew,
                    roster,
                    profile,
                    hero,
                    merge,
                    cabin,
                    gear,
                    power.hero(&hero.id),
                );
            }
            // Лента героев над нижней панелью: тап — выбрать.
            root.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    bottom: px(86),
                    left: px(6),
                    right: px(6),
                    height: px(80),
                    padding: UiRect::horizontal(px(14)),
                    justify_content: JustifyContent::SpaceBetween,
                    align_items: AlignItems::Center,
                    ..default()
                },
                Skin(Look::Tile),
            ))
            .with_children(|rail| {
                for hero in data.heroes.iter().filter(|h| roster.owns(&h.id)) {
                    roster_chip(
                        rail,
                        assets,
                        hero,
                        roster,
                        selected == Some(hero.id.as_str()),
                    );
                }
            });
            bottom_nav(root, assets, fonts, Screen::Crew);
            spawn_toast(root, fonts);
        });
}

/// Сетка героев: крупные карточки как плитки отсеков, под ней — отряд в бой.
#[allow(clippy::too_many_arguments)]
fn hero_grid(
    root: &mut ChildSpawnerCommands,
    assets: &AssetServer,
    fonts: &Fonts,
    data: &GameData,
    roster: &Roster,
    crew: &CrewData,
    power: &PowerInfo,
) {
    root.spawn(Node {
        position_type: PositionType::Absolute,
        top: px(112),
        left: px(10),
        right: px(10),
        flex_wrap: FlexWrap::Wrap,
        column_gap: px(10),
        row_gap: px(12),
        ..default()
    })
    .with_children(|grid| {
        // В сетке — только команда; кто придёт позже, сказано строкой ниже.
        for hero in data.heroes.iter().filter(|h| roster.owns(&h.id)) {
            hero_tile(grid, assets, fonts, hero, roster, power.hero(&hero.id));
        }
    });
    let mut coming: Vec<(u32, &str)> = data
        .heroes
        .iter()
        .filter(|h| !roster.owns(&h.id))
        .filter_map(|h| crew.arrivals.get(&h.id).map(|&c| (c, h.name.as_str())))
        .collect();
    coming.sort();
    if !coming.is_empty() {
        let line = coming
            .iter()
            .map(|(c, name)| format!("{name} — за главу {c}"))
            .collect::<Vec<_>>()
            .join(" · ");
        root.spawn((
            Node {
                position_type: PositionType::Absolute,
                top: px(714),
                left: px(10),
                right: px(10),
                padding: UiRect::axes(px(16), px(8)),
                justify_content: JustifyContent::Center,
                ..default()
            },
            Skin(Look::Tile),
        ))
        .with_child(text(
            fonts,
            format!("Скоро в команде: {line}"),
            false,
            13.0,
            PARCHMENT,
        ));
    }
    // Отряд в бой: четыре медальона.
    root.spawn((
        Node {
            position_type: PositionType::Absolute,
            top: px(614),
            left: px(10),
            right: px(10),
            height: px(92),
            padding: UiRect::horizontal(px(18)),
            column_gap: px(12),
            align_items: AlignItems::Center,
            ..default()
        },
        Skin(Look::Tile),
    ))
    .with_children(|row| {
        row.spawn(Node {
            flex_grow: 1.0,
            flex_direction: FlexDirection::Column,
            ..default()
        })
        .with_children(|c| {
            c.spawn(Node {
                column_gap: px(10),
                align_items: AlignItems::Center,
                ..default()
            })
            .with_children(|line| {
                line.spawn(style::heading(fonts, "Отряд", 19.0, BRIGHT));
                crate::power::badge(line, assets, fonts, power.team, 20.0, GOLD);
            });
            c.spawn(text(
                fonts,
                format!("с кораблём: залп +{}", crate::power::format(power.ship)),
                false,
                11.0,
                MUTED,
            ));
            let mut open = c.spawn((
                Node {
                    margin: UiRect::top(px(4)),
                    padding: UiRect::axes(px(12), px(6)),
                    align_self: AlignSelf::FlexStart,
                    ..default()
                },
                Skin(Look::Go),
            ));
            open.with_child(style::heading(fonts, "Расстановка", 15.0, Color::WHITE));
            pressable(&mut open);
            open.observe(
                |_: On<PointerClick>,
                 mut formation: ResMut<Formation>,
                 mut dirty: ResMut<CrewDirty>| {
                    *formation = Formation {
                        open: true,
                        slot: None,
                    };
                    dirty.0 = true;
                },
            );
        });
        for id in &roster.team {
            row.spawn(style::medallion(assets, id, 58.0, false));
        }
    });
}

/// Расстановка: четыре места отряда в ряд (порядок — где герой стоит на палубе), ниже — скамейка.
fn formation_view(
    root: &mut ChildSpawnerCommands,
    assets: &AssetServer,
    fonts: &Fonts,
    data: &GameData,
    roster: &Roster,
    picked: Option<usize>,
    away: &dyn Fn(&str) -> bool,
) {
    style::title(root, fonts, "Расстановка отряда", 56.0);
    crate::help::corner(root, fonts, "formation", 60.0, 12.0);
    root.spawn(Node {
        position_type: PositionType::Absolute,
        top: px(124),
        left: px(10),
        right: px(10),
        flex_direction: FlexDirection::Column,
        row_gap: px(10),
        ..default()
    })
    .with_children(|col| {
        col.spawn((
            Node {
                padding: UiRect::axes(px(8), px(12)),
                flex_direction: FlexDirection::Column,
                row_gap: px(8),
                ..default()
            },
            Skin(Look::Tile),
        ))
        .with_children(|panel| {
            panel.spawn(style::heading(fonts, "Отряд в бой", 22.0, BRIGHT));
            panel
                .spawn(Node {
                    justify_content: JustifyContent::SpaceEvenly,
                    ..default()
                })
                .with_children(|row| {
                    for slot in 0..4 {
                        formation_slot(
                            row,
                            assets,
                            fonts,
                            data,
                            roster,
                            slot,
                            picked == Some(slot),
                        );
                    }
                });
        });
        // Скамейка: все герои команды. Героя из отряда можно бросить сюда — он уйдёт из отряда.
        let mut bench_panel = col.spawn((
            Node {
                padding: UiRect::axes(px(12), px(10)),
                flex_wrap: FlexWrap::Wrap,
                justify_content: JustifyContent::SpaceEvenly,
                row_gap: px(8),
                ..default()
            },
            Skin(Look::Tile),
        ));
        bench_panel.observe(
            |_: On<PointerDragDrop>,
             mut dragging: ResMut<Dragging>,
             mut roster: ResMut<Roster>,
             mut toast: ResMut<Toast>,
             mut dirty: ResMut<CrewDirty>| {
                let Some((id, Some(_))) = dragging.0.take() else {
                    return;
                };
                if roster.team.len() <= 1 {
                    toast.show("В отряде нужен хотя бы один герой");
                    return;
                }
                roster.team.retain(|h| *h != id);
                dirty.0 = true;
            },
        );
        bench_panel.with_children(|bench| {
            for hero in data.heroes.iter().filter(|h| roster.owns(&h.id)) {
                let id = hero.id.clone();
                let busy = away(&id);
                let in_team = roster.team.contains(&id);
                let mut cell = bench.spawn(Node {
                    width: px(96),
                    flex_direction: FlexDirection::Column,
                    align_items: AlignItems::Center,
                    row_gap: px(2),
                    ..default()
                });
                cell.with_children(|c| {
                    c.spawn(style::medallion(assets, &id, 66.0, busy || in_team));
                    c.spawn(style::heading(fonts, hero.name.clone(), 15.0, BRIGHT));
                    c.spawn(text(
                        fonts,
                        if busy {
                            "в экспедиции"
                        } else if in_team {
                            "в отряде"
                        } else {
                            "свободен"
                        },
                        false,
                        11.0,
                        MUTED,
                    ));
                });
                pressable(&mut cell);
                if !busy {
                    draggable(&mut cell, assets, &id, None);
                }
                cell.observe(
                    move |_: On<PointerClick>,
                          exps: Res<crate::expeditions::Expeditions>,
                          crew: Res<CrewData>,
                          mut roster: ResMut<Roster>,
                          mut formation: ResMut<Formation>,
                          mut toast: ResMut<Toast>,
                          mut dirty: ResMut<CrewDirty>| {
                        if exps.is_away(&id) {
                            toast.show("Герой в экспедиции — вернётся, тогда поставим");
                            return;
                        }
                        let team = &mut roster.team;
                        let at = team.iter().position(|h| *h == id);
                        match (formation.slot, at) {
                            // Выбрано место: герой встаёт туда (из отряда — меняются местами).
                            (Some(slot), Some(j)) if slot < team.len() => team.swap(slot, j),
                            (Some(slot), None) if slot < team.len() => team[slot] = id.clone(),
                            (Some(_), Some(_)) => {}
                            (_, None) if team.len() < crew.team_size => team.push(id.clone()),
                            (None, Some(j)) => {
                                formation.slot = Some(j);
                                dirty.0 = true;
                                return;
                            }
                            _ => {
                                toast.show("Отряд полон — выберите место, куда поставить героя");
                                return;
                            }
                        }
                        formation.slot = None;
                        dirty.0 = true;
                    },
                );
            }
        });
        let mut done = col.spawn((
            Node {
                height: px(58),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            Skin(Look::Go),
        ));
        done.with_child(style::heading(fonts, "Готово", 22.0, Color::WHITE));
        pressable(&mut done);
        done.observe(
            |_: On<PointerClick>,
             mut formation: ResMut<Formation>,
             mut dirty: ResMut<CrewDirty>| {
                *formation = Formation::default();
                dirty.0 = true;
            },
        );
    });
}

/// Место в расстановке: медальон и имя героя или пустой круг.
#[allow(clippy::too_many_arguments)]
fn formation_slot(
    row: &mut ChildSpawnerCommands,
    assets: &AssetServer,
    fonts: &Fonts,
    data: &GameData,
    roster: &Roster,
    slot: usize,
    picked: bool,
) {
    let hero = roster.team.get(slot).and_then(|id| data.hero(id));
    let mut cell = row.spawn((
        Node {
            width: px(118),
            padding: UiRect::all(px(4)),
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::Center,
            row_gap: px(4),
            border_radius: BorderRadius::all(px(14)),
            ..default()
        },
        Outline::new(px(if picked { 3.0 } else { 0.0 }), px(2), GOLD),
        BackgroundColor(if picked {
            Color::srgba(1.0, 0.85, 0.4, 0.15)
        } else {
            Color::NONE
        }),
    ));
    cell.with_children(|c| match hero {
        Some(h) => {
            c.spawn(style::medallion(assets, &h.id, 88.0, false));
            c.spawn(style::heading(fonts, h.name.clone(), 17.0, BRIGHT));
            c.spawn(text(fonts, role_label(h.role), false, 12.0, MUTED));
        }
        None => {
            c.spawn((
                Node {
                    width: px(88),
                    height: px(88),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    border: UiRect::all(px(3)),
                    border_radius: BorderRadius::MAX,
                    ..default()
                },
                BorderColor::all(Color::srgba(1.0, 0.9, 0.7, 0.4)),
            ))
            .with_child(style::heading(fonts, "+", 40.0, MUTED));
            c.spawn(text(fonts, "пусто", false, 13.0, MUTED));
        }
    });
    pressable(&mut cell);
    if let Some(h) = hero {
        draggable(&mut cell, assets, &h.id, Some(slot));
    }
    cell.observe(
        move |_: On<PointerDragDrop>,
              mut dragging: ResMut<Dragging>,
              exps: Res<crate::expeditions::Expeditions>,
              crew: Res<CrewData>,
              mut roster: ResMut<Roster>,
              mut formation: ResMut<Formation>,
              mut toast: ResMut<Toast>,
              mut dirty: ResMut<CrewDirty>| {
            let Some((id, _)) = dragging.0.take() else {
                return;
            };
            if exps.is_away(&id) {
                toast.show("Герой в экспедиции — вернётся, тогда поставим");
                return;
            }
            put_in_slot(&mut roster.team, crew.team_size, &id, slot);
            formation.slot = None;
            dirty.0 = true;
        },
    );
    cell.observe(
        move |_: On<PointerClick>,
              mut roster: ResMut<Roster>,
              mut formation: ResMut<Formation>,
              mut toast: ResMut<Toast>,
              mut dirty: ResMut<CrewDirty>| {
            let len = roster.team.len();
            match formation.slot {
                // Повторный тап — убрать героя с места (один в отряде остаётся).
                Some(s) if s == slot => {
                    if slot < len && len > 1 {
                        roster.team.remove(slot);
                    } else if slot < len {
                        toast.show("В отряде нужен хотя бы один герой");
                    }
                    formation.slot = None;
                }
                Some(s) if s < len && slot < len => {
                    roster.team.swap(s, slot);
                    formation.slot = None;
                }
                _ => formation.slot = Some(slot),
            }
            dirty.0 = true;
        },
    );
}

/// Кого сейчас тащат: герой и место в отряде, откуда (`None` — со скамейки).
#[derive(Resource, Default)]
pub(crate) struct Dragging(Option<(String, Option<usize>)>);

/// Медальон, который едет за пальцем, пока героя тащат.
#[derive(Component)]
struct DragGhost;

const GHOST: f32 = 84.0;

/// Героя можно тащить: за пальцем едет его медальон, бросить — на место в отряде или на скамейку.
fn draggable(cell: &mut EntityCommands, assets: &AssetServer, id: &str, from: Option<usize>) {
    let image = assets.load(format!("ui/hero_{id}.png"));
    let id = id.to_string();
    cell.observe(
        move |ev: On<PointerDragStart>, mut commands: Commands, mut dragging: ResMut<Dragging>| {
            dragging.0 = Some((id.clone(), from));
            let at = ev.pointer.position - Vec2::splat(GHOST / 2.0);
            commands.spawn((
                DragGhost,
                DespawnOnExit(Screen::Crew),
                Node {
                    position_type: PositionType::Absolute,
                    left: px(at.x),
                    top: px(at.y),
                    width: px(GHOST),
                    height: px(GHOST),
                    ..default()
                },
                ImageNode::new(image.clone()),
                GlobalZIndex(50),
                Pickable::IGNORE,
            ));
        },
    );
    cell.observe(
        |ev: On<PointerDrag>, mut ghosts: Query<&mut Node, With<DragGhost>>| {
            let at = ev.pointer.position - Vec2::splat(GHOST / 2.0);
            for mut node in &mut ghosts {
                node.left = px(at.x);
                node.top = px(at.y);
            }
        },
    );
    // Бросок обрабатывает цель (место или скамейка) раньше; здесь только убираем медальон.
    cell.observe(
        |_: On<PointerDragEnd>,
         mut commands: Commands,
         ghosts: Query<Entity, With<DragGhost>>,
         mut dragging: ResMut<Dragging>| {
            for ghost in &ghosts {
                commands.entity(ghost).despawn();
            }
            dragging.0 = None;
        },
    );
}

/// Поставить героя на место `slot`: из отряда — поменять местами, со скамейки — заменить
/// стоящего там (или встать в конец, если место пустое).
fn put_in_slot(team: &mut Vec<String>, size: usize, id: &str, slot: usize) {
    match team.iter().position(|h| h == id) {
        Some(j) if slot < team.len() => team.swap(j, slot),
        Some(j) => {
            let h = team.remove(j);
            team.push(h);
        }
        None if slot < team.len() => team[slot] = id.to_string(),
        None if team.len() < size => team.push(id.to_string()),
        None => {}
    }
}

fn hero_tile(
    grid: &mut ChildSpawnerCommands,
    assets: &AssetServer,
    fonts: &Fonts,
    hero: &HeroDef,
    roster: &Roster,
    power: u32,
) {
    let owned = roster.owns(&hero.id);
    let in_team = roster.team.contains(&hero.id);
    let glow = crate::view::element_color(hero.element);
    let id = hero.id.clone();
    let mut tile = grid.spawn((
        Node {
            width: px(122),
            height: px(238),
            flex_direction: FlexDirection::Column,
            overflow: Overflow::clip(),
            ..default()
        },
        Skin(Look::Tile),
    ));
    tile.with_children(|t| {
        // Портрет: верх фигуры на фоне цвета стихии.
        t.spawn((
            Node {
                height: px(164),
                justify_content: JustifyContent::Center,
                overflow: Overflow::clip(),
                border_radius: BorderRadius::top(px(12)),
                ..default()
            },
            BackgroundGradient(vec![
                RadialGradient::new(
                    UiPosition::CENTER,
                    RadialGradientShape::FarthestCorner,
                    vec![
                        ColorStop::auto(glow.with_alpha(if owned { 0.75 } else { 0.25 })),
                        ColorStop::auto(Color::srgb(0.07, 0.05, 0.03)),
                    ],
                )
                .into(),
            ]),
            Pickable::IGNORE,
        ))
        .with_child((
            Node {
                height: px(300),
                margin: UiRect::top(px(6)),
                flex_shrink: 0.0,
                ..default()
            },
            ImageNode {
                image: assets.load(format!("ui/full_{}.png", hero.id)),
                color: if owned {
                    Color::WHITE
                } else {
                    Color::srgba(0.3, 0.3, 0.35, 0.9)
                },
                ..default()
            },
            Pickable::IGNORE,
        ));
        t.spawn(Node {
            flex_grow: 1.0,
            flex_direction: FlexDirection::Column,
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            row_gap: px(2),
            ..default()
        })
        .with_children(|info| {
            info.spawn(style::heading(fonts, hero.name.clone(), 17.0, BRIGHT));
            if owned {
                stars_row(info, assets, roster.stars(&hero.id), 11.0);
                info.spawn(Node {
                    column_gap: px(6),
                    align_items: AlignItems::Center,
                    ..default()
                })
                .with_children(|row| {
                    row.spawn(text(
                        fonts,
                        format!("ур. {}", roster.level(&hero.id)),
                        true,
                        12.0,
                        MUTED,
                    ));
                    crate::power::badge(row, assets, fonts, power, 13.0, GOLD);
                });
            } else {
                info.spawn(text(fonts, "придёт за главу", false, 12.0, MUTED));
            }
        });
        if in_team {
            t.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    top: px(6),
                    left: px(6),
                    padding: UiRect::axes(px(7), px(2)),
                    ..default()
                },
                Skin(Look::Go),
                Pickable::IGNORE,
            ))
            .with_child(style::heading(fonts, "в отряде", 11.0, Color::WHITE));
        }
    });
    pressable(&mut tile);
    tile.observe(
        move |_: On<PointerClick>, mut selected: ResMut<Selected>, mut dirty: ResMut<CrewDirty>| {
            selected.0 = Some(id.clone());
            dirty.0 = true;
        },
    );
}

/// «« Экипаж»: со страницы героя — обратно к сетке.
fn back_button(root: &mut ChildSpawnerCommands, fonts: &Fonts) {
    let mut back = root.spawn((
        Node {
            position_type: PositionType::Absolute,
            top: px(58),
            left: px(10),
            height: px(42),
            padding: UiRect::horizontal(px(14)),
            align_items: AlignItems::Center,
            ..default()
        },
        Skin(Look::Dark),
    ));
    back.with_child(style::heading(fonts, "« Экипаж", 16.0, BRIGHT));
    pressable(&mut back);
    back.observe(
        |_: On<PointerClick>, mut selected: ResMut<Selected>, mut dirty: ResMut<CrewDirty>| {
            selected.0 = None;
            dirty.0 = true;
        },
    );
}

fn roster_chip(
    rail: &mut ChildSpawnerCommands,
    assets: &AssetServer,
    hero: &HeroDef,
    roster: &Roster,
    selected: bool,
) {
    let in_team = roster.team.contains(&hero.id);
    let owned = roster.owns(&hero.id);
    let id = hero.id.clone();
    let mut chip = rail.spawn(Node {
        flex_direction: FlexDirection::Column,
        align_items: AlignItems::Center,
        row_gap: px(2),
        ..default()
    });
    chip.with_children(|c| {
        let mut medal = c.spawn((
            Node {
                width: px(CHIP),
                height: px(CHIP),
                border_radius: BorderRadius::MAX,
                ..default()
            },
            Pickable::IGNORE,
        ));
        if selected {
            medal.insert(Outline::new(px(3), px(1), GOLD));
        }
        medal.with_children(|m| {
            m.spawn(style::medallion(assets, &hero.id, CHIP, !owned));
            if in_team {
                // Метка «в отряде» — латунная бляшка в углу.
                m.spawn((
                    Node {
                        position_type: PositionType::Absolute,
                        top: px(-2),
                        right: px(-2),
                        width: px(16),
                        height: px(16),
                        border: UiRect::all(px(2)),
                        border_radius: BorderRadius::MAX,
                        ..default()
                    },
                    BackgroundColor(Color::srgb(0.95, 0.62, 0.20)),
                    BorderColor::all(Color::srgb(0.47, 0.32, 0.11)),
                    Pickable::IGNORE,
                ));
            }
        });
        if owned {
            stars_row(c, assets, roster.stars(&hero.id), 9.0);
        }
    });
    pressable(&mut chip);
    chip.observe(
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
    hero_power: u32,
) {
    let (merge_to, merge_info) = merge;
    let level = roster.level(&hero.id);
    let stars = roster.stars(&hero.id);
    let cap = crew.level_cap(stars);
    let at_cap = level >= cap;
    let now = hero.to_unit(level).stats;
    let next = hero.to_unit(level + 1).stats;
    let cost = crew.level_cost(level);
    let affordable = profile.journals >= cost.journals && profile.silver >= cost.silver;
    let in_team = roster.team.contains(&hero.id);
    let owned = roster.owns(&hero.id);

    // Вкладка «Снаряжение»: панель на всю ширину вместо героя.
    if owned && gear.tab == CrewTab::Gear {
        root.spawn((
            Node {
                position_type: PositionType::Absolute,
                top: px(HERO_TOP),
                left: px(10),
                right: px(10),
                padding: UiRect::axes(px(32), px(30)),
                flex_direction: FlexDirection::Column,
                row_gap: px(8),
                ..default()
            },
            Skin(Look::Tile),
        ))
        .with_children(|panel| {
            panel
                .spawn(Node {
                    column_gap: px(8),
                    align_items: AlignItems::Center,
                    ..default()
                })
                .with_children(|row| {
                    row.spawn(style::heading(fonts, hero.name.clone(), 24.0, BRIGHT));
                    row.spawn(text(fonts, hero.title.clone(), false, 13.0, MUTED));
                });
            tabs_row(panel, fonts, gear.tab);
            gear_tab(panel, fonts, gear);
            cabin_row(panel, fonts, cabin);
        });
        return;
    }

    // Герой во весь рост слева.
    root.spawn((
        Node {
            position_type: PositionType::Absolute,
            top: px(HERO_TOP + 6.0),
            left: px(6),
            height: px(474),
            ..default()
        },
        ImageNode {
            image: assets.load(format!("ui/full_{}.png", hero.id)),
            color: if owned {
                Color::WHITE
            } else {
                Color::srgba(0.35, 0.35, 0.4, 0.85)
            },
            ..default()
        },
        Pickable::IGNORE,
    ));

    // Справа — пергамент с характеристиками и вкладки под ним.
    root.spawn(Node {
        position_type: PositionType::Absolute,
        top: px(HERO_TOP + 6.0),
        right: px(12),
        width: px(254),
        flex_direction: FlexDirection::Column,
        row_gap: px(8),
        ..default()
    })
    .with_children(|col| {
        col.spawn((
            Node {
                padding: UiRect::new(px(18), px(18), px(16), px(16)),
                flex_direction: FlexDirection::Column,
                row_gap: px(4),
                ..default()
            },
            Skin(Look::Parchment),
        ))
        .with_children(|card| {
            card.spawn((
                Node {
                    margin: UiRect::right(px(40)),
                    ..default()
                },
                style::ink(fonts, hero.name.clone(), 30.0, INK),
            ));
            card.spawn((
                Node {
                    margin: UiRect::right(px(40)),
                    ..default()
                },
                style::ink(fonts, hero.title.clone(), 16.0, INK_SOFT),
            ));
            stars_row(card, assets, if owned { stars } else { 0 }, 16.0);
            if owned {
                crate::power::badge(card, assets, fonts, hero_power, 20.0, INK);
                card.spawn(style::ink(
                    fonts,
                    format!("Уровень {level} / {cap}"),
                    20.0,
                    INK,
                ));
                // Полоса уровня до потолка звёзд.
                card.spawn((
                    Node {
                        height: px(8),
                        margin: UiRect::bottom(px(4)),
                        border_radius: BorderRadius::all(px(4)),
                        ..default()
                    },
                    BackgroundColor(Color::srgba(0.27, 0.18, 0.11, 0.35)),
                ))
                .with_child((
                    Node {
                        width: percent(100.0 * level as f32 / cap.max(1) as f32),
                        height: percent(100),
                        border_radius: BorderRadius::all(px(4)),
                        ..default()
                    },
                    BackgroundColor(Color::srgb(0.85, 0.55, 0.18)),
                ));
            } else {
                card.spawn(style::ink(fonts, "Ещё не в команде", 17.0, INK_SOFT));
            }
            let growing = owned && !at_cap;
            for (icon, name, a, b) in [
                ("hp", "Здоровье", now.max_hp, next.max_hp),
                ("atk", "Атака", now.atk, next.atk),
                ("def", "Защита", now.def, next.def),
                ("spd", "Скорость", now.spd, next.spd),
            ] {
                card.spawn(Node {
                    column_gap: px(6),
                    align_items: AlignItems::Center,
                    ..default()
                })
                .with_children(|row| {
                    row.spawn((
                        Node {
                            width: px(24),
                            height: px(24),
                            ..default()
                        },
                        ImageNode::new(assets.load(format!("ui/stat_{icon}.png"))),
                    ));
                    row.spawn((
                        Node {
                            flex_grow: 1.0,
                            ..default()
                        },
                        style::ink(fonts, name, 19.0, INK_SOFT),
                    ));
                    row.spawn(style::ink(fonts, a.to_string(), 21.0, INK));
                    if growing && b > a {
                        row.spawn(style::ink(fonts, format!("+{}", b - a), 14.0, INK_GOOD));
                    }
                });
            }
            card.spawn((
                Node {
                    margin: UiRect::top(px(4)),
                    ..default()
                },
                text(
                    fonts,
                    format!(
                        "{} · {}",
                        role_label(hero.role),
                        element_label(hero.element)
                    ),
                    false,
                    13.0,
                    INK_SOFT,
                ),
            ));
        });
        if owned {
            tabs_row(col, fonts, gear.tab);
        }
    });

    // Умения в латунных кольцах.
    root.spawn(Node {
        position_type: PositionType::Absolute,
        top: px(590),
        left: px(12),
        column_gap: px(10),
        ..default()
    })
    .with_children(|row| skills_row(row, assets, fonts, crew, roster, profile, hero, owned));

    root.spawn(Node {
        position_type: PositionType::Absolute,
        top: px(700),
        left: px(12),
        right: px(12),
        flex_direction: FlexDirection::Column,
        row_gap: px(4),
        ..default()
    })
    .with_children(|col| {
        col.spawn(Node {
            column_gap: px(8),
            ..default()
        })
        .with_children(|buttons| {
            if owned {
                level_button(
                    buttons, assets, fonts, cost, profile, at_cap, affordable, cap,
                );
                merge_button(buttons, fonts, merge_to);
                team_button(buttons, fonts, &hero.id, in_team);
            } else {
                to_tavern_button(buttons, fonts);
            }
        });
        if owned {
            let (line, color) = match &merge_info {
                Ok(text) => (text.clone(), BETTER),
                Err(reason) => (reason.clone(), PARCHMENT),
            };
            col.spawn(text(fonts, line, false, 12.0, color));
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
                button.with_child(style::heading(
                    fonts,
                    label,
                    15.0,
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

#[allow(clippy::too_many_arguments)]
fn level_button(
    buttons: &mut ChildSpawnerCommands,
    assets: &AssetServer,
    fonts: &Fonts,
    cost: LevelCost,
    profile: &Profile,
    at_cap: bool,
    affordable: bool,
    cap: u32,
) {
    let ready = !at_cap && affordable;
    let mut button = buttons.spawn((
        Node {
            flex_grow: 1.0,
            height: px(68),
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
            b.spawn(style::heading(
                fonts,
                format!("Потолок: уровень {cap}"),
                18.0,
                Color::WHITE,
            ));
            b.spawn(text(fonts, "дальше — звёзды героя", false, 12.0, PARCHMENT));
        } else {
            b.spawn(style::heading(
                fonts,
                "Повысить уровень",
                20.0,
                Color::WHITE,
            ));
            // Цена крупно, под ней — сколько есть; чего не хватает — красным.
            b.spawn(Node {
                column_gap: px(14),
                ..default()
            })
            .with_children(|row| {
                for (kind, need, have) in [
                    ("journals", cost.journals, profile.journals),
                    ("silver", cost.silver, profile.silver),
                ] {
                    let short = have < need;
                    row.spawn(Node {
                        align_items: AlignItems::Center,
                        column_gap: px(4),
                        ..default()
                    })
                    .with_children(|item| {
                        item.spawn(style::icon(assets, kind, 24.0));
                        item.spawn(style::heading(
                            fonts,
                            if short {
                                format!("{need} (есть {have})")
                            } else {
                                need.to_string()
                            },
                            17.0,
                            if short {
                                Color::srgb(1.0, 0.62, 0.55)
                            } else {
                                Color::WHITE
                            },
                        ));
                    });
                }
            });
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
    row: &mut ChildSpawnerCommands,
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
    for (i, &level) in levels.iter().enumerate() {
        let (charts, silver) = crew.skill_cost(level);
        let can = owned && level < cap && profile.charts >= charts && profile.silver >= silver;
        let mut slot = row.spawn(Node {
            width: px(84),
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::Center,
            row_gap: px(2),
            ..default()
        });
        slot.with_children(|c| {
            let mut ring = c.spawn((
                Node {
                    width: px(SKILL_RING),
                    height: px(SKILL_RING),
                    border_radius: BorderRadius::MAX,
                    ..default()
                },
                ImageNode::new(assets.load(format!("ui/skill_{}_{i}.png", hero.id))),
                Pickable::IGNORE,
            ));
            if can {
                ring.insert(Outline::new(px(2), px(1), GOLD));
            }
            // Табличка уровня под кольцом.
            c.spawn((
                Node {
                    margin: UiRect::top(px(-10)),
                    padding: UiRect::axes(px(10), px(1)),
                    border: UiRect::all(px(2)),
                    border_radius: BorderRadius::all(px(10)),
                    ..default()
                },
                BackgroundColor(Color::srgb(0.16, 0.10, 0.06)),
                BorderColor::all(Color::srgb(0.72, 0.55, 0.26)),
                Pickable::IGNORE,
            ))
            .with_child(style::heading(
                fonts,
                if level >= cap && owned {
                    format!("ур. {level} · макс.")
                } else {
                    format!("ур. {level}")
                },
                13.0,
                BRIGHT,
            ));
            if owned && level < cap {
                style::costs(
                    c,
                    assets,
                    fonts,
                    &[("charts", charts), ("silver", silver)],
                    10.0,
                    if can { BETTER } else { BRIGHT },
                );
            }
        });
        if owned {
            pressable(&mut slot);
            slot.observe(move |_: On<PointerClick>, mut commands: Commands| {
                commands.run_system_cached_with(upgrade_skill, i);
            });
        }
    }
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
            width: px(108),
            height: px(68),
            flex_direction: FlexDirection::Column,
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            ..default()
        },
        Skin(if ready { Look::Go } else { Look::TabOff }),
    ));
    button.with_children(|b| {
        b.spawn(style::heading(fonts, "Слияние", 18.0, Color::WHITE));
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
            height: px(56),
            flex_direction: FlexDirection::Column,
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            ..default()
        },
        Skin(Look::Go),
    ));
    button.with_children(|b| {
        b.spawn(style::heading(
            fonts,
            "Нанять в Таверне",
            20.0,
            Color::WHITE,
        ));
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
            width: px(108),
            height: px(68),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            ..default()
        },
        Skin(Look::Dark),
    ));
    button.with_child(style::heading(
        fonts,
        if in_team {
            "Из отряда"
        } else {
            "В отряд"
        },
        18.0,
        BRIGHT,
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

#[cfg(test)]
mod drag_tests {
    use super::put_in_slot;

    fn team(ids: &[&str]) -> Vec<String> {
        ids.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn drop_swaps_or_replaces() {
        let mut t = team(&["a", "b", "c", "d"]);
        put_in_slot(&mut t, 4, "d", 0);
        assert_eq!(t, team(&["d", "b", "c", "a"]));
        put_in_slot(&mut t, 4, "e", 1);
        assert_eq!(t, team(&["d", "e", "c", "a"]));
        let mut t = team(&["a", "b"]);
        put_in_slot(&mut t, 4, "c", 3);
        assert_eq!(t, team(&["a", "b", "c"]));
    }
}
