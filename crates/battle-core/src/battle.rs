//! Состояние боя и правила ходов.
//!
//! Очерёдность по скорости: у каждого бойца есть `action_value` — «расстояние» до хода.
//! Ходит боец с наименьшим значением, после хода его значение сбрасывается до
//! `ACTION_GAUGE / скорость`. Быстрые бойцы ходят чаще.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::ai;
use crate::rng::Rng;
use crate::unit::{Ability, Effect, Element, Side, Target, Unit, UnitDef, UnitId};
use crate::{
    BROADSIDE_MAX, BROADSIDE_PER_ACTION, CRIT_CHANCE, CRIT_MULTIPLIER, ENERGY_BASIC, ENERGY_MAX,
    ENERGY_ON_HIT, ENERGY_SKILL, WEAKNESS_MULTIPLIER, WIND_BONUS,
};

/// Корабль игрока в бою. Сила залпа зависит от уровня пушечной палубы.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Ship {
    pub broadside_power: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Outcome {
    Victory,
    Defeat,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ActionKind {
    /// Обычная атака: даёт очко умений.
    Basic,
    /// Умение: тратит очко умений (только у союзников).
    Skill,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Action {
    pub kind: ActionKind,
    /// Выбранная цель. `None` — цель выберет автобой.
    pub target: Option<UnitId>,
}

impl Action {
    pub fn basic(target: Option<UnitId>) -> Self {
        Self {
            kind: ActionKind::Basic,
            target,
        }
    }

    pub fn skill(target: Option<UnitId>) -> Self {
        Self {
            kind: ActionKind::Skill,
            target,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum AbilityKind {
    Basic,
    Skill,
    Ultimate,
}

/// Что произошло в бою. Движок проигрывает эти события как анимации.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Event {
    TurnStarted {
        unit: UnitId,
    },
    AbilityUsed {
        unit: UnitId,
        kind: AbilityKind,
        name: String,
    },
    /// `source == None` — урон от залпа корабля.
    Damage {
        source: Option<UnitId>,
        target: UnitId,
        amount: u32,
        weakness: bool,
        crit: bool,
    },
    Healed {
        source: UnitId,
        target: UnitId,
        amount: u32,
    },
    Advanced {
        unit: UnitId,
        fraction: f32,
    },
    Delayed {
        unit: UnitId,
        fraction: f32,
    },
    Defeated {
        unit: UnitId,
    },
    BroadsideFired,
    BattleEnded {
        outcome: Outcome,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BattleError {
    BattleOver,
    SkillOnCooldown,
    NoSkill,
    NoUltimate,
    UltimateNotReady,
    NotAnAlly,
    UnitDefeated,
}

impl fmt::Display for BattleError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let text = match self {
            BattleError::BattleOver => "бой уже закончен",
            BattleError::SkillOnCooldown => "умение ещё перезаряжается",
            BattleError::NoSkill => "у бойца нет умения",
            BattleError::NoUltimate => "у бойца нет ульты",
            BattleError::UltimateNotReady => "ульта ещё не заряжена",
            BattleError::NotAnAlly => "это не союзник",
            BattleError::UnitDefeated => "боец повержен",
        };
        f.write_str(text)
    }
}

impl std::error::Error for BattleError {}

#[derive(Clone, Debug)]
pub struct Battle {
    units: Vec<Unit>,
    ship: Ship,
    broadside: u32,
    rng: Rng,
    current: Option<UnitId>,
    outcome: Option<Outcome>,
    turn: u32,
    /// Во сколько раз быстрее копится энергия ульт (особые правила ивентов).
    energy_rate: u32,
    /// Ветер боя: стихия, чьи бойцы бьют сильнее.
    wind: Option<Element>,
}

impl Battle {
    /// Особое правило боя: ульты копятся в `rate` раз быстрее.
    pub fn set_energy_rate(&mut self, rate: u32) {
        self.energy_rate = rate.max(1);
    }

    /// Ветер боя: бойцы этой стихии (с обеих сторон) наносят на `WIND_BONUS` больше урона.
    pub fn set_wind(&mut self, wind: Option<Element>) {
        self.wind = wind;
    }

    pub fn wind(&self) -> Option<Element> {
        self.wind
    }

    /// Особое правило боя: бойец начинает с части здоровья (оборона в несколько волн).
    pub fn set_hp(&mut self, id: UnitId, hp: u32) {
        let unit = &mut self.units[id.0];
        unit.hp = hp.clamp(1, unit.def.stats.max_hp);
    }

    /// Особое правило боя: залп корабля заряжен с первого хода.
    pub fn charge_broadside(&mut self) {
        self.broadside = BROADSIDE_MAX;
    }

    /// Сколько накоплено на шкале залпа.
    pub fn broadside_charge(&self) -> u32 {
        self.broadside
    }

    /// Создаёт бой. Первый ход уже определён: см. [`Battle::current`].
    pub fn new(allies: Vec<UnitDef>, enemies: Vec<UnitDef>, ship: Ship, seed: u64) -> Self {
        let mut units = Vec::with_capacity(allies.len() + enemies.len());
        for (side, defs) in [(Side::Ally, allies), (Side::Enemy, enemies)] {
            for def in defs {
                let mut unit = Unit {
                    id: UnitId(units.len()),
                    side,
                    hp: def.stats.max_hp,
                    energy: 0,
                    action_value: 0.0,
                    turns_taken: 0,
                    skill_cooldown: 0,
                    def,
                };
                unit.action_value = unit.base_action_value();
                unit.skill_cooldown = unit.skill_cooldown_start();
                units.push(unit);
            }
        }

        let mut battle = Battle {
            units,
            ship,
            broadside: 0,
            rng: Rng::new(seed),
            current: None,
            outcome: None,
            turn: 0,
            energy_rate: 1,
            wind: None,
        };
        let mut events = Vec::new();
        battle.check_outcome(&mut events);
        if battle.outcome.is_none() {
            battle.advance_to_next_turn(&mut events);
        }
        battle
    }

    // ---- Чтение состояния ----

    pub fn units(&self) -> &[Unit] {
        &self.units
    }

    pub fn unit(&self, id: UnitId) -> &Unit {
        &self.units[id.0]
    }

    pub fn alive(&self, side: Side) -> impl Iterator<Item = &Unit> {
        self.units
            .iter()
            .filter(move |u| u.side == side && u.is_alive())
    }

    /// Чей сейчас ход. `None`, если бой закончен.
    pub fn current(&self) -> Option<UnitId> {
        self.current
    }

    pub fn outcome(&self) -> Option<Outcome> {
        self.outcome
    }

    /// Заряд залпа корабля, от 0 до [`BROADSIDE_MAX`].
    pub fn broadside(&self) -> u32 {
        self.broadside
    }

    pub fn ship(&self) -> Ship {
        self.ship
    }

    /// Номер текущего хода, начиная с 1.
    pub fn turn(&self) -> u32 {
        self.turn
    }

    /// Порядок следующих `n` ходов для шкалы сверху экрана (без учёта будущих эффектов).
    pub fn turn_order_preview(&self, n: usize) -> Vec<UnitId> {
        let mut gauges: Vec<(UnitId, f32, f32)> = self
            .units
            .iter()
            .filter(|u| u.is_alive())
            .map(|u| (u.id, u.action_value, u.base_action_value()))
            .collect();
        let mut order = Vec::with_capacity(n);
        if gauges.is_empty() || self.outcome.is_some() {
            return order;
        }
        if let Some(current) = self.current {
            order.push(current);
            if let Some(g) = gauges.iter_mut().find(|g| g.0 == current) {
                g.1 = g.2;
            }
        }
        while order.len() < n {
            let (idx, min) = gauges
                .iter()
                .enumerate()
                .min_by(|a, b| a.1.1.total_cmp(&b.1.1).then(a.1.0.cmp(&b.1.0)))
                .map(|(i, g)| (i, g.1))
                .expect("список не пуст");
            for g in gauges.iter_mut() {
                g.1 = (g.1 - min).max(0.0);
            }
            order.push(gauges[idx].0);
            gauges[idx].1 = gauges[idx].2;
        }
        order
    }

    // ---- Действия ----

    /// Ход текущего бойца.
    pub fn act(&mut self, action: Action) -> Result<Vec<Event>, BattleError> {
        if self.outcome.is_some() {
            return Err(BattleError::BattleOver);
        }
        let actor = self.current.ok_or(BattleError::BattleOver)?;
        let side = self.units[actor.0].side;

        let (ability, kind) = match action.kind {
            ActionKind::Basic => (self.units[actor.0].def.basic.clone(), AbilityKind::Basic),
            ActionKind::Skill => {
                let skill = self.units[actor.0]
                    .def
                    .skill
                    .clone()
                    .ok_or(BattleError::NoSkill)?;
                if !self.units[actor.0].skill_ready() {
                    return Err(BattleError::SkillOnCooldown);
                }
                (skill, AbilityKind::Skill)
            }
        };

        let mut events = Vec::new();
        {
            // Умение уходит на перезарядку, обычная атака приближает его.
            let unit = &mut self.units[actor.0];
            unit.skill_cooldown = match kind {
                AbilityKind::Skill => unit.skill_cooldown_start(),
                _ => unit.skill_cooldown.saturating_sub(1),
            };
        }
        if side == Side::Ally {
            let energy = if kind == AbilityKind::Skill {
                ENERGY_SKILL
            } else {
                ENERGY_BASIC
            };
            self.gain_energy(actor, energy);
        }

        // Сбрасываем шкалу до эффектов, чтобы «ускорить себя» работало.
        {
            let unit = &mut self.units[actor.0];
            unit.action_value = unit.base_action_value();
            unit.turns_taken += 1;
        }

        events.push(Event::AbilityUsed {
            unit: actor,
            kind,
            name: ability.name.clone(),
        });
        self.apply_ability(actor, &ability, action.target, &mut events);

        if side == Side::Ally {
            self.broadside = (self.broadside + BROADSIDE_PER_ACTION).min(BROADSIDE_MAX);
            if self.broadside >= BROADSIDE_MAX {
                self.fire_broadside(&mut events);
            }
        }

        self.check_outcome(&mut events);
        if self.outcome.is_none() {
            self.advance_to_next_turn(&mut events);
        }
        Ok(events)
    }

    /// Ульта союзника. Можно нажать в любой момент, ход не тратится.
    pub fn use_ultimate(
        &mut self,
        unit: UnitId,
        target: Option<UnitId>,
    ) -> Result<Vec<Event>, BattleError> {
        if self.outcome.is_some() {
            return Err(BattleError::BattleOver);
        }
        let u = self.units.get(unit.0).ok_or(BattleError::NotAnAlly)?;
        if u.side != Side::Ally {
            return Err(BattleError::NotAnAlly);
        }
        if !u.is_alive() {
            return Err(BattleError::UnitDefeated);
        }
        let ultimate = u.def.ultimate.clone().ok_or(BattleError::NoUltimate)?;
        if u.energy < ENERGY_MAX {
            return Err(BattleError::UltimateNotReady);
        }

        self.units[unit.0].energy = 0;
        let mut events = vec![Event::AbilityUsed {
            unit,
            kind: AbilityKind::Ultimate,
            name: ultimate.name.clone(),
        }];
        self.apply_ability(unit, &ultimate, target, &mut events);
        self.check_outcome(&mut events);

        // Если ульта добила того, чей был ход, ход переходит дальше.
        if self.outcome.is_none() {
            let current_dead = self.current.is_none_or(|c| !self.units[c.0].is_alive());
            if current_dead {
                self.advance_to_next_turn(&mut events);
            }
        }
        Ok(events)
    }

    /// Один шаг автобоя: готовые ульты, затем ход текущего бойца.
    pub fn auto_step(&mut self) -> Vec<Event> {
        let mut events = Vec::new();
        if self.outcome.is_some() {
            return events;
        }

        let ready: Vec<UnitId> = self
            .alive(Side::Ally)
            .filter(|u| u.ultimate_ready())
            .map(|u| u.id)
            .collect();
        for id in ready {
            if self.outcome.is_some() {
                return events;
            }
            if ai::should_use_ultimate(self, id)
                && let Ok(ev) = self.use_ultimate(id, None)
            {
                events.extend(ev);
            }
        }

        events.extend(self.auto_turn());
        events
    }

    /// Ход текущего бойца по решению ИИ, без ульт: их нажимает игрок.
    pub fn auto_turn(&mut self) -> Vec<Event> {
        let mut events = Vec::new();
        let Some(actor) = self.current else {
            return events;
        };
        let action = ai::choose_action(self, actor);
        match self.act(action) {
            Ok(ev) => events.extend(ev),
            Err(_) => {
                if let Ok(ev) = self.act(Action::basic(None)) {
                    events.extend(ev);
                }
            }
        }
        events
    }

    /// Прогнать бой на автобое до конца или до `max_turns` ходов.
    pub fn run_auto(&mut self, max_turns: u32) -> Option<Outcome> {
        for _ in 0..max_turns {
            if self.outcome.is_some() {
                break;
            }
            self.auto_step();
        }
        self.outcome
    }

    // ---- Внутренние правила ----

    fn advance_to_next_turn(&mut self, events: &mut Vec<Event>) {
        let next = self
            .units
            .iter()
            .filter(|u| u.is_alive())
            .min_by(|a, b| {
                a.action_value
                    .total_cmp(&b.action_value)
                    .then(a.id.cmp(&b.id))
            })
            .map(|u| (u.id, u.action_value));
        let Some((next, value)) = next else {
            self.current = None;
            return;
        };
        for unit in self.units.iter_mut().filter(|u| u.is_alive()) {
            unit.action_value = (unit.action_value - value).max(0.0);
        }
        self.current = Some(next);
        self.turn += 1;
        events.push(Event::TurnStarted { unit: next });
    }

    fn check_outcome(&mut self, events: &mut Vec<Event>) {
        if self.outcome.is_some() {
            return;
        }
        let outcome = if self.alive(Side::Enemy).next().is_none() {
            Some(Outcome::Victory)
        } else if self.alive(Side::Ally).next().is_none() {
            Some(Outcome::Defeat)
        } else {
            None
        };
        if let Some(outcome) = outcome {
            self.outcome = Some(outcome);
            self.current = None;
            events.push(Event::BattleEnded { outcome });
        }
    }

    fn gain_energy(&mut self, id: UnitId, amount: u32) {
        let unit = &mut self.units[id.0];
        if unit.def.ultimate.is_some() {
            unit.energy = (unit.energy + amount * self.energy_rate).min(ENERGY_MAX);
        }
    }

    fn alive_ids(&self, side: Side) -> Vec<UnitId> {
        self.alive(side).map(|u| u.id).collect()
    }

    /// Цель для эффектов «один враг» / «один союзник».
    fn resolve_single(&self, actor: UnitId, side: Side, chosen: Option<UnitId>) -> Option<UnitId> {
        if let Some(id) = chosen
            && let Some(u) = self.units.get(id.0)
            && u.side == side
            && u.is_alive()
        {
            return Some(id);
        }
        if side == self.units[actor.0].side {
            ai::most_wounded(self, side)
        } else {
            ai::pick_enemy_target(self, actor)
        }
    }

    fn apply_ability(
        &mut self,
        actor: UnitId,
        ability: &Ability,
        chosen: Option<UnitId>,
        events: &mut Vec<Event>,
    ) {
        let side = self.units[actor.0].side;
        let single_enemy = self.resolve_single(actor, side.opposite(), chosen);
        let single_ally = self.resolve_single(actor, side, chosen);

        for te in &ability.effects {
            let targets: Vec<UnitId> = match te.target {
                Target::SingleEnemy => single_enemy.into_iter().collect(),
                Target::AllEnemies => self.alive_ids(side.opposite()),
                Target::SingleAlly => single_ally.into_iter().collect(),
                Target::AllAllies => self.alive_ids(side),
                Target::Myself => vec![actor],
            };
            for target in targets {
                if !self.units[target.0].is_alive() {
                    continue;
                }
                match te.effect {
                    Effect::Damage { multiplier } => {
                        let attacker = &self.units[actor.0];
                        let tailwind = if self.wind == Some(attacker.def.element) {
                            1.0 + WIND_BONUS
                        } else {
                            1.0
                        };
                        let raw = attacker.def.stats.atk as f32 * multiplier * tailwind;
                        let weakness = attacker
                            .def
                            .element
                            .is_strong_against(self.units[target.0].def.element);
                        self.hit(Some(actor), target, raw, weakness, events);
                    }
                    Effect::Heal { percent } => {
                        let unit = &mut self.units[target.0];
                        let max = unit.def.stats.max_hp;
                        let amount = ((max as f32 * percent).round() as u32).min(max - unit.hp);
                        unit.hp += amount;
                        events.push(Event::Healed {
                            source: actor,
                            target,
                            amount,
                        });
                    }
                    Effect::Advance { fraction } => {
                        let unit = &mut self.units[target.0];
                        unit.action_value =
                            (unit.action_value - unit.base_action_value() * fraction).max(0.0);
                        events.push(Event::Advanced {
                            unit: target,
                            fraction,
                        });
                    }
                    Effect::Delay { fraction } => {
                        let unit = &mut self.units[target.0];
                        unit.action_value += unit.base_action_value() * fraction;
                        events.push(Event::Delayed {
                            unit: target,
                            fraction,
                        });
                    }
                }
            }
        }
    }

    fn fire_broadside(&mut self, events: &mut Vec<Event>) {
        self.broadside = 0;
        events.push(Event::BroadsideFired);
        let raw = self.ship.broadside_power as f32;
        for target in self.alive_ids(Side::Enemy) {
            self.hit(None, target, raw, false, events);
        }
    }

    fn hit(
        &mut self,
        source: Option<UnitId>,
        target: UnitId,
        raw: f32,
        weakness: bool,
        events: &mut Vec<Event>,
    ) {
        let crit = self.rng.chance(CRIT_CHANCE);
        let variance = self.rng.range_f32(0.95, 1.05);
        let def = self.units[target.0].def.stats.def as f32;
        let mut amount = raw * 100.0 / (100.0 + def) * variance;
        if weakness {
            amount *= WEAKNESS_MULTIPLIER;
        }
        if crit {
            amount *= CRIT_MULTIPLIER;
        }
        let amount = amount.round().max(1.0) as u32;

        let unit = &mut self.units[target.0];
        unit.hp = unit.hp.saturating_sub(amount);
        let (side, alive) = (unit.side, unit.is_alive());
        events.push(Event::Damage {
            source,
            target,
            amount,
            weakness,
            crit,
        });

        if !alive {
            events.push(Event::Defeated { unit: target });
        } else if side == Side::Ally {
            self.gain_energy(target, ENERGY_ON_HIT);
        }
    }
}
