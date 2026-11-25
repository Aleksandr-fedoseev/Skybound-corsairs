//! Состояние экрана боя: бой из battle-core, очередь событий и их проигрывание с паузами.
//!
//! Модель (`Battle`) считает ход сразу целиком, а экран показывает его события по одному.
//! Поэтому здоровье на экране — своя копия, которая меняется только по событиям.

use std::collections::VecDeque;
use std::time::{SystemTime, UNIX_EPOCH};

use battle_core::{AbilityKind, Battle, Event, Outcome, Ship, UnitId};
use bevy::prelude::*;
use game_data::GameData;

use crate::battlefield::FieldConfig;
use serde::Deserialize;

/// Бой не из главы: свои враги, своя добыча.
#[derive(Clone, Debug)]
pub struct CustomFight {
    pub title: String,
    pub kind: FightKind,
    /// Враги и их уровни.
    pub enemies: Vec<(String, u32)>,
    pub silver: u32,
    pub planks: u32,
    pub journals: u32,
    /// Особое правило: ульты копятся быстрее (1 — как обычно).
    pub energy_rate: u32,
    /// Здоровье героев в начале боя (доля от максимума); пусто — все целы.
    pub ally_hp: Vec<(String, f32)>,
    /// Залп заряжен с первого хода.
    pub charged: bool,
    /// Усиления героев на этот бой (Блокада).
    pub boost: Boost,
    /// Ветер боя.
    pub wind: Option<battle_core::Element>,
}

/// Усиления героев: доли прибавки к здоровью, атаке, скорости, лечению, урону умений и залпу.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Boost {
    pub hp: f32,
    pub atk: f32,
    pub spd: f32,
    pub heal: f32,
    pub skill: f32,
    pub broadside: f32,
}

impl Boost {
    /// Герой с усилениями.
    pub fn apply(&self, mut unit: battle_core::UnitDef) -> battle_core::UnitDef {
        let up = |v: u32, k: f32| (v as f32 * (1.0 + k)).round() as u32;
        unit.stats.max_hp = up(unit.stats.max_hp, self.hp);
        unit.stats.atk = up(unit.stats.atk, self.atk);
        unit.stats.spd = up(unit.stats.spd, self.spd);
        let heal = 1.0 + self.heal;
        let abilities = [
            Some(&mut unit.basic),
            unit.skill.as_mut(),
            unit.ultimate.as_mut(),
        ];
        for ability in abilities.into_iter().flatten() {
            for e in &mut ability.effects {
                if let battle_core::Effect::Heal { percent } = &mut e.effect {
                    *percent *= heal;
                }
            }
        }
        if let Some(skill) = unit.skill.as_mut() {
            for e in &mut skill.effects {
                if let battle_core::Effect::Damage { multiplier } = &mut e.effect {
                    *multiplier *= 1.0 + self.skill;
                }
            }
        }
        unit
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FightKind {
    Convoy,
    /// Этаж Бездны облаков.
    Abyss(u32),
    /// Буревая тварь ивента «Буря»: счёт — урон.
    Storm,
    /// Этап трассы «Небесной регаты».
    Regatta(usize),
    /// Волна «Имперской блокады».
    Blockade(usize),
    /// Руины: этаж и выбранный зал.
    Ruins(u32, usize),
    /// Арена: ступень силы соперника.
    Arena(usize),
}

/// Залп до первых данных о корабле (Пушечная палуба ур. 1).
const BASE_BROADSIDE: u32 = 120;
/// Не больше стольких событий за кадр, чтобы на x3 бой не «проскакивал».
const MAX_EVENTS_PER_FRAME: usize = 8;

/// Темп и размах анимаций боя. Лежит в `data/presentation.json`, время — в секундах на x1.
#[derive(Clone, Debug, Deserialize)]
pub struct Presentation {
    /// Пауза после каждого хода.
    pub step_pause: f32,
    /// Замах перед ударом: обычная атака короткая, ульта и залп — событие.
    pub basic_windup: f32,
    pub skill_windup: f32,
    pub ult_windup: f32,
    pub broadside_windup: f32,
    /// Пауза между попаданиями по нескольким целям.
    pub hit_gap: f32,
    /// Стоп-кадр после крита или удара по слабости.
    pub crit_hitstop: f32,
    pub defeat_pause: f32,
    /// Сколько висит название умения над головой и полоса ульты.
    pub banner_skill: f32,
    pub banner_ult: f32,
    /// Атака: рывок к цели на долю расстояния и назад.
    pub attack_time: f32,
    pub attack_reach: f32,
    /// Ульта: присед, затем рывок дальше обычного.
    pub ult_time: f32,
    pub ult_reach: f32,
    /// Попадание: отброс назад и вспышка.
    pub hit_time: f32,
    pub hit_knockback: f32,
    /// Смерть: падение и угасание.
    pub death_time: f32,
    /// Дыхание в ожидании: период (с) и размах (доля роста).
    pub idle_period: f32,
    pub idle_amp: f32,
    pub flash_time: f32,
    /// Тряска всего экрана на ульте и залпе.
    pub screen_shake_px: f32,
    pub screen_shake_time: f32,
    /// Затемнение неба на ульте.
    pub dim_alpha: f32,
    pub dim_time: f32,
    pub float_px: f32,
    pub float_time: f32,
    /// Во сколько раз цифра урона больше в момент появления.
    pub pop_scale: f32,
    pub crit_scale: f32,
}

impl Presentation {
    pub fn bundled() -> Self {
        serde_json::from_str(include_str!("../../../data/presentation.json"))
            .expect("data/presentation.json корректен")
    }
}

/// Что показать на экране: событие боя, которое уже пора проиграть.
#[derive(Message, Clone, Debug)]
pub struct Shown {
    pub event: Event,
    /// Для действия — первая цель, к которой боец делает рывок.
    pub focus: Option<UnitId>,
}

#[derive(Resource)]
pub struct BattleScreen {
    pub data: GameData,
    pub look: Presentation,
    /// Раскладки поля боя и текущая (в тестовой сцене переключается клавишей V).
    pub field: FieldConfig,
    pub layout: String,
    pub stage: usize,
    /// Свой бой вместо этапа главы (конвой, этаж Бездны).
    pub custom: Option<CustomFight>,
    /// Отряд: герои, уровни и звёзды (собирается на экране «Экипаж»).
    pub team: Vec<crate::crew::Member>,
    /// Правила звёзд — чтобы собрать бойцов с бонусами звёзд.
    pub crew: crate::crew::CrewData,
    /// Прибавка к уровню всех героев в тестовой сцене (клавиши [ и ]).
    pub gym_bonus: u32,
    /// Сила залпа: от уровня Пушечной палубы.
    pub broadside_power: u32,
    pub seed: u64,
    pub battle: Battle,
    /// Здоровье, которое сейчас видит игрок.
    pub shown_hp: Vec<u32>,
    pub shown_current: Option<UnitId>,
    pub shown_outcome: Option<Outcome>,
    /// Ветер боя, который видит игрок (меняется событием `WindChanged`).
    pub shown_wind: Option<battle_core::Element>,
    /// Кто сейчас замахивается (красное кольцо под ним).
    pub shown_charging: Option<UnitId>,
    pub queue: VecDeque<Event>,
    pub wait: f32,
    pub speed: u32,
    pub paused: bool,
    /// АВТО: ульты нажимаются сами. Без него ульты — только по тапу игрока.
    pub auto: bool,
    /// Экран надо собрать заново (новый бой с другим составом).
    pub rebuild: bool,
    /// Темп умения, которое сейчас показывается (свой эффект — свои паузы между попаданиями).
    pub spell: Option<crate::spells::Spell>,
    /// Сообщение для строки тестовой сцены (например, ошибка в data/).
    #[cfg_attr(not(feature = "dev"), allow(dead_code))]
    pub notice: Option<String>,
}

impl BattleScreen {
    pub fn new(
        data: GameData,
        look: Presentation,
        field: FieldConfig,
        team: Vec<crate::crew::Member>,
        crew: crate::crew::CrewData,
    ) -> Self {
        let seed = random_seed();
        let battle = new_battle(&data, &crew, 0, None, &team, 0, BASE_BROADSIDE, seed);
        let mut screen = Self {
            data,
            look,
            layout: field.default.clone(),
            field,
            stage: 0,
            custom: None,
            team,
            crew,
            gym_bonus: 0,
            broadside_power: BASE_BROADSIDE,
            seed,
            battle,
            shown_hp: Vec::new(),
            shown_current: None,
            shown_outcome: None,
            shown_wind: None,
            shown_charging: None,
            queue: VecDeque::new(),
            wait: 0.0,
            speed: 1,
            paused: false,
            auto: true,
            rebuild: true,
            spell: None,
            notice: None,
        };
        screen.reset_view();
        screen
    }

    pub fn stage_id(&self) -> &str {
        &self.data.stages[self.stage].id
    }

    pub fn stage_name(&self) -> &str {
        &self.data.stages[self.stage].name
    }

    /// Заголовок боя над полем.
    pub fn title(&self) -> String {
        match &self.custom {
            Some(fight) => fight.title.clone(),
            None => match self.shown_wind {
                Some(wind) => format!(
                    "{} · {} · ветер боя: {}",
                    self.stage_id(),
                    self.stage_name(),
                    crate::crew::element_label(wind)
                ),
                None => format!("{} · {}", self.stage_id(), self.stage_name()),
            },
        }
    }

    /// Начать бой заново с текущими этапом, уровнем и seed.
    pub fn restart(&mut self) {
        self.battle = new_battle(
            &self.data,
            &self.crew,
            self.stage,
            self.custom.as_ref(),
            &self.team,
            self.gym_bonus,
            self.broadside_power,
            self.seed,
        );
        self.reset_view();
    }

    pub fn restart_with_new_seed(&mut self) {
        self.seed = random_seed();
        self.restart();
    }

    fn reset_view(&mut self) {
        self.shown_hp = self.battle.units().iter().map(|u| u.hp).collect();
        self.shown_current = self.battle.current();
        self.shown_outcome = None;
        self.shown_wind = self.battle.wind();
        self.shown_charging = None;
        self.queue.clear();
        self.spell = None;
        self.wait = self.look.step_pause;
        self.rebuild = true;
    }

    /// Ульта по тапу. Работает в любой момент, даже вне хода героя.
    pub fn try_ultimate(&mut self, id: UnitId) {
        if self.shown_outcome.is_some() || !self.battle.unit(id).ultimate_ready() {
            return;
        }
        if let Ok(events) = self.battle.use_ultimate(id, None) {
            self.queue.extend(events);
        }
    }

    /// Сколько ждать после показа события.
    /// Замах умения до первого попадания: свой у умения с эффектом, иначе общий для вида.
    pub fn windup(&self, unit: UnitId, kind: AbilityKind) -> f32 {
        let id = &self.battle.unit(unit).def.id;
        if let Some(w) = self.field.spells.get(id, kind).and_then(|s| s.windup) {
            return w;
        }
        let look = &self.look;
        match kind {
            AbilityKind::Basic => look.basic_windup,
            AbilityKind::Skill => look.skill_windup,
            AbilityKind::Ultimate => look.ult_windup,
        }
    }

    fn delay_after(&self, event: &Event) -> f32 {
        let look = &self.look;
        match event {
            Event::AbilityUsed { unit, kind, .. } => self.windup(*unit, *kind),
            Event::BroadsideFired => look.broadside_windup,
            Event::Damage { weakness, crit, .. } => {
                self.spell
                    .as_ref()
                    .and_then(|s| s.hit_gap)
                    .unwrap_or(look.hit_gap)
                    + if *weakness || *crit {
                        look.crit_hitstop
                    } else {
                        0.0
                    }
            }
            Event::Healed { .. } | Event::Advanced { .. } | Event::Delayed { .. } => look.hit_gap,
            Event::Defeated { .. } => look.defeat_pause,
            Event::WindChanged { .. } => look.broadside_windup,
            // Замах — пауза подольше: игрок успевает понять, что пора жать ульты.
            Event::Charging { .. } => look.broadside_windup * 1.5,
            Event::ChargeBroken { .. } => look.defeat_pause,
            Event::TurnStarted { .. } | Event::BattleEnded { .. } => 0.0,
        }
    }

    /// Обновить то, что видит игрок, по событию.
    fn apply_shown(&mut self, event: &Event) {
        match *event {
            Event::Damage { target, amount, .. } => {
                let hp = &mut self.shown_hp[target.0];
                *hp = hp.saturating_sub(amount);
            }
            Event::Healed { target, amount, .. } => {
                let max = self.battle.unit(target).def.stats.max_hp;
                let hp = &mut self.shown_hp[target.0];
                *hp = (*hp + amount).min(max);
            }
            Event::Defeated { unit } => self.shown_hp[unit.0] = 0,
            Event::WindChanged { wind, .. } => self.shown_wind = Some(wind),
            Event::Charging { unit, .. } => self.shown_charging = Some(unit),
            Event::ChargeBroken { unit } if self.shown_charging == Some(unit) => {
                self.shown_charging = None;
            }
            Event::AbilityUsed { unit, .. } if self.shown_charging == Some(unit) => {
                self.shown_charging = None;
            }
            Event::TurnStarted { unit } => self.shown_current = Some(unit),
            Event::BattleEnded { outcome } => {
                self.shown_outcome = Some(outcome);
                self.shown_current = None;
            }
            _ => {}
        }
    }
}

pub fn random_seed() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0)
}

/// Герой отряда таким, каким он выходит в бой: уровень, звёзды, умения, снаряжение, Каюты.
pub fn member_unit(
    data: &GameData,
    crew: &crate::crew::CrewData,
    m: &crate::crew::Member,
    bonus: u32,
) -> battle_core::UnitDef {
    let hero = data.hero(&m.id).expect("герой есть в data/heroes.json");
    let mut unit = crew.hero_unit(hero, m.level + bonus, m.stars, m.skills);
    unit.stats.max_hp += m.gear.max_hp;
    unit.stats.atk += m.gear.atk;
    unit.stats.def += m.gear.def;
    unit.stats.spd += m.gear.spd;
    unit.stats.atk = (unit.stats.atk as f32 * (1.0 + m.atk_bonus)).round() as u32;
    unit
}

#[allow(clippy::too_many_arguments)]
fn new_battle(
    data: &GameData,
    crew: &crate::crew::CrewData,
    stage: usize,
    custom: Option<&CustomFight>,
    team: &[crate::crew::Member],
    bonus: u32,
    broadside_power: u32,
    seed: u64,
) -> Battle {
    let team_ids: Vec<String> = team.iter().map(|m| m.id.clone()).collect();
    let boost = custom.map(|f| f.boost.clone()).unwrap_or_default();
    let team = team
        .iter()
        .map(|m| boost.apply(member_unit(data, crew, m, bonus)))
        .collect();
    let broadside_power = (broadside_power as f32 * (1.0 + boost.broadside)).round() as u32;
    let enemies = match custom {
        Some(fight) => fight
            .enemies
            .iter()
            .map(|(id, level)| match data.enemy(id) {
                Some(enemy) => enemy.to_unit(*level),
                // Соперник на Арене выставляет таких же героев, как у игрока.
                None => data
                    .hero(id)
                    .expect("враг есть в data/enemies.json или data/heroes.json")
                    .to_unit(*level),
            })
            .collect(),
        None => data
            .stage_enemies(&data.stages[stage].id)
            .expect("этап есть в data/stages.json"),
    };
    let mut battle = Battle::new(team, enemies, Ship { broadside_power }, seed);
    match custom {
        None => battle.set_wind(data.stages[stage].wind),
        Some(fight) => battle.set_wind(fight.wind),
    }
    if let Some(fight) = custom {
        battle.set_energy_rate(fight.energy_rate);
        for (i, member) in team_ids.iter().enumerate() {
            if let Some((_, frac)) = fight.ally_hp.iter().find(|(id, _)| id == member) {
                let max = battle.unit(battle_core::UnitId(i)).def.stats.max_hp;
                battle.set_hp(battle_core::UnitId(i), (max as f32 * frac).round() as u32);
            }
        }
        if fight.charged {
            battle.charge_broadside();
        }
    }
    battle
}

/// Выход в поход: этап с карты или следующий непройденный.
#[allow(clippy::too_many_arguments)]
pub fn enter(
    mut screen: ResMut<BattleScreen>,
    progress: Res<crate::menu::Progress>,
    voyage: Res<crate::menu::Voyage>,
    roster: Res<crate::crew::Roster>,
    ship: Res<crate::ship::Ship>,
    ship_data: Res<crate::ship::ShipData>,
    cabins: (Res<crate::cabins::Cabins>, Res<crate::cabins::CabinData>),
    gear: (Res<crate::gear::Gear>, Res<crate::gear::GearData>),
) {
    screen.custom = voyage.custom.clone();
    // Блокада и Руины: идёт тот отряд, с которым начали, — только выжившие, даже если в Экипаже
    // с тех пор поменяли состав.
    let ids: Vec<String> = match &screen.custom {
        Some(fight) if !fight.ally_hp.is_empty() => fight
            .ally_hp
            .iter()
            .filter(|(id, f)| *f > 0.0 && roster.owns(id))
            .map(|(id, _)| id.clone())
            .collect(),
        _ => roster.team.clone(),
    };
    screen.team = roster.members(&ids);
    for m in &mut screen.team {
        m.atk_bonus = cabins.0.atk_bonus(&m.id, &ids, &cabins.1);
        m.gear = gear.0.bonus(&m.id, &gear.1);
    }
    screen.broadside_power = ship.broadside(&ship_data);
    let next = progress.next_stage(screen.data.stages.len());
    screen.stage = voyage.stage.unwrap_or(next);
    screen.paused = false;
    screen.restart_with_new_seed();
}

/// Возврат на корабль: убрать поле, интерфейс боя и эффекты, вернуть камеру на место.
#[allow(clippy::type_complexity)]
pub fn leave(
    mut commands: Commands,
    roots: Query<
        Entity,
        Or<(
            With<crate::view::ScreenRoot>,
            With<crate::battlefield::FieldRoot>,
            With<crate::fx::Floating>,
            With<crate::fx::Projectile>,
            With<crate::fx::Flash>,
            With<crate::spells::SpellRoot>,
        )>,
    >,
    mut camera: Query<&mut Transform, With<Camera2d>>,
) {
    for root in &roots {
        commands.entity(root).despawn();
    }
    for mut transform in &mut camera {
        transform.translation = Vec3::ZERO;
    }
    commands.insert_resource(crate::fx::BandFx::default());
    commands.insert_resource(crate::fx::ScreenFx::default());
}

/// Проигрывает события по одному; когда очередь пуста — просит у боя следующий ход.
pub fn play_events(
    time: Res<Time>,
    mut screen: ResMut<BattleScreen>,
    mut shown: MessageWriter<Shown>,
) {
    if screen.paused || screen.rebuild {
        return;
    }
    screen.wait -= time.delta_secs() * screen.speed as f32;
    for _ in 0..MAX_EVENTS_PER_FRAME {
        if screen.wait > 0.0 {
            break;
        }
        if let Some(event) = screen.queue.pop_front() {
            screen.apply_shown(&event);
            match &event {
                Event::AbilityUsed { unit, kind, .. } => {
                    let id = screen.battle.unit(*unit).def.id.clone();
                    screen.spell = screen.field.spells.get(&id, *kind).cloned();
                }
                Event::TurnStarted { .. } => screen.spell = None,
                _ => {}
            }
            screen.wait += screen.delay_after(&event);
            let focus = match event {
                Event::AbilityUsed { unit, .. } => first_target(&screen.queue, unit),
                _ => None,
            };
            shown.write(Shown { event, focus });
        } else if screen.battle.outcome().is_none() {
            let events = if screen.auto {
                screen.battle.auto_step()
            } else {
                screen.battle.auto_turn()
            };
            screen.queue.extend(events);
            screen.wait += screen.look.step_pause;
        } else {
            screen.wait = 0.0;
            break;
        }
    }
}

/// Клавиши для игры на ПК: те же действия, что у кнопок внизу экрана.
pub fn keyboard(keys: Res<ButtonInput<KeyCode>>, mut screen: ResMut<BattleScreen>) {
    use crate::view::{ButtonKind, press};
    for (key, kind) in [
        (KeyCode::KeyA, ButtonKind::Auto),
        (KeyCode::KeyX, ButtonKind::Speed),
        (KeyCode::Space, ButtonKind::Pause),
    ] {
        if keys.just_pressed(key) {
            press(&mut screen, kind);
        }
    }
    // N — новый бой, R — переиграть тот же (тот же seed).
    if keys.just_pressed(KeyCode::KeyN) {
        screen.restart_with_new_seed();
    }
    if keys.just_pressed(KeyCode::KeyR) {
        screen.restart();
    }
}

/// Первая цель действия: смотрим вперёд по очереди до следующего действия.
fn first_target(queue: &VecDeque<Event>, actor: UnitId) -> Option<UnitId> {
    for event in queue {
        match *event {
            Event::Damage {
                source: Some(source),
                target,
                ..
            } if source == actor => return Some(target),
            Event::Healed { source, target, .. } if source == actor => return Some(target),
            Event::AbilityUsed { .. } | Event::TurnStarted { .. } | Event::BattleEnded { .. } => {
                return None;
            }
            _ => {}
        }
    }
    None
}
