//! Звук: музыка по экранам и звуки событий боя и интерфейса.
//!
//! Звуки синтезированы `tools/make_sounds.py`, громкости — в `data/audio.json`.
//! Любая часть игры просит звук сообщением `Sfx("имя")`.

use std::collections::HashMap;

use battle_core::{AbilityKind, Event, Outcome};
use bevy::audio::{AudioSinkPlayback, Volume};
use bevy::prelude::*;
use serde::Deserialize;

use crate::Screen;
use crate::battle_screen::{BattleScreen, Shown};

const SOUNDS: [&str; 18] = [
    "hit_1",
    "hit_2",
    "hit_3",
    "crit",
    "skill",
    "ultimate",
    "broadside",
    "heal",
    "death",
    "victory",
    "defeat",
    "click",
    "coin",
    "hammer",
    "done",
    "levelup",
    "flip",
    "ssr",
];
const HARBOR: &str = "music_harbor";
const BATTLE: &str = "music_battle";

/// Громкости: `data/audio.json`.
#[derive(Resource, Deserialize)]
pub struct AudioData {
    pub music_volume: f32,
    /// Доля громкости музыки, когда бой закончился (звучит фанфара).
    pub music_after_battle: f32,
    pub sfx_volume: f32,
    /// Не чаще одного удара за столько секунд (по площади бьёт много целей сразу).
    pub hit_gap: f32,
    pub heal_gap: f32,
    /// Громкость каждого звука; у ударов — общая `hit`.
    pub volumes: HashMap<String, f32>,
}

impl AudioData {
    fn bundled() -> Self {
        serde_json::from_str(include_str!("../../../data/audio.json"))
            .expect("data/audio.json корректен")
    }

    fn volume(&self, name: &str) -> f32 {
        let key = if name.starts_with("hit_") {
            "hit"
        } else {
            name
        };
        self.volumes.get(key).copied().unwrap_or(1.0) * self.sfx_volume
    }
}

/// Просьба сыграть звук.
#[derive(Message, Clone, Copy)]
pub struct Sfx(pub &'static str);

#[derive(Resource)]
struct Sounds(HashMap<&'static str, Handle<AudioSource>>);

impl FromWorld for Sounds {
    fn from_world(world: &mut World) -> Self {
        let assets = world.resource::<AssetServer>();
        Self(
            SOUNDS
                .iter()
                .chain([HARBOR, BATTLE].iter())
                .map(|name| (*name, assets.load(format!("audio/{name}.ogg"))))
                .collect(),
        )
    }
}

/// Играющая музыка и её трек.
#[derive(Component)]
struct Music(&'static str);

pub fn plugin(app: &mut App) {
    app.insert_resource(AudioData::bundled())
        .init_resource::<Sounds>()
        .add_message::<Sfx>()
        .add_systems(Update, (battle_sounds, play_sfx, music).chain());
}

fn play_sfx(
    mut commands: Commands,
    mut requests: MessageReader<Sfx>,
    sounds: Res<Sounds>,
    data: Res<AudioData>,
) {
    // Один и тот же звук в одном кадре — один раз.
    let mut played: Vec<&str> = Vec::new();
    for Sfx(name) in requests.read() {
        if played.contains(name) {
            continue;
        }
        played.push(name);
        if let Some(handle) = sounds.0.get(name) {
            commands.spawn((
                AudioPlayer(handle.clone()),
                PlaybackSettings::DESPAWN.with_volume(Volume::Linear(data.volume(name))),
            ));
        }
    }
}

/// Звуки боя по событиям, которые экран только что показал.
fn battle_sounds(
    time: Res<Time>,
    data: Res<AudioData>,
    mut shown: MessageReader<Shown>,
    mut sfx: MessageWriter<Sfx>,
    mut last_hit: Local<f32>,
    mut last_heal: Local<f32>,
    mut next_hit: Local<usize>,
) {
    let now = time.elapsed_secs();
    for Shown { event, .. } in shown.read() {
        match event {
            Event::AbilityUsed { kind, .. } => match kind {
                AbilityKind::Skill => {
                    sfx.write(Sfx("skill"));
                }
                AbilityKind::Ultimate => {
                    sfx.write(Sfx("ultimate"));
                }
                AbilityKind::Basic => {}
            },
            Event::BroadsideFired => {
                sfx.write(Sfx("broadside"));
            }
            Event::WindChanged { .. } => {
                sfx.write(Sfx("skill"));
            }
            Event::Damage { weakness, crit, .. } => {
                if now - *last_hit < data.hit_gap {
                    continue;
                }
                *last_hit = now;
                if *weakness || *crit {
                    sfx.write(Sfx("crit"));
                } else {
                    *next_hit = (*next_hit + 1) % 3;
                    sfx.write(Sfx(["hit_1", "hit_2", "hit_3"][*next_hit]));
                }
            }
            Event::Healed { .. } => {
                if now - *last_heal >= data.heal_gap {
                    *last_heal = now;
                    sfx.write(Sfx("heal"));
                }
            }
            Event::Defeated { .. } => {
                sfx.write(Sfx("death"));
            }
            Event::BattleEnded { outcome } => {
                sfx.write(Sfx(match outcome {
                    Outcome::Victory => "victory",
                    Outcome::Defeat => "defeat",
                }));
            }
            Event::TurnStarted { .. } | Event::Advanced { .. } | Event::Delayed { .. } => {}
        }
    }
}

/// Своя музыка у боя и у корабля; после боя музыка затихает под фанфару.
fn music(
    mut commands: Commands,
    state: Res<State<Screen>>,
    screen: Res<BattleScreen>,
    sounds: Res<Sounds>,
    data: Res<AudioData>,
    mut playing: Query<(Entity, &Music, Option<&mut AudioSink>)>,
) {
    let battle = *state.get() == Screen::Battle;
    let track = if battle { BATTLE } else { HARBOR };
    let quiet = battle && screen.shown_outcome.is_some();
    let volume = data.music_volume * if quiet { data.music_after_battle } else { 1.0 };
    let mut found = false;
    for (entity, Music(current), sink) in &mut playing {
        if *current != track {
            commands.entity(entity).despawn();
            continue;
        }
        found = true;
        if let Some(mut sink) = sink {
            sink.set_volume(Volume::Linear(volume));
        }
    }
    if !found && let Some(handle) = sounds.0.get(track) {
        commands.spawn((
            Music(track),
            AudioPlayer(handle.clone()),
            PlaybackSettings::LOOP.with_volume(Volume::Linear(volume)),
        ));
    }
}
