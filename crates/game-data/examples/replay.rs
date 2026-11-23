//! Один автобой текстом: кто ходит, что делает, сколько урона.
//!
//! Запуск: `cargo run -p game-data --example replay -- [этап] [уровень] [seed]`

use std::path::Path;

use battle_core::{AbilityKind, Battle, Event, Ship};
use game_data::GameData;

const TEAM: [&str; 4] = ["ren", "kaya", "borg", "mei"];

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let stage = args.next().unwrap_or_else(|| "1-1".into());
    let level: u32 = args.next().map(|s| s.parse()).transpose()?.unwrap_or(1);
    let seed: u64 = args.next().map(|s| s.parse()).transpose()?.unwrap_or(1);

    let data = GameData::load_dir(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data"))?;
    let team = TEAM
        .iter()
        .map(|id| {
            data.hero(id)
                .ok_or(format!("нет героя {id}"))
                .map(|h| h.to_unit(level))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let mut battle = Battle::new(
        team,
        data.stage_enemies(&stage)?,
        Ship {
            broadside_power: 120,
        },
        seed,
    );
    battle.set_wind(data.stage(&stage).and_then(|s| s.wind));

    let hp = |b: &Battle| {
        b.units()
            .iter()
            .map(|u| format!("{} {}/{}", u.def.name, u.hp, u.def.stats.max_hp))
            .collect::<Vec<_>>()
            .join(", ")
    };
    println!("Старт: {}", hp(&battle));
    if let Some(first) = battle.current() {
        println!("\nХод 1: {}", battle.unit(first).def.name);
    }

    while battle.outcome().is_none() && battle.turn() < 500 {
        for event in battle.auto_step() {
            let name = |id| battle.unit(id).def.name.clone();
            match event {
                Event::TurnStarted { unit } => println!("\nХод {}: {}", battle.turn(), name(unit)),
                Event::AbilityUsed {
                    unit,
                    kind,
                    name: ability,
                } => {
                    let tag = match kind {
                        AbilityKind::Basic => "",
                        AbilityKind::Skill => " [умение]",
                        AbilityKind::Ultimate => " [УЛЬТА]",
                    };
                    println!("  {} — {ability}{tag}", name(unit));
                }
                Event::Damage {
                    source,
                    target,
                    amount,
                    weakness,
                    crit,
                } => println!(
                    "    {} -> {}: {amount}{}{}",
                    source.map(name).unwrap_or_else(|| "Залп".into()),
                    name(target),
                    if weakness { " (слабость)" } else { "" },
                    if crit { " (крит)" } else { "" },
                ),
                Event::Healed { target, amount, .. } => println!("    +{amount} {}", name(target)),
                Event::Advanced { unit, fraction } => {
                    println!("    {} ускорен на {fraction}", name(unit))
                }
                Event::Delayed { unit, fraction } => {
                    println!("    {} отброшен на {fraction}", name(unit))
                }
                Event::Defeated { unit } => println!("    {} повержен", name(unit)),
                Event::BroadsideFired => println!("  ЗАЛП КОРАБЛЯ"),
                Event::WindChanged { wind, by } => match by {
                    Some(id) => println!("  {} разворачивает ветер: {wind:?}", name(id)),
                    None => println!("  Ветер меняется: {wind:?}"),
                },
                Event::BattleEnded { outcome } => println!("\nИтог: {outcome:?}"),
            }
        }
    }
    println!("Финиш: {}", hp(&battle));
    Ok(())
}
