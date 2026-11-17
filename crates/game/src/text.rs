//! Тексты для игрока. Позже здесь будет локализация.

use battle_core::{AbilityKind, Battle, Element, Event, Outcome};

pub fn element_name(element: Element) -> &'static str {
    match element {
        Element::Wind => "Ветер",
        Element::Lightning => "Молния",
        Element::Powder => "Порох",
        Element::Steel => "Сталь",
        Element::Mist => "Туман",
    }
}

/// Строка боевого журнала для события. `None` — событие не показываем.
pub fn describe(battle: &Battle, event: &Event) -> Option<String> {
    let name = |id| battle.unit(id).def.name.as_str();
    let line = match event {
        Event::AbilityUsed {
            unit,
            kind,
            name: ability,
        } => match kind {
            AbilityKind::Ultimate => format!("{} — УЛЬТА «{ability}»!", name(*unit)),
            AbilityKind::Skill => format!("{} — умение «{ability}»", name(*unit)),
            AbilityKind::Basic => format!("{} — «{ability}»", name(*unit)),
        },
        Event::Damage {
            source,
            target,
            amount,
            weakness,
            crit,
        } => {
            let mut tags = String::new();
            if *weakness {
                tags.push_str(" слабость!");
            }
            if *crit {
                tags.push_str(" крит!");
            }
            match source {
                Some(_) => format!("    {} -{amount}{tags}", name(*target)),
                None => format!("    залп по {} -{amount}{tags}", name(*target)),
            }
        }
        Event::Healed { target, amount, .. } => format!("    {} +{amount}", name(*target)),
        Event::Advanced { unit, fraction } => {
            format!("    {} ускорен на {:.0}%", name(*unit), fraction * 100.0)
        }
        Event::Delayed { unit, fraction } => {
            format!("    {} отброшен на {:.0}%", name(*unit), fraction * 100.0)
        }
        Event::Defeated { unit } => format!("    {} повержен", name(*unit)),
        Event::BroadsideFired => "ЗАЛП КОРАБЛЯ!".to_string(),
        Event::BattleEnded {
            outcome: Outcome::Victory,
        } => "ПОБЕДА!".to_string(),
        Event::BattleEnded {
            outcome: Outcome::Defeat,
        } => "ПОРАЖЕНИЕ...".to_string(),
        Event::TurnStarted { .. } | Event::SkillPointsChanged { .. } => return None,
    };
    Some(line)
}
