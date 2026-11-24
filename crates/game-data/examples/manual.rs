//! Ручной бой против автобоя: тот же отряд и seed, разная тактика ульт.
//!
//! Запуск: `cargo run -p game-data --example manual --release -- [этап] [боёв]`
//! Автобой жмёт ульты, как только они готовы. «Вдумчивый игрок» придерживает ульты с уроном,
//! когда у врага скоро замах, и сбивает замах всем сразу; лечит, только когда кто-то ранен.
//! Печатает долю побед обоих по уровням отряда.

use std::path::Path;

use battle_core::{Battle, Effect, Outcome, Ship, Side, UnitDef};
use game_data::GameData;

const TEAM: [&str; 4] = ["borg", "ren", "kaya", "mei"];

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let stage_id = args.next().unwrap_or_else(|| "1-boss".into());
    let runs: u64 = args.next().map(|s| s.parse()).transpose()?.unwrap_or(300);
    let data = GameData::load_dir(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data"))?;
    let stage = data.stage(&stage_id).ok_or("нет такого этапа")?;
    let enemies = data.stage_enemies(&stage_id)?;
    println!(
        "{} {} · рек. уровень {} · боёв на точку {runs}\n",
        stage.id, stage.name, stage.recommended_level
    );
    println!("{:>7} {:>9} {:>9}", "уровень", "автобой", "вручную");
    let from = stage.recommended_level.saturating_sub(2).max(1);
    for level in from..=stage.recommended_level + 4 {
        let team: Vec<UnitDef> = TEAM
            .iter()
            .map(|id| data.hero(id).expect("герой").to_unit(level))
            .collect();
        let rate = |smart: bool| {
            (0..runs)
                .filter(|&seed| {
                    let mut b = Battle::new(
                        team.clone(),
                        enemies.clone(),
                        Ship {
                            broadside_power: 120,
                        },
                        seed,
                    );
                    b.set_wind(stage.wind);
                    play(&mut b, smart) == Some(Outcome::Victory)
                })
                .count() as f64
                / runs as f64
        };
        CHARGES.store(0, Relaxed);
        BROKEN.store(0, Relaxed);
        let auto = rate(false);
        let (ca, ba) = (CHARGES.swap(0, Relaxed), BROKEN.swap(0, Relaxed));
        let manual = rate(true);
        let (cm, bm) = (CHARGES.load(Relaxed), BROKEN.load(Relaxed));
        println!(
            "{level:>7} {:>8.0}% {:>8.0}%   сорвано замахов: авто {ba}/{ca}, вручную {bm}/{cm}",
            auto * 100.0,
            manual * 100.0
        );
    }
    Ok(())
}

use std::sync::atomic::{AtomicU64, Ordering::Relaxed};

/// Сколько замахов было и сколько сорвано (для настройки порога срыва).
static CHARGES: AtomicU64 = AtomicU64::new(0);
static BROKEN: AtomicU64 = AtomicU64::new(0);

fn count(events: &[battle_core::Event]) {
    for e in events {
        match e {
            battle_core::Event::Charging { .. } => {
                CHARGES.fetch_add(1, Relaxed);
            }
            battle_core::Event::ChargeBroken { .. } => {
                BROKEN.fetch_add(1, Relaxed);
            }
            _ => {}
        }
    }
}

fn play(b: &mut Battle, smart: bool) -> Option<Outcome> {
    let mut steps = 0;
    while b.outcome().is_none() && steps < 3000 {
        steps += 1;
        if !smart {
            count(&b.auto_step());
            continue;
        }
        let charging = b
            .units()
            .iter()
            .any(|u| u.side == Side::Enemy && u.is_alive() && u.charging.is_some());
        let soon = b.units().iter().any(|u| {
            u.side == Side::Enemy
                && u.is_alive()
                && u.charging.is_none()
                && u.def.skill.as_ref().is_some_and(|s| s.charge)
                && u.skill_cooldown <= 1
        });
        let wounded = b
            .units()
            .iter()
            .any(|u| u.side == Side::Ally && u.is_alive() && u.hp_ratio() < 0.5);
        let ready: Vec<_> = b
            .units()
            .iter()
            .filter(|u| u.side == Side::Ally && u.ultimate_ready())
            .map(|u| u.id)
            .collect();
        for id in ready {
            let ult = b.unit(id).def.ultimate.clone().expect("ульта");
            let damage = ult
                .effects
                .iter()
                .any(|e| matches!(e.effect, Effect::Damage { .. }));
            let use_it = if ult.heals() && !damage {
                wounded
            } else {
                charging || !soon
            };
            if use_it
                && b.outcome().is_none()
                && let Ok(ev) = b.use_ultimate(id, None)
            {
                count(&ev);
            }
        }
        count(&b.auto_turn());
    }
    b.outcome()
}
