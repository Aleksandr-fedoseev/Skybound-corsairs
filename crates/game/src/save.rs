//! Сохранение прогресса в файл и доход корабля, пока игрока не было.
//!
//! Файл: `$XDG_DATA_HOME/skybound-corsairs/save.json` (или `~/.local/share/...`), путь можно
//! задать `SKYBOUND_SAVE`. В режиме скриншота сохранения нет — кадры всегда с чистого старта.
//! Это временная локальная копия: в релизе прогресс и экономику хранит сервер.

use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::abyss::Abyss;
use crate::cabins::Cabins;
use crate::contracts::Contracts;
use crate::convoys::{ConvoyData, Convoys};
use crate::crew::Roster;
use crate::events::StormEvent;
use crate::expeditions::Expeditions;
use crate::gear::Gear;
use crate::menu::{Profile, Progress, Toast};
use crate::ship::{Ship, ShipData};
use crate::tavern::TavernState;

const VERSION: u32 = 1;
const AUTOSAVE_SECONDS: f32 = 5.0;
/// Дольше этого корабль сам не копит — чтобы неделя отсутствия не ломала экономику.
const OFFLINE_CAP_SECONDS: u64 = 12 * 3600;

#[derive(Serialize, Deserialize)]
pub struct SaveData {
    pub version: u32,
    pub saved_at: u64,
    pub profile: Profile,
    pub progress: Progress,
    pub roster: Roster,
    pub ship: Ship,
    pub tavern: TavernState,
    #[serde(default)]
    pub convoys: Convoys,
    #[serde(default)]
    pub contracts: Contracts,
    #[serde(default)]
    pub abyss: Abyss,
    #[serde(default)]
    pub expeditions: Expeditions,
    #[serde(default)]
    pub cabins: Cabins,
    #[serde(default)]
    pub gear: Gear,
    #[serde(default)]
    pub storm: StormEvent,
}

pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

fn path() -> Option<PathBuf> {
    if std::env::var_os("SKYBOUND_SCREENSHOT").is_some() {
        return None;
    }
    if let Some(p) = std::env::var_os("SKYBOUND_SAVE") {
        return Some(PathBuf::from(p));
    }
    let base = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))?;
    Some(base.join("skybound-corsairs/save.json"))
}

/// Прочитать сохранение; битый или старый файл — как будто его нет.
pub fn load() -> Option<SaveData> {
    let text = std::fs::read_to_string(path()?).ok()?;
    match serde_json::from_str::<SaveData>(&text) {
        Ok(save) if save.version == VERSION => Some(save),
        Ok(_) => None,
        Err(e) => {
            warn!("сохранение не прочитано: {e}");
            None
        }
    }
}

fn write(save: &SaveData) {
    let Some(path) = path() else {
        return;
    };
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let Ok(text) = serde_json::to_string_pretty(save) else {
        return;
    };
    // Сначала во временный файл: оборванная запись не испортит сохранение.
    let tmp = path.with_extension("json.tmp");
    if std::fs::write(&tmp, text).is_ok() {
        let _ = std::fs::rename(&tmp, &path);
    }
}

/// Что корабль накопил за `seconds` без игрока. Возвращает строку отчёта.
pub fn offline(save: &mut SaveData, data: &ShipData, convoy: &ConvoyData, seconds: u64) -> String {
    let seconds = seconds.min(OFFLINE_CAP_SECONDS) as f32;
    let minutes = seconds / 60.0;
    let mut parts = Vec::new();

    let mut built = None;
    if let Some(build) = &mut save.ship.build {
        build.left -= seconds;
        if build.left <= 0.0 {
            built = save.ship.finish_build();
        }
    }
    if let Some((id, level)) = built {
        let name = data.compartment(&id).map_or("", |c| c.name.as_str());
        parts.push(format!("достроено: {name} ур. {level}"));
    }

    let cap = save.ship.capacity(data).ether as f32;
    let before = save.profile.ether_pending;
    save.profile.ether_pending =
        (before + save.ship.ether_per_minute(data) as f32 * minutes).min(cap);
    let ether = (save.profile.ether_pending - before).floor() as u32;
    if ether > 0 {
        parts.push(format!("+{ether} эфира у Ветроловов"));
    }

    let journals = (save.ship.journals_per_minute(data) as f32 * minutes).floor() as u32;
    if journals > 0 {
        save.profile.journals += journals;
        parts.push(format!("+{journals} журналов"));
    }

    let arrived = save.convoys.pass(convoy, seconds);
    if arrived > 0 {
        parts.push(format!("конвоев на карте: +{arrived}"));
    }

    if parts.is_empty() {
        String::new()
    } else {
        format!("Пока вас не было: {}", parts.join(", "))
    }
}

/// Поставить загруженное сохранение в игру (после плагинов — заменяет стартовые ресурсы).
pub fn apply(app: &mut App, mut save: SaveData) {
    let data = ShipData::bundled();
    let away = now().saturating_sub(save.saved_at);
    let report = offline(&mut save, &data, &ConvoyData::bundled(), away);
    app.insert_resource(save.profile)
        .insert_resource(save.progress)
        .insert_resource(save.roster)
        .insert_resource(save.ship)
        .insert_resource(save.tavern)
        .insert_resource(save.convoys)
        .insert_resource(save.contracts)
        .insert_resource(save.abyss)
        .insert_resource(save.expeditions)
        .insert_resource(save.cabins)
        .insert_resource(save.gear)
        .insert_resource(save.storm);
    if !report.is_empty() {
        app.world_mut().resource_mut::<Toast>().show_long(&report);
    }
}

pub fn plugin(app: &mut App) {
    app.add_systems(Update, autosave)
        .add_systems(Last, save_on_exit);
}

#[derive(bevy::ecs::system::SystemParam)]
struct State<'w> {
    profile: Res<'w, Profile>,
    progress: Res<'w, Progress>,
    roster: Res<'w, Roster>,
    ship: Res<'w, Ship>,
    tavern: Res<'w, TavernState>,
    convoys: Res<'w, Convoys>,
    contracts: Res<'w, Contracts>,
    abyss: Res<'w, Abyss>,
    expeditions: Res<'w, Expeditions>,
    cabins: Res<'w, Cabins>,
    gear: Res<'w, Gear>,
    storm: Res<'w, StormEvent>,
}

impl State<'_> {
    fn snapshot(&self) -> SaveData {
        SaveData {
            version: VERSION,
            saved_at: now(),
            profile: self.profile.clone(),
            progress: self.progress.clone(),
            roster: self.roster.clone(),
            ship: self.ship.clone(),
            tavern: self.tavern.clone(),
            convoys: self.convoys.clone(),
            contracts: self.contracts.clone(),
            abyss: self.abyss.clone(),
            expeditions: self.expeditions.clone(),
            cabins: self.cabins.clone(),
            gear: self.gear.clone(),
            storm: self.storm.clone(),
        }
    }
}

fn autosave(time: Res<Time>, mut timer: Local<f32>, state: State) {
    *timer += time.delta_secs();
    if *timer >= AUTOSAVE_SECONDS {
        *timer = 0.0;
        write(&state.snapshot());
    }
}

fn save_on_exit(mut exits: MessageReader<AppExit>, state: State) {
    if exits.read().next().is_some() {
        write(&state.snapshot());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crew::CrewData;
    use crate::menu::MenuData;
    use crate::ship::Build;

    fn fresh() -> SaveData {
        let menu = MenuData::bundled();
        let ship = ShipData::bundled();
        SaveData {
            version: VERSION,
            saved_at: 1000,
            profile: Profile::new(&menu.start),
            progress: Progress {
                cleared: 2,
                seen: Vec::new(),
            },
            roster: Roster::new(&CrewData::bundled()),
            ship: Ship::new(&ship),
            tavern: TavernState::default(),
            convoys: Convoys::default(),
            contracts: Contracts::default(),
            abyss: Abyss::default(),
            expeditions: Expeditions::default(),
            cabins: Cabins::default(),
            gear: Gear::default(),
            storm: StormEvent::default(),
        }
    }

    #[test]
    fn round_trip_keeps_progress() {
        let mut save = fresh();
        save.profile.silver = 777;
        save.roster.recruit("sora");
        let text = serde_json::to_string(&save).unwrap();
        let back: SaveData = serde_json::from_str(&text).unwrap();
        assert_eq!(back.profile.silver, 777);
        assert_eq!(back.progress.cleared, 2);
        assert!(back.roster.owns("sora"));
    }

    #[test]
    fn offline_finishes_build_and_fills_ether() {
        let data = ShipData::bundled();
        let mut save = fresh();
        save.profile.ether_pending = 0.0;
        save.ship.build = Some(Build {
            id: "hull".into(),
            to: 2,
            left: 60.0,
        });
        let report = offline(&mut save, &data, &ConvoyData::bundled(), 600);
        assert!(save.ship.build.is_none());
        assert_eq!(save.ship.hull(), 2);
        assert!(save.profile.ether_pending > 0.0);
        assert!(report.contains("Корпус"));
    }

    #[test]
    fn offline_is_capped() {
        let data = ShipData::bundled();
        let mut a = fresh();
        let mut b = fresh();
        offline(&mut a, &data, &ConvoyData::bundled(), OFFLINE_CAP_SECONDS);
        offline(
            &mut b,
            &data,
            &ConvoyData::bundled(),
            OFFLINE_CAP_SECONDS * 10,
        );
        assert_eq!(a.profile.ether_pending, b.profile.ether_pending);
    }
}
