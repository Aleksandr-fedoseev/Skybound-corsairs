//! Логика пошагового боя Skybound Corsairs.
//!
//! Крейт не зависит от движка: его можно тестировать в терминале, прогонять баланс
//! тысячами автобоёв и запускать на сервере для проверки результатов.
//!
//! Основной цикл использования:
//! 1. [`Battle::new`] — создать бой.
//! 2. [`Battle::current`] — чей ход.
//! 3. [`Battle::act`] / [`Battle::use_ultimate`] / [`Battle::auto_step`] — действия.
//! 4. Проиграть полученные [`Event`] как анимации.

pub mod ai;
mod battle;
mod rng;
mod unit;

pub use battle::{AbilityKind, Action, ActionKind, Battle, BattleError, Event, Outcome, Ship};
pub use rng::Rng;
pub use unit::{
    Ability, Effect, Element, Role, Side, Stats, Target, TargetedEffect, Unit, UnitDef, UnitId,
};

/// Полная шкала хода. Длина хода бойца = `ACTION_GAUGE / скорость`.
pub const ACTION_GAUGE: f32 = 10_000.0;

pub const ENERGY_MAX: u32 = 100;
pub const ENERGY_BASIC: u32 = 20;
pub const ENERGY_SKILL: u32 = 30;
/// Энергия за полученный удар (если союзник выжил).
pub const ENERGY_ON_HIT: u32 = 5;

pub const SKILL_POINTS_MAX: u32 = 5;
pub const SKILL_POINTS_START: u32 = 3;

pub const BROADSIDE_MAX: u32 = 100;
/// Заряд залпа за каждое действие союзника.
pub const BROADSIDE_PER_ACTION: u32 = 15;

pub const CRIT_CHANCE: f32 = 0.10;
pub const CRIT_MULTIPLIER: f32 = 1.5;
pub const WEAKNESS_MULTIPLIER: f32 = 1.5;
