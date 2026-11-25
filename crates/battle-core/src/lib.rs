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
pub mod power;
mod rng;
mod unit;

pub use battle::{AbilityKind, Action, ActionKind, Battle, BattleError, Event, Outcome, Ship};
pub use power::{ship_power, team_power, unit_power};
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

pub const BROADSIDE_MAX: u32 = 100;
/// Заряд залпа за каждое действие союзника.
pub const BROADSIDE_PER_ACTION: u32 = 15;

/// Темп боя: весь урон и лечение умножаются на это число, пороги срыва замаха — тоже.
/// Доли от здоровья (а значит, и расклад сил) не меняются — бой просто короче по ходам.
pub const DAMAGE_SCALE: f32 = 2.0;

pub const CRIT_CHANCE: f32 = 0.10;
pub const CRIT_MULTIPLIER: f32 = 1.5;
pub const WEAKNESS_MULTIPLIER: f32 = 1.5;
/// Ветер боя: бойцы стихии ветра бьют сильнее на эту долю (обе стороны, залп — нет).
pub const WIND_BONUS: f32 = 0.25;
/// Ветер боя меняется на другую стихию раз в столько ходов (если у боя есть ветер).
pub const WIND_SHIFT_TURNS: u32 = 6;
/// Замах сорван, если за время замаха боец получил урон не меньше этой доли здоровья.
pub const CHARGE_BREAK: f32 = 0.12;
/// Замах длится столько обычных ходов бойца — окно, чтобы его сорвать.
pub const CHARGE_TIME: f32 = 1.6;
/// Сорванный замах оглушает: боец отброшен по шкале ходов на эту долю своего хода.
pub const CHARGE_STUN: f32 = 0.6;
