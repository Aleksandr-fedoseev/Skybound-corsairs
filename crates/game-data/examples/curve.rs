//! Кривая сложности: с какого уровня стартовый отряд (автобой) берёт каждый бой.
//!
//! Запуск: `cargo run -p game-data --example curve --release -- [число боёв]`
//! Для этапов главы, конвоев и этажей Бездны печатает уровень, с которого побед ≥ 50% и ≥ 80%.
//! Второе число — то, что стоит писать в `recommended_level`.
//! У боссов с замахом `recommended_level` — по ручной игре (`--example manual`): автобою нужно
//! на 1–2 уровня больше, и здесь он это покажет.
//! `-- [число боёв] abyss` — только Бездна, этажи 1–60 (таблица для tools/economy.py).

use std::path::Path;

use battle_core::{Battle, Element, Outcome, Ship, UnitDef};
use game_data::GameData;
use serde_json::Value;

const TEAM: [&str; 4] = ["borg", "ren", "kaya", "mei"];
const MAX_LEVEL: u32 = 60;

/// Бой: подпись, враги (id, уровень), уровень из данных, ветер.
type Fight = (String, Vec<(String, u32)>, Option<u64>, Option<Element>);

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let runs: u64 = std::env::args()
        .nth(1)
        .map(|s| s.parse())
        .transpose()?
        .unwrap_or(200);
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data");
    let data = GameData::load_dir(&dir)?;
    let read = |name: &str| -> Result<Value, Box<dyn std::error::Error>> {
        Ok(serde_json::from_str(&std::fs::read_to_string(
            dir.join(name),
        )?)?)
    };

    let mut fights: Vec<Fight> = Vec::new();
    for stage in &data.stages {
        let enemies = stage
            .enemies
            .iter()
            .map(|e| (e.enemy.clone(), e.level))
            .collect();
        fights.push((
            format!("{} {}", stage.id, stage.name),
            enemies,
            Some(stage.recommended_level as u64),
            stage.wind,
        ));
    }
    let convoys = read("convoys.json")?;
    for tier in convoys["tiers"].as_array().into_iter().flatten() {
        fights.push((
            format!(
                "конвой {} (с {} этапов)",
                tier["name"].as_str().unwrap_or("?"),
                tier["from_cleared"]
            ),
            pairs(&tier["enemies"]),
            tier["recommended_level"].as_u64(),
            None,
        ));
    }
    // Состав этажа — как в crates/game/src/abyss.rs (AbyssData::enemies).
    let abyss = read("abyss.json")?;
    let n = |k: &str| abyss[k].as_f64().unwrap_or(0.0);
    let pool: Vec<String> = abyss["enemy_pool"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|v| v.as_str().map(String::from))
        .collect();
    let abyss_only = std::env::args().nth(2).as_deref() == Some("abyss");
    if abyss_only {
        fights.clear();
    }
    let floors: Vec<u32> = if abyss_only {
        (1..=60).collect()
    } else {
        vec![1, 3, 5, 7, 9, 10, 11, 13, 15, 19, 20, 21, 25, 29, 30, 40]
    };
    for floor in floors {
        let count = (n("base_count") as u32 + (floor - 1) / n("more_every") as u32)
            .min(n("max_count") as u32);
        let extra = count - n("base_count") as u32;
        let level = (n("base_level") as u32 + ((floor - 1) as f64 * n("level_per_floor")) as u32)
            .saturating_sub(extra * n("level_drop_per_extra") as u32)
            .max(1);
        let mut enemies: Vec<(String, u32)> = (0..count)
            .map(|i| (pool[((floor + i) as usize) % pool.len()].clone(), level))
            .collect();
        if floor.is_multiple_of(n("boss_every") as u32) {
            enemies.truncate(2);
            let boss = abyss["boss"].as_str().unwrap_or("").to_string();
            enemies.insert(1, (boss, level.saturating_sub(3).max(1)));
        }
        fights.push((format!("Бездна, этаж {floor}"), enemies, None, None));
    }

    println!("Отряд: {} · боёв на точку: {runs}\n", TEAM.join(", "));
    println!("{:<42} {:>6} {:>6} {:>6}", "Бой", "50%", "80%", "в данных");
    for (name, enemies, written, wind) in fights {
        let foes: Vec<UnitDef> = enemies
            .iter()
            .map(|(id, level)| {
                data.enemy(id)
                    .unwrap_or_else(|| panic!("нет врага {id}"))
                    .to_unit(*level)
            })
            .collect();
        let mut at50 = None;
        let mut at80 = None;
        for level in 1..=MAX_LEVEL {
            let rate = win_rate(&data, level, &foes, wind, runs);
            if at50.is_none() && rate >= 0.5 {
                at50 = Some(level);
            }
            if rate >= 0.8 {
                at80 = Some(level);
                break;
            }
        }
        let show = |v: Option<u32>| v.map_or(format!(">{MAX_LEVEL}"), |l| l.to_string());
        println!(
            "{name:<42} {:>6} {:>6} {:>6}",
            show(at50),
            show(at80),
            written.map_or("—".to_string(), |w| w.to_string())
        );
    }
    Ok(())
}

fn pairs(v: &Value) -> Vec<(String, u32)> {
    v.as_array()
        .into_iter()
        .flatten()
        .filter_map(|p| Some((p[0].as_str()?.to_string(), p[1].as_u64()? as u32)))
        .collect()
}

fn win_rate(
    data: &GameData,
    level: u32,
    foes: &[UnitDef],
    wind: Option<Element>,
    runs: u64,
) -> f64 {
    let team: Vec<UnitDef> = TEAM
        .iter()
        .map(|id| data.hero(id).expect("герой из TEAM").to_unit(level))
        .collect();
    let wins = (0..runs)
        .filter(|&seed| {
            let mut battle = Battle::new(
                team.clone(),
                foes.to_vec(),
                Ship {
                    broadside_power: 120,
                },
                seed,
            );
            battle.set_wind(wind);
            let mut steps = 0;
            while battle.outcome().is_none() && steps < 2000 {
                battle.auto_step();
                steps += 1;
            }
            battle.outcome() == Some(Outcome::Victory)
        })
        .count();
    wins as f64 / runs as f64
}
