//! Решения автобоя. Те же функции выбирают цель по умолчанию при ручном управлении.

use crate::battle::{Action, Battle};
use crate::unit::{Effect, Side, UnitId};

/// Как часто враг с умением использует его: каждый N-й ход.
const ENEMY_SKILL_EVERY: u32 = 3;
/// Лечащее умение используется, если у кого-то из своих здоровья меньше этой доли.
const HEAL_SKILL_THRESHOLD: f32 = 0.6;
/// Лечащая ульта используется, если у кого-то из своих здоровья меньше этой доли.
const HEAL_ULT_THRESHOLD: f32 = 0.7;

pub fn choose_action(battle: &Battle, actor: UnitId) -> Action {
    let unit = battle.unit(actor);
    let enemy_target = pick_enemy_target(battle, actor);

    match unit.side {
        Side::Ally => {
            if let Some(skill) = &unit.def.skill {
                if skill.heals() {
                    let wounded = most_wounded(battle, Side::Ally);
                    let needs_heal =
                        wounded.is_some_and(|id| battle.unit(id).hp_ratio() < HEAL_SKILL_THRESHOLD);
                    if needs_heal && battle.skill_points() >= 1 {
                        return Action::skill(wounded);
                    }
                } else if battle.skill_points() >= 2 {
                    // Одно очко оставляем в запасе для лечения.
                    return Action::skill(enemy_target);
                }
            }
            Action::basic(enemy_target)
        }
        Side::Enemy => {
            if unit.def.skill.is_some()
                && unit.turns_taken % ENEMY_SKILL_EVERY == ENEMY_SKILL_EVERY - 1
            {
                Action::skill(enemy_target)
            } else {
                Action::basic(enemy_target)
            }
        }
    }
}

/// Цель атаки: сначала те, у кого слабость к стихии атакующего, затем с наименьшим здоровьем.
pub fn pick_enemy_target(battle: &Battle, actor: UnitId) -> Option<UnitId> {
    let attacker = battle.unit(actor);
    battle
        .alive(attacker.side.opposite())
        .min_by_key(|u| {
            (
                !attacker.def.element.is_strong_against(u.def.element),
                u.hp,
                u.id,
            )
        })
        .map(|u| u.id)
}

/// Союзник с наименьшей долей здоровья.
pub fn most_wounded(battle: &Battle, side: Side) -> Option<UnitId> {
    battle
        .alive(side)
        .min_by(|a, b| a.hp_ratio().total_cmp(&b.hp_ratio()).then(a.id.cmp(&b.id)))
        .map(|u| u.id)
}

pub fn should_use_ultimate(battle: &Battle, id: UnitId) -> bool {
    let Some(ult) = &battle.unit(id).def.ultimate else {
        return false;
    };
    let deals_damage = ult
        .effects
        .iter()
        .any(|e| matches!(e.effect, Effect::Damage { .. }));
    if ult.heals() && !deals_damage {
        return most_wounded(battle, Side::Ally)
            .is_some_and(|w| battle.unit(w).hp_ratio() < HEAL_ULT_THRESHOLD);
    }
    true
}
