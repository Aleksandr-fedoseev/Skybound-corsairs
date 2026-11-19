//! «Соседи по каюте»: два героя в одной каюте получают парный бонус к атаке, если оба в
//! отряде. Бонус растёт с близостью: совместные победы и разговор раз в день.
//!
//! Кают столько, сколько даёт отсек «Каюты». Числа — `data/cabins.json`.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::battle_screen::BattleScreen;
use crate::save::now;
use battle_core::Outcome;

const SECONDS_PER_DAY: u64 = 86_400;

#[derive(Resource, Deserialize)]
pub struct CabinData {
    pub bonus_first: f32,
    pub bonus_per_level: f32,
    pub max_level: u32,
    pub points_per_level: u32,
    pub points_per_win: u32,
    pub points_per_talk: u32,
}

impl CabinData {
    fn bundled() -> Self {
        serde_json::from_str(include_str!("../../../data/cabins.json"))
            .expect("data/cabins.json корректен")
    }

    pub fn level(&self, points: u32) -> u32 {
        (1 + points / self.points_per_level).min(self.max_level)
    }

    pub fn bonus(&self, points: u32) -> f32 {
        self.bonus_first + self.bonus_per_level * (self.level(points) - 1) as f32
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Pair {
    pub a: String,
    pub b: String,
    pub points: u32,
    /// День последнего разговора (по UTC).
    #[serde(default)]
    pub talked_on: u64,
}

impl Pair {
    pub fn has(&self, hero: &str) -> bool {
        self.a == hero || self.b == hero
    }

    pub fn partner(&self, hero: &str) -> &str {
        if self.a == hero { &self.b } else { &self.a }
    }
}

/// Кто с кем живёт (сохраняется).
#[derive(Resource, Clone, Default, Serialize, Deserialize)]
pub struct Cabins {
    pub pairs: Vec<Pair>,
}

pub fn today() -> u64 {
    now() / SECONDS_PER_DAY
}

impl Cabins {
    pub fn pair_of(&self, hero: &str) -> Option<&Pair> {
        self.pairs.iter().find(|p| p.has(hero))
    }

    /// Поселить двоих в свободную каюту.
    pub fn settle(&mut self, a: &str, b: &str, cabins: u32) -> Result<(), String> {
        if a == b {
            return Err("Нужны два разных героя".into());
        }
        if self.pair_of(a).is_some() || self.pair_of(b).is_some() {
            return Err("Кто-то из них уже живёт в каюте".into());
        }
        if self.pairs.len() as u32 >= cabins {
            return Err("Свободных кают нет — больше кают даёт отсек «Каюты»".into());
        }
        self.pairs.push(Pair {
            a: a.to_string(),
            b: b.to_string(),
            points: 0,
            talked_on: 0,
        });
        Ok(())
    }

    pub fn evict(&mut self, hero: &str) {
        self.pairs.retain(|p| !p.has(hero));
    }

    /// Разговор раз в день: близость растёт. `false`, если сегодня уже говорили.
    pub fn talk(&mut self, hero: &str, data: &CabinData, day: u64) -> bool {
        let Some(pair) = self.pairs.iter_mut().find(|p| p.has(hero)) else {
            return false;
        };
        if pair.talked_on == day {
            return false;
        }
        pair.talked_on = day;
        pair.points += data.points_per_talk;
        true
    }

    /// Прибавка к атаке героя, если его сосед тоже в отряде.
    pub fn atk_bonus(&self, hero: &str, team: &[String], data: &CabinData) -> f32 {
        match self.pair_of(hero) {
            Some(p) if team.iter().any(|t| t == p.partner(hero)) => data.bonus(p.points),
            _ => 0.0,
        }
    }
}

pub fn plugin(app: &mut App) {
    app.insert_resource(CabinData::bundled())
        .init_resource::<Cabins>()
        .add_systems(Update, closeness_from_wins);
}

/// Победа, где оба соседа в отряде, сближает их.
fn closeness_from_wins(
    screen: Res<BattleScreen>,
    data: Res<CabinData>,
    mut cabins: ResMut<Cabins>,
    mut counted: Local<Option<(usize, u64)>>,
) {
    if screen.shown_outcome != Some(Outcome::Victory) {
        return;
    }
    let battle = (screen.stage, screen.seed);
    if *counted == Some(battle) {
        return;
    }
    *counted = Some(battle);
    let team: Vec<&str> = screen.team.iter().map(|m| m.id.as_str()).collect();
    for pair in &mut cabins.pairs {
        if team.contains(&pair.a.as_str()) && team.contains(&pair.b.as_str()) {
            pair.points += data.points_per_win;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bonus_only_with_partner_in_team() {
        let data = CabinData::bundled();
        let mut c = Cabins::default();
        c.settle("ren", "borg", 1).unwrap();
        let both = vec!["ren".to_string(), "borg".to_string()];
        let alone = vec!["ren".to_string()];
        assert!(c.atk_bonus("ren", &both, &data) > 0.0);
        assert_eq!(c.atk_bonus("ren", &alone, &data), 0.0);
    }

    #[test]
    fn cabins_limit_and_daily_talk() {
        let data = CabinData::bundled();
        let mut c = Cabins::default();
        c.settle("ren", "borg", 1).unwrap();
        assert!(c.settle("kaya", "mei", 1).is_err());
        assert!(c.talk("ren", &data, 7));
        assert!(!c.talk("borg", &data, 7));
        assert!(c.talk("borg", &data, 8));
        let before = data.bonus(0);
        let pair = c.pair_of("ren").unwrap();
        assert!(data.bonus(pair.points) >= before);
    }
}
