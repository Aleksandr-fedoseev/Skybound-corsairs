//! Переподбор уровней врагов под новый темп боя (`battle_core::DAMAGE_SCALE`).
//!
//! Запуск: `cargo run -p game-data --example retune --release -- <старый темп> [число боёв]`
//! Для каждого боя (этапы, конвои, этапы Регаты; Блокада и Руины — формулой, по `curve`) сначала ищем уровень
//! стартового отряда, с которого он берёт бой в 80% автобоёв при старом темпе, затем сдвигаем
//! уровни всех врагов боя так, чтобы при новом темпе порог остался тем же. Последней строкой
//! печатает `RESULT {json}` — новые уровни по боям; `tools/apply_retune.py` вписывает их в `data/`,
//! не трогая формат файлов. Рекомендуемые уровни и прогрессия не меняются.

use std::path::Path;

use battle_core::{Battle, Element, Outcome, Ship, UnitDef};
use game_data::GameData;
use serde_json::Value;

const TEAM: [&str; 4] = ["borg", "ren", "kaya", "mei"];
const MAX_LEVEL: u32 = 70;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let old_scale: f32 = std::env::args()
        .nth(1)
        .ok_or("укажите старый темп, например 1.0")?
        .parse()?;
    let runs: u64 = std::env::args()
        .nth(2)
        .map(|s| s.parse())
        .transpose()?
        .unwrap_or(150);
    let new_scale = battle_core::DAMAGE_SCALE;
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data");
    let data = GameData::load_dir(&dir)?;
    let tuner = Tuner {
        data: &data,
        runs,
        old_scale,
        new_scale,
    };

    // `-- <старый темп> <боёв> abyss` — только Бездна: порог по этажам при старом темпе и сдвиг
    // уровня врагов при новом (параметры формулы в data/abyss.json подбираются по этой таблице).
    if std::env::args().nth(3).as_deref() == Some("abyss") {
        let abyss = read(&dir, "abyss.json")?;
        let n = |k: &str| abyss[k].as_f64().unwrap_or(0.0);
        let pool: Vec<String> = abyss["enemy_pool"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|v| v.as_str().map(String::from))
            .collect();
        for floor in [1u32, 5, 10, 15, 20, 30, 40, 50, 60] {
            let count = (n("base_count") as u32 + (floor - 1) / n("more_every") as u32)
                .min(n("max_count") as u32);
            let extra = count - n("base_count") as u32;
            let level = (n("base_level") as u32
                + ((floor - 1) as f64 * n("level_per_floor")) as u32)
                .saturating_sub(extra * n("level_drop_per_extra") as u32)
                .max(1);
            let mut foes: Vec<(String, u32)> = (0..count)
                .map(|i| (pool[((floor + i) as usize) % pool.len()].clone(), level))
                .collect();
            if floor.is_multiple_of(n("boss_every") as u32) {
                foes.truncate(2);
                let boss = abyss["boss"].as_str().unwrap_or("").to_string();
                foes.insert(1, (boss, level.saturating_sub(3).max(1)));
            }
            tuner.retune(
                &format!("Бездна, этаж {floor} (ур. {level})"),
                &mut foes,
                None,
            );
        }
        return Ok(());
    }
    let mut result = serde_json::Map::new();
    // Этапы глав: враги — {"enemy", "level"}.
    let mut stages: Value = read(&dir, "stages.json")?;
    for stage in stages.as_array_mut().ok_or("stages.json — список")? {
        let wind = stage["wind"]
            .as_str()
            .and_then(|w| serde_json::from_value::<Element>(Value::String(w.into())).ok());
        let mut foes: Vec<(String, u32)> = stage["enemies"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|e| {
                (
                    e["enemy"].as_str().unwrap_or("").to_string(),
                    e["level"].as_u64().unwrap_or(1) as u32,
                )
            })
            .collect();
        let name = stage["id"].as_str().unwrap_or("?").to_string();
        tuner.retune(&name, &mut foes, wind);
        for (e, (_, level)) in stage["enemies"]
            .as_array_mut()
            .into_iter()
            .flatten()
            .zip(&foes)
        {
            e["level"] = Value::from(*level);
        }
    }
    result.insert(
        "stages".into(),
        levels(&stages, |s| &s["enemies"], |e| &e["level"]),
    );

    // Конвои: враги — [id, уровень].
    let mut convoys: Value = read(&dir, "convoys.json")?;
    for tier in convoys["tiers"].as_array_mut().into_iter().flatten() {
        let name = format!("конвой {}", tier["name"].as_str().unwrap_or("?"));
        retune_pairs(&tuner, &name, &mut tier["enemies"]);
    }
    result.insert(
        "convoys".into(),
        levels(&convoys["tiers"], |t| &t["enemies"], |e| &e[1]),
    );

    // Ивенты: этапы Регаты.
    let mut events: Value = read(&dir, "events.json")?;
    for (i, leg) in events["regatta"]["legs"]
        .as_array_mut()
        .into_iter()
        .flatten()
        .enumerate()
    {
        retune_pairs(
            &tuner,
            &format!("регата, этап {}", i + 1),
            &mut leg["enemies"],
        );
    }
    result.insert(
        "regatta".into(),
        levels(&events["regatta"]["legs"], |l| &l["enemies"], |e| &e[1]),
    );
    println!("RESULT {}", Value::Object(result));
    Ok(())
}

struct Tuner<'a> {
    data: &'a GameData,
    runs: u64,
    old_scale: f32,
    new_scale: f32,
}

impl Tuner<'_> {
    fn win_rate(
        &self,
        level: u32,
        foes: &[(String, u32)],
        wind: Option<Element>,
        scale: f32,
    ) -> f64 {
        let team: Vec<UnitDef> = TEAM
            .iter()
            .map(|id| self.data.hero(id).expect("герой").to_unit(level))
            .collect();
        let foes: Vec<UnitDef> = foes
            .iter()
            .map(|(id, l)| {
                self.data
                    .enemy(id)
                    .unwrap_or_else(|| panic!("нет врага {id}"))
                    .to_unit((*l).max(1))
            })
            .collect();
        let wins = (0..self.runs)
            .filter(|&seed| {
                let mut b = Battle::new(
                    team.clone(),
                    foes.clone(),
                    Ship {
                        broadside_power: 120,
                    },
                    seed,
                );
                b.set_wind(wind);
                b.set_damage_scale(scale);
                let mut n = 0;
                while b.outcome().is_none() && n < 3000 {
                    b.auto_step();
                    n += 1;
                }
                b.outcome() == Some(Outcome::Victory)
            })
            .count();
        wins as f64 / self.runs as f64
    }

    /// Уровень отряда, с которого бой берётся в 80% автобоёв.
    fn threshold(&self, foes: &[(String, u32)], wind: Option<Element>, scale: f32) -> u32 {
        (1..=MAX_LEVEL)
            .find(|&l| self.win_rate(l, foes, wind, scale) >= 0.8)
            .unwrap_or(MAX_LEVEL)
    }

    /// Сдвинуть уровни врагов: при новом темпе порог — как был при старом.
    fn retune(&self, name: &str, foes: &mut [(String, u32)], wind: Option<Element>) {
        let target = self.threshold(foes, wind, self.old_scale);
        let shifted = |d: i32| -> Vec<(String, u32)> {
            foes.iter()
                .map(|(id, l)| (id.clone(), (*l as i32 + d).max(1) as u32))
                .collect()
        };
        // Самый сильный сдвиг, при котором отряд уровня `target` всё ещё берёт 80%.
        let (mut lo, mut hi) = (-60i32, 30i32);
        while lo < hi {
            let mid = (lo + hi + 1).div_euclid(2);
            if self.win_rate(target, &shifted(mid), wind, self.new_scale) >= 0.8 {
                lo = mid;
            } else {
                hi = mid - 1;
            }
        }
        // Бой, который брали уже на 1-м уровне, не делаем тяжелее: ниже порога 1 не опустить,
        // и поиск «дотянул» бы его до грани 80%.
        if target == 1 {
            lo = lo.min(0);
        }
        let new = shifted(lo);
        let after = self.threshold(&new, wind, self.new_scale);
        println!(
            "{name:<28} порог {target:>2} · сдвиг врагов {lo:+} · новый порог {after:>2} · {:?} → {:?}",
            foes.iter().map(|(_, l)| *l).max().unwrap_or(0),
            new.iter().map(|(_, l)| *l).max().unwrap_or(0)
        );
        foes.clone_from_slice(&new);
    }
}

fn retune_pairs(tuner: &Tuner, name: &str, list: &mut Value) {
    let mut foes: Vec<(String, u32)> = list
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|p| Some((p[0].as_str()?.to_string(), p[1].as_u64()? as u32)))
        .collect();
    tuner.retune(name, &mut foes, None);
    *list = Value::Array(
        foes.into_iter()
            .map(|(id, l)| Value::Array(vec![Value::String(id), Value::from(l)]))
            .collect(),
    );
}

fn read(dir: &Path, name: &str) -> Result<Value, Box<dyn std::error::Error>> {
    Ok(serde_json::from_str(&std::fs::read_to_string(
        dir.join(name),
    )?)?)
}

/// Уровни врагов по боям списка: `[[6, 6, 6], [10, 10, 10, 10], …]`.
fn levels(
    list: &Value,
    enemies: impl Fn(&Value) -> &Value,
    level: impl Fn(&Value) -> &Value,
) -> Value {
    Value::Array(
        list.as_array()
            .into_iter()
            .flatten()
            .map(|fight| {
                Value::Array(
                    enemies(fight)
                        .as_array()
                        .into_iter()
                        .flatten()
                        .map(|e| level(e).clone())
                        .collect(),
                )
            })
            .collect(),
    )
}
