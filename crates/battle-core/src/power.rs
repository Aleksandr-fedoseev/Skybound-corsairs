//! Мощь — одно число, по которому видно, насколько боец, отряд или корабль силён.
//!
//! Это оценка, а не симуляция: живучесть (здоровье с поправкой на защиту) и натиск (атака,
//! скорость и множители умений) сводятся в √(живучесть × натиск). Число растёт почти
//! линейно с уровнем, поэтому его удобно сравнивать: «нужно 7 000 · у вас 7 400».
//! Рекомендуемая мощь боя считается тем же способом — по эталонному отряду на рекомендуемом уровне.

use crate::BROADSIDE_MAX;
use crate::unit::{Ability, Effect, Target, UnitDef};

/// Чтобы герой 1-го уровня был около тысячи.
const SCALE: f32 = 2.4;
/// Удар по всем стоит, как по стольким целям сразу.
const AREA_TARGETS: f32 = 2.5;
/// Лечение 30% — как удар с множителем 1.2.
const HEAL_WORTH: f32 = 4.0;
/// Ульта выходит примерно раз в столько ходов.
const ULT_EVERY: f32 = 6.0;
/// Сколько врагов в среднем под залпом.
const BROADSIDE_TARGETS: f32 = 4.0;

/// Насколько сильно действие: сумма множителей урона и лечения с учётом числа целей.
fn ability_worth(a: &Ability) -> f32 {
    a.effects
        .iter()
        .map(|e| {
            let area = match e.target {
                Target::AllEnemies | Target::AllAllies => AREA_TARGETS,
                _ => 1.0,
            };
            match e.effect {
                Effect::Damage { multiplier } => multiplier * area,
                Effect::Heal { percent } => percent * HEAL_WORTH * area,
                _ => 0.0,
            }
        })
        .sum()
}

/// Мощь одного бойца.
pub fn unit_power(u: &UnitDef) -> u32 {
    let s = &u.stats;
    let toughness = s.max_hp as f32 * (1.0 + s.def as f32 / 100.0);
    let mut per_turn = ability_worth(&u.basic);
    if let Some(skill) = &u.skill {
        // Умение заменяет обычную атаку раз в `cooldown` ходов.
        let every = skill.cooldown.max(1) as f32;
        per_turn += (ability_worth(skill) - ability_worth(&u.basic)).max(0.0) / every;
    }
    if let Some(ult) = &u.ultimate {
        per_turn += ability_worth(ult) / ULT_EVERY;
    }
    let pressure = s.atk as f32 * per_turn * s.spd as f32 / 100.0;
    ((toughness * pressure).sqrt() * SCALE).round() as u32
}

/// Мощь отряда — сумма бойцов.
pub fn team_power(units: &[UnitDef]) -> u32 {
    units.iter().map(unit_power).sum()
}

/// Мощь корабля: залп бьёт по всем врагам раз в несколько действий отряда.
pub fn ship_power(broadside_power: u32) -> u32 {
    let volleys_per_turn = crate::BROADSIDE_PER_ACTION as f32 / BROADSIDE_MAX as f32;
    (broadside_power as f32 * BROADSIDE_TARGETS * volleys_per_turn * SCALE * 2.0).round() as u32
}
