//! Прогон баланса: процент побед стартовой команды на каждом этапе.
//!
//! Запуск: `cargo run -p game-data --example simulate --release -- [уровень] [число боёв]`
//! Данные читаются из папки data/, так что баланс можно править без пересборки.

use std::path::Path;

use battle_core::{Battle, Event, Outcome, Ship, Side};
use game_data::GameData;

const TEAM: [&str; 4] = ["borg", "ren", "kaya", "mei"];

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let level: u32 = args.next().map(|s| s.parse()).transpose()?.unwrap_or(1);
    let runs: u64 = args.next().map(|s| s.parse()).transpose()?.unwrap_or(1000);

    let data_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data");
    let data = GameData::load_dir(&data_dir)?;
    let team: Vec<_> = TEAM
        .iter()
        .map(|id| {
            data.hero(id)
                .ok_or(format!("нет героя {id}"))
                .map(|h| h.to_unit(level))
        })
        .collect::<Result<_, _>>()?;

    println!(
        "Команда: {} (уровень {level}), боёв на этап: {runs}\n",
        TEAM.join(", ")
    );
    println!(
        "{:<8} {:<26} {:>8} {:>11} {:>12} {:>10}",
        "Этап", "Название", "Победы", "Ходов в ср.", "HP команды", "Потери"
    );
    for stage in &data.stages {
        let enemies = data.stage_enemies(&stage.id)?;
        let mut wins = 0;
        let mut turns = 0u64;
        let mut hp_left = 0.0f64;
        let mut losses = 0u64;
        for seed in 0..runs {
            let mut battle = Battle::new(
                team.clone(),
                enemies.clone(),
                Ship {
                    broadside_power: 120,
                },
                seed,
            );
            battle.set_wind(stage.wind);
            let mut turn_count = 0;
            while battle.outcome().is_none() && turn_count < 500 {
                turn_count += battle
                    .auto_step()
                    .iter()
                    .filter(|e| matches!(e, Event::TurnStarted { .. }))
                    .count() as u64;
            }
            if battle.outcome() == Some(Outcome::Victory) {
                wins += 1;
            }
            turns += turn_count;
            let (hp, max) = battle
                .units()
                .iter()
                .filter(|u| u.side == Side::Ally)
                .fold((0u64, 0u64), |(h, m), u| {
                    (h + u.hp as u64, m + u.def.stats.max_hp as u64)
                });
            hp_left += hp as f64 / max as f64;
            losses += battle
                .units()
                .iter()
                .filter(|u| u.side == Side::Ally && !u.is_alive())
                .count() as u64;
        }
        println!(
            "{:<8} {:<26} {:>7.1}% {:>11.1} {:>11.0}% {:>10.2}",
            stage.id,
            stage.name,
            100.0 * wins as f64 / runs as f64,
            turns as f64 / runs as f64,
            100.0 * hp_left / runs as f64,
            losses as f64 / runs as f64
        );
    }
    Ok(())
}
