//! Абордаж конвоев: торговые суда Империи появляются на карте по таймеру.
//!
//! Раз в `every_minutes` в очереди прибавляется судно (до `max_ready`), в том числе пока
//! игрока нет. Чем дальше игрок прошёл главу, тем богаче и опаснее конвой. Победа даёт
//! доски и серебро — главный источник досок для стройки.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::battle_screen::{CustomFight, FightKind};
use crate::menu::Progress;

#[derive(Resource, Deserialize)]
pub struct ConvoyData {
    pub every_minutes: f32,
    pub max_ready: u32,
    pub start_ready: u32,
    pub spot: Spot,
    pub tiers: Vec<Tier>,
}

#[derive(Deserialize)]
pub struct Spot {
    pub name: String,
    pub x: f32,
    pub y: f32,
}

#[derive(Deserialize)]
pub struct Tier {
    /// С какого числа пройденных этапов главы появляется такой конвой.
    pub from_cleared: usize,
    pub name: String,
    pub enemies: Vec<(String, u32)>,
    pub silver: u32,
    pub planks: u32,
}

impl ConvoyData {
    pub fn bundled() -> Self {
        serde_json::from_str(include_str!("../../../data/convoys.json"))
            .expect("data/convoys.json корректен")
    }

    pub fn period(&self) -> f32 {
        self.every_minutes * 60.0
    }

    pub fn tier(&self, progress: &Progress) -> &Tier {
        self.tiers
            .iter()
            .rfind(|t| t.from_cleared <= progress.cleared)
            .or(self.tiers.first())
            .expect("в data/convoys.json есть конвои")
    }

    /// Бой с текущим конвоем.
    pub fn fight(&self, progress: &Progress) -> CustomFight {
        let tier = self.tier(progress);
        CustomFight {
            title: format!("Абордаж · {}", tier.name),
            kind: FightKind::Convoy,
            enemies: tier.enemies.clone(),
            silver: tier.silver,
            planks: tier.planks,
            journals: 0,
            energy_rate: 1,
        }
    }
}

/// Очередь судов и время до следующего.
#[derive(Resource, Clone, Serialize, Deserialize)]
pub struct Convoys {
    pub ready: u32,
    /// Сколько секунд прошло с появления последнего судна.
    pub timer: f32,
}

impl Convoys {
    pub fn new(data: &ConvoyData) -> Self {
        Self {
            ready: data.start_ready,
            timer: 0.0,
        }
    }

    /// Прошло `seconds`: новые суда в очередь. Возвращает, сколько прибавилось.
    pub fn pass(&mut self, data: &ConvoyData, seconds: f32) -> u32 {
        if self.ready >= data.max_ready {
            self.timer = 0.0;
            return 0;
        }
        self.timer += seconds;
        let mut added = 0;
        while self.timer >= data.period() && self.ready < data.max_ready {
            self.timer -= data.period();
            self.ready += 1;
            added += 1;
        }
        if self.ready >= data.max_ready {
            self.timer = 0.0;
        }
        added
    }

    pub fn next_in(&self, data: &ConvoyData) -> f32 {
        (data.period() - self.timer).max(0.0)
    }
}

impl Default for Convoys {
    fn default() -> Self {
        Self::new(&ConvoyData::bundled())
    }
}

pub fn plugin(app: &mut App) {
    let data = ConvoyData::bundled();
    app.insert_resource(Convoys::new(&data))
        .insert_resource(data)
        .add_systems(Update, tick);
}

fn tick(time: Res<Time>, data: Res<ConvoyData>, mut convoys: ResMut<Convoys>) {
    convoys.pass(&data, time.delta_secs());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn convoys_arrive_and_cap() {
        let data = ConvoyData::bundled();
        let mut c = Convoys {
            ready: 0,
            timer: 0.0,
        };
        assert_eq!(c.pass(&data, data.period() * 2.5), 2);
        assert_eq!(c.ready, 2);
        c.pass(&data, data.period() * 100.0);
        assert_eq!(c.ready, data.max_ready);
    }

    #[test]
    fn tier_grows_with_progress() {
        let data = ConvoyData::bundled();
        let early = data.tier(&Progress::default()).planks;
        let late = data.tier(&Progress {
            cleared: 4,
            seen: Vec::new(),
        });
        assert!(late.planks > early);
    }
}
