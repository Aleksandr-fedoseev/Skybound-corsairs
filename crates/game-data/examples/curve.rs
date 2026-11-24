//! Кривая сложности: с какого уровня стартовый отряд (автобой) берёт каждый бой.
//!
//! Запуск: `cargo run -p game-data --example curve --release -- [число боёв]`
//! Для этапов главы, конвоев и этажей Бездны печатает уровень, с которого побед ≥ 50% и ≥ 80%.
//! Второе число — то, что стоит писать в `recommended_level`.
//! У боссов с замахом `recommended_level` — по ручной игре (`--example manual`): автобою нужно
//! на 1–2 уровня больше, и здесь он это покажет.
//! `-- [число боёв] abyss` — только Бездна, этажи 1–60 (таблица для tools/economy.py).
//! `-- [число боёв] ruins` — спуск в Руины подряд с переносом здоровья при 10, 15 и 20 этапах.
//! `-- [число боёв] arena` — доля побед отряда над соперниками Арены своего уровня ±.

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
    let mode = std::env::args().nth(2);
    if mode.as_deref() == Some("arena") {
        return arena(&data, &read("arena.json")?, runs);
    }
    if mode.as_deref() == Some("ruins") {
        return ruins(&data, &read("ruins.json")?, runs);
    }
    let abyss_only = mode.as_deref() == Some("abyss");
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

/// Руины — весь спуск подряд, как в игре: здоровье переходит из боя в бой, павшие выбывают
/// (без залов: ни родника, ни хранилища). Для каждого уровня отряда — средняя глубина и доля
/// полных спусков. «Рек.» — рекомендуемый уровень следующего этапа главы.
fn ruins(data: &GameData, ruins: &Value, runs: u64) -> Result<(), Box<dyn std::error::Error>> {
    let base = ruins["base_level"].as_f64().unwrap_or(0.0);
    let per = ruins["level_per_stage"].as_f64().unwrap_or(0.0);
    let floors: Vec<&Value> = ruins["floors"].as_array().into_iter().flatten().collect();
    for cleared in [10usize, 15, 20] {
        let rec = data
            .stages
            .get(cleared)
            .or(data.stages.last())
            .map_or(1, |s| s.recommended_level);
        let foes: Vec<Vec<UnitDef>> = floors
            .iter()
            .map(|f| {
                let add = f["add"].as_i64().unwrap_or(0);
                let level = ((base + per * cleared as f64).round() as i64 + add).max(1) as u32;
                f["enemies"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|e| Some(data.enemy(e.as_str()?)?.to_unit(level)))
                    .collect()
            })
            .collect();
        println!("Руины при {cleared} пройденных этапах (рек. ур. {rec}):");
        for level in rec.saturating_sub(4).max(1)..=rec + 6 {
            let mut depth_sum = 0;
            let mut cleared_all = 0;
            let mut reached = vec![0u32; foes.len()];
            for seed in 0..runs {
                let mut team: Vec<(UnitDef, f32)> = TEAM
                    .iter()
                    .map(|id| (data.hero(id).expect("герой").to_unit(level), 1.0))
                    .collect();
                let mut depth = 0;
                for (i, enemies) in foes.iter().enumerate() {
                    let alive: Vec<(UnitDef, f32)> =
                        team.iter().filter(|(_, f)| *f > 0.0).cloned().collect();
                    let mut battle = Battle::new(
                        alive.iter().map(|(u, _)| u.clone()).collect(),
                        enemies.clone(),
                        Ship {
                            broadside_power: 120,
                        },
                        seed * 31 + i as u64,
                    );
                    for (k, (u, f)) in alive.iter().enumerate() {
                        battle.set_hp(
                            battle_core::UnitId(k),
                            (u.stats.max_hp as f32 * f).round() as u32,
                        );
                    }
                    let mut steps = 0;
                    while battle.outcome().is_none() && steps < 2000 {
                        battle.auto_step();
                        steps += 1;
                    }
                    if battle.outcome() != Some(Outcome::Victory) {
                        break;
                    }
                    reached[i] += 1;
                    depth = i + 1;
                    team = alive
                        .iter()
                        .enumerate()
                        .map(|(k, (u, _))| {
                            let hp = battle.unit(battle_core::UnitId(k)).hp;
                            (u.clone(), hp as f32 / u.stats.max_hp as f32)
                        })
                        .collect();
                }
                depth_sum += depth;
                if depth == foes.len() {
                    cleared_all += 1;
                }
            }
            let floors_line: Vec<String> = reached
                .iter()
                .map(|r| format!("{:>3}", r * 100 / runs as u32))
                .collect();
            println!(
                "  ур. {level:>2}: глубина {:.1}, до конца {:>3}% · по этажам % {}",
                depth_sum as f64 / runs as f64,
                cleared_all * 100 / runs,
                floors_line.join("")
            );
        }
    }
    Ok(())
}

/// Арена: отряд уровня L против случайных четвёрок героев уровня L + поправка ступени.
fn arena(data: &GameData, arena: &Value, runs: u64) -> Result<(), Box<dyn std::error::Error>> {
    let heroes: Vec<&str> = data.heroes.iter().map(|h| h.id.as_str()).collect();
    println!("Арена: доля побед стартового отряда (автобой, без звёзд и снаряжения)\n");
    for level in [12u32, 20, 30] {
        for tier in arena["rivals"].as_array().into_iter().flatten() {
            let shift = tier["level"].as_i64().unwrap_or(0);
            let foe_level = (level as i64 + shift).max(1) as u32;
            let mut wins = 0.0;
            let teams = 12;
            for t in 0..teams {
                // Четыре разных героя по кругу — без генератора случайных чисел.
                let foes: Vec<UnitDef> = (0..4)
                    .map(|k| {
                        let id = heroes[(t * 3 + k * 2) % heroes.len()];
                        data.hero(id).expect("герой").to_unit(foe_level)
                    })
                    .collect();
                wins += win_rate(data, level, &foes, None, runs / teams as u64 + 1);
            }
            println!(
                "ур. {level:>2} против «{}» (ур. {foe_level}): {:.0}% побед",
                tier["name"].as_str().unwrap_or("?"),
                wins / teams as f64 * 100.0
            );
        }
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
