use battle_core::{Battle, Outcome, Ship};
use game_data::GameData;

const STARTER_TEAM: [&str; 4] = ["borg", "ren", "kaya", "mei"];

fn win_rate(data: &GameData, stage: &str, level: u32, runs: u64) -> f64 {
    let team: Vec<_> = STARTER_TEAM
        .iter()
        .map(|id| data.hero(id).expect("герой есть в данных").to_unit(level))
        .collect();
    let enemies = data.stage_enemies(stage).unwrap();
    let wins = (0..runs)
        .filter(|seed| {
            let mut battle = Battle::new(
                team.clone(),
                enemies.clone(),
                Ship {
                    broadside_power: 120,
                },
                *seed,
            );
            battle.run_auto(500) == Some(Outcome::Victory)
        })
        .count();
    wins as f64 / runs as f64
}

#[test]
fn bundled_data_is_valid() {
    let data = GameData::bundled().expect("данные из data/ корректны");
    assert_eq!(data.heroes.len(), 7);
    for stage in &data.stages {
        assert!(!data.stage_enemies(&stage.id).unwrap().is_empty());
    }
}

#[test]
fn hero_stats_grow_with_level() {
    let data = GameData::bundled().unwrap();
    let ren = data.hero("ren").unwrap();
    let l1 = ren.to_unit(1).stats;
    let l10 = ren.to_unit(10).stats;
    assert_eq!(l10.max_hp, l1.max_hp + ren.growth.max_hp * 9);
    assert!(l10.atk > l1.atk);
}

#[test]
fn unknown_enemy_in_stage_is_rejected() {
    let stages = r#"[{ "id": "x", "name": "x", "enemies": [{ "enemy": "nobody", "level": 1 }] }]"#;
    let err = GameData::from_json("[]", "[]", stages).unwrap_err();
    assert!(err.to_string().contains("nobody"));
}

/// Первый этап должен проходиться стартовой командой почти всегда.
#[test]
fn first_stage_is_beatable_by_starter_team() {
    let data = GameData::bundled().unwrap();
    let rate = win_rate(&data, "1-1", 1, 200);
    assert!(rate >= 0.95, "процент побед на 1-1: {rate}");
}
