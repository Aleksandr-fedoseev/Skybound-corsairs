//! Эффекты умений: у умения свой рисунок вместо общего снаряда.
//!
//! Порыв — серп ветра летит в одну цель. Вихрь — смерч идёт от бойца по вражеской линии и
//! задевает цели по очереди (урон каждой — когда смерч до неё дошёл). Буря — над врагами
//! сгущается туча, льёт дождь, в каждого бьёт молния. Клинки (Рэн): укол, круговой удар (кольцо
//! взмаха крутится вокруг бойца и бежит с ним к цели, потом разрез по цели),
//! гроза — молния с неба в поднятые сабли, рывок, молния и крест-разрез по цели. Какое умение каким
//! эффектом показано и его темп — в `data/spells.json`.

use std::collections::HashMap;
use std::f32::consts::TAU;

use battle_core::{AbilityKind, Event, UnitId};
use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use serde::Deserialize;

use crate::battle_screen::{BattleScreen, Shown};
use crate::battlefield::{Rig, UnitModel, Z_FLOATING};

const Z_TORNADO: f32 = Z_FLOATING - 6.0;
const Z_CLOUD: f32 = Z_FLOATING - 5.0;
const Z_RAIN: f32 = Z_FLOATING - 4.5;
const Z_BOLT: f32 = Z_FLOATING - 4.0;
const LEAVES: usize = 9;
const LEAF: Color = Color::srgb(0.55, 0.9, 0.45);
const RAIN: Color = Color::srgba(0.78, 0.86, 1.0, 0.55);
const BOLT_GLOW: Color = Color::srgba(0.55, 0.75, 1.0, 0.5);
const BOLT_CORE: Color = Color::srgb(0.95, 0.97, 1.0);

#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum SpellFx {
    Gust,
    Whirlwind,
    Storm,
    Thrust,
    WhirlSlash,
    ThunderStrike,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Spell {
    pub fx: SpellFx,
    /// Замах до первого попадания (с); нет — как у прочих умений этого вида.
    #[serde(default)]
    pub windup: Option<f32>,
    /// Пауза между попаданиями по целям (с); нет — общая.
    #[serde(default)]
    pub hit_gap: Option<f32>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Tuning {
    /// Длина серпа порыва — в ростах бойца.
    pub gust_length: f32,
    /// Высота смерча — в ростах бойца; оборотов в секунду; сколько уходит за последнюю цель (с).
    pub tornado_height: f32,
    pub tornado_spin_hz: f32,
    pub tornado_exit: f32,
    /// Туча бури: ширина и высота центра на поле, сколько висит после последней молнии (с).
    pub storm_cloud_width: f32,
    pub storm_cloud_y: f32,
    pub storm_linger: f32,
    pub rain_drops: usize,
    pub bolt_time: f32,
    /// Сколько живёт разрез клинка (с); откуда бьёт молния с неба (высота на поле).
    pub slash_time: f32,
    pub sky_y: f32,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Spells {
    pub tuning: Tuning,
    /// Боец → вид умения (basic, skill, ultimate) → эффект.
    pub units: HashMap<String, HashMap<String, Spell>>,
}

impl Spells {
    pub fn bundled() -> Self {
        serde_json::from_str(include_str!("../../../data/spells.json"))
            .expect("data/spells.json корректен")
    }

    pub fn get(&self, unit: &str, kind: AbilityKind) -> Option<&Spell> {
        let key = match kind {
            AbilityKind::Basic => "basic",
            AbilityKind::Skill => "skill",
            AbilityKind::Ultimate => "ultimate",
        };
        self.units.get(unit)?.get(key)
    }
}

/// Корень любого эффекта умения: убирается при выходе из боя.
#[derive(Component)]
pub struct SpellRoot;

/// Серп порыва: летит от руки к цели, у цели расходится и гаснет.
#[derive(Component)]
pub struct GustFx {
    timer: Timer,
    from: Vec2,
    to: Vec2,
    size: Vec2,
}

/// Расходящийся след порыва у цели.
#[derive(Component)]
pub struct Puff {
    timer: Timer,
}

/// Смерч: путь — точки со временем прихода (с от начала).
#[derive(Component)]
pub struct Tornado {
    age: f32,
    /// Размер рисунка смерча.
    size: Vec2,
    path: Vec<(Vec2, f32)>,
    spin_hz: f32,
}

/// Половина смерча (рисунок или зеркальный): попеременно проявляются — смерч крутится.
#[derive(Component)]
pub struct TornadoLayer {
    mirrored: bool,
}

#[derive(Component)]
pub struct Leaf {
    /// Высота на смерче (доля), начальный угол, скорость (оборотов за оборот смерча).
    level: f32,
    phase: f32,
    speed: f32,
}

/// Буря: туча над целями, дождь и молнии в каждую цель в своё время.
#[derive(Component)]
pub struct Storm {
    age: f32,
    windup: f32,
    end: f32,
    strikes: Vec<(Vec2, f32, bool)>,
    cloud_bottom: f32,
    /// Середина и ширина тучи; когда сверкнуть в ней следующей молнией (без цели).
    center_x: f32,
    width: f32,
    next_flicker: f32,
    seed: u32,
}

#[derive(Component)]
pub struct StormCloud;

#[derive(Component)]
pub struct Raindrop {
    x: f32,
    phase: f32,
    top: f32,
    bottom: f32,
}

#[derive(Component)]
pub struct Bolt {
    timer: Timer,
    glow: bool,
}

/// Вспышка: свечение в месте удара молнии или белый проблеск на весь экран.
#[derive(Component)]
pub struct Spark {
    timer: Timer,
    /// Насколько вырастает за жизнь и начальная непрозрачность.
    grow: f32,
    alpha: f32,
}

/// Удар клинком: кусочки эффекта по расписанию (с от начала умения).
#[derive(Component)]
pub struct Blades {
    age: f32,
    steps: Vec<(f32, Piece)>,
    seed: u32,
}

#[derive(Clone, Copy)]
enum Piece {
    /// Разрез (серп) или укол (веретено) — рисунок, угол, размер.
    Slash {
        at: Vec2,
        angle: f32,
        size: Vec2,
        thrust: bool,
        flip: bool,
    },
    /// Молния с неба в точку; толщина — множитель.
    SkyBolt { to: Vec2, width: f32 },
    /// Свечение в точке.
    Glow { at: Vec2, size: f32 },
    /// Короткие разряды во все стороны.
    Arcs { at: Vec2, radius: f32 },
    /// Белый проблеск на весь экран и тряска.
    Flash,
    /// Кольцо кругового взмаха вокруг бойца: держится за его риг и бежит вместе с ним.
    Whirl {
        rig: Entity,
        height: f32,
        seconds: f32,
    },
}

/// Круговой взмах: эллипс вокруг пояса, внутри крутится кольцо-комета.
#[derive(Component)]
pub struct WhirlFx {
    timer: Timer,
    turns: f32,
}

/// Разрез: быстро прочерчивается и гаснет.
#[derive(Component)]
pub struct SlashFx {
    timer: Timer,
    size: Vec2,
}

fn once(seconds: f32) -> Timer {
    Timer::from_seconds(seconds.max(0.01), TimerMode::Once)
}

/// Цели умения по порядку попаданий: ближайшие события урона от этого бойца.
fn targets(queue: &std::collections::VecDeque<Event>, actor: UnitId) -> Vec<UnitId> {
    let mut out = Vec::new();
    for event in queue {
        match *event {
            Event::Damage {
                source: Some(source),
                target,
                ..
            } if source == actor => {
                if !out.contains(&target) {
                    out.push(target);
                }
            }
            Event::AbilityUsed { .. } | Event::TurnStarted { .. } | Event::BattleEnded { .. } => {
                break;
            }
            _ => {}
        }
    }
    out
}

/// Дешёвый генератор для формы молний: эффект, а не бой — детерминизм не нужен.
fn next(seed: &mut u32) -> f32 {
    *seed ^= *seed << 13;
    *seed ^= *seed >> 17;
    *seed ^= *seed << 5;
    (*seed % 10_000) as f32 / 10_000.0
}

/// Ломаные молнии (ствол и ответвления) лентой заданной ширины.
fn bolt_mesh(lines: &[Vec<Vec2>], width: f32) -> Mesh {
    let mut positions = Vec::new();
    let mut indices = Vec::new();
    for pair in lines.iter().flat_map(|l| l.windows(2)) {
        let (a, b) = (pair[0], pair[1]);
        let n = (b - a).perp().normalize_or_zero() * width * 0.5;
        let i = positions.len() as u32;
        for p in [a - n, a + n, b - n, b + n] {
            positions.push([p.x, p.y, 0.0]);
        }
        indices.extend_from_slice(&[i, i + 1, i + 2, i + 1, i + 3, i + 2]);
    }
    let count = positions.len();
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0, 0.0, 1.0]; count])
    .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, vec![[0.0, 0.0]; count])
    .with_inserted_indices(Indices::U32(indices))
}

/// Молния от точки к точке: зигзаг и два коротких ответвления.
fn zigzag(from: Vec2, to: Vec2, steps: usize, jitter: f32, seed: &mut u32) -> Vec<Vec<Vec2>> {
    let mut points = vec![from];
    for i in 1..steps {
        let k = i as f32 / steps as f32;
        points.push(from.lerp(to, k) + Vec2::new((next(seed) - 0.5) * jitter, 0.0));
    }
    points.push(to);
    let down = (to - from).normalize_or(Vec2::NEG_Y);
    let mut lines = vec![points.clone()];
    for (i, side) in [(steps / 3, 0.7_f32), (steps * 2 / 3, -0.6)] {
        let mut p = points[i];
        let dir = Vec2::from_angle(side).rotate(down);
        let mut branch = vec![p];
        for _ in 0..3 {
            p += dir * 24.0 + Vec2::new((next(seed) - 0.5) * 18.0, 0.0);
            branch.push(p);
        }
        lines.push(branch);
    }
    lines
}

/// Молния: широкое голубое свечение и белая сердцевина, мерцает и гаснет (Bolt).
fn spawn_bolt(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<ColorMaterial>,
    lines: &[Vec<Vec2>],
    width: f32,
    seconds: f32,
) {
    for (glow, w, color) in [(true, 22.0, BOLT_GLOW), (false, 5.5, BOLT_CORE)] {
        commands.spawn((
            SpellRoot,
            Bolt {
                timer: once(seconds),
                glow,
            },
            Mesh2d(meshes.add(bolt_mesh(lines, w * width))),
            MeshMaterial2d(materials.add(color)),
            Transform::from_xyz(0.0, 0.0, Z_BOLT + if glow { 0.0 } else { 0.1 }),
        ));
    }
}

/// Свечение в точке: вспыхивает и расходится (Spark).
fn glow_sprite(assets: &AssetServer, at: Vec2, size: f32, seconds: f32) -> impl Bundle {
    (
        SpellRoot,
        Spark {
            timer: once(seconds),
            grow: 1.2,
            alpha: 1.0,
        },
        Sprite {
            image: assets.load("fx/glow.png"),
            custom_size: Some(Vec2::splat(size)),
            color: Color::srgb(0.8, 0.9, 1.0),
            ..default()
        },
        Transform::from_xyz(at.x, at.y, Z_BOLT + 0.2),
    )
}

/// Запускает эффект умения, если у этого бойца и вида умения он есть.
#[allow(clippy::too_many_arguments)]
pub fn spawn_spells(
    mut commands: Commands,
    mut shown: MessageReader<Shown>,
    screen: Res<BattleScreen>,
    models: Query<(&UnitModel, &Children)>,
    rigs: Query<(), With<Rig>>,
    assets: Res<AssetServer>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    time: Res<Time>,
) {
    let tune = &screen.field.spells.tuning;
    let place = |id: UnitId| {
        models
            .iter()
            .find(|(m, _)| m.id == id)
            .map(|(m, _)| (m.home, m.height, m.facing))
    };
    let rig_of = |id: UnitId| {
        models
            .iter()
            .find(|(m, _)| m.id == id)
            .and_then(|(_, children)| children.iter().find(|c| rigs.contains(*c)))
    };
    for Shown { event, .. } in shown.read() {
        let Event::AbilityUsed { unit, kind, .. } = event else {
            continue;
        };
        let actor = screen.battle.unit(*unit);
        let Some(spell) = screen.field.spells.get(&actor.def.id, *kind) else {
            continue;
        };
        let Some((home, height, facing)) = place(*unit) else {
            continue;
        };
        let windup = screen.windup(*unit, *kind);
        let gap = spell.hit_gap.unwrap_or(screen.look.hit_gap);
        let hits: Vec<(Vec2, f32)> = targets(&screen.queue, *unit)
            .into_iter()
            .filter_map(|t| place(t).map(|(at, h, _)| (at, h)))
            .collect();
        let Some(&(first, first_h)) = hits.first() else {
            continue;
        };
        match spell.fx {
            SpellFx::Gust => {
                let hand = home + Vec2::new(facing * height * 0.3, height * 0.55);
                let length = height * tune.gust_length;
                commands.spawn((
                    SpellRoot,
                    GustFx {
                        timer: once(windup),
                        from: hand,
                        to: first + Vec2::new(0.0, first_h * 0.5),
                        size: Vec2::new(length, length * 0.43),
                    },
                    Sprite {
                        image: assets.load("fx/gust.png"),
                        custom_size: Some(Vec2::new(length, length * 0.43)),
                        ..default()
                    },
                    Transform::from_xyz(hand.x, hand.y, Z_FLOATING - 1.0),
                ));
            }
            SpellFx::Whirlwind => {
                // От бойца — к каждой цели к её попаданию — и дальше за последнюю.
                let start = home + Vec2::new(facing * height * 0.7, 0.0);
                let mut path = vec![(start, 0.0)];
                for (i, (at, _)) in hits.iter().enumerate() {
                    path.push((*at, windup + i as f32 * gap));
                }
                let (last, last_t) = *path.last().unwrap();
                let away = (last - start).normalize_or(Vec2::X) * height * 0.9;
                path.push((last + away, last_t + tune.tornado_exit));
                let h = height * tune.tornado_height;
                let size = Vec2::new(h * 371.0 / 512.0, h);
                let image = assets.load("fx/tornado.png");
                let leaf = meshes.add(Ellipse::new(4.0, 2.2));
                commands
                    .spawn((
                        SpellRoot,
                        Tornado {
                            age: 0.0,
                            size,
                            path,
                            spin_hz: tune.tornado_spin_hz,
                        },
                        Transform::from_xyz(start.x, start.y, Z_TORNADO).with_scale(Vec3::ZERO),
                        Visibility::default(),
                    ))
                    .with_children(|t| {
                        for mirrored in [false, true] {
                            t.spawn((
                                TornadoLayer { mirrored },
                                Sprite {
                                    image: image.clone(),
                                    custom_size: Some(size),
                                    flip_x: mirrored,
                                    ..default()
                                },
                                // Низ рисунка — в точке на палубе.
                                Transform::from_xyz(0.0, h * 0.5, if mirrored { 0.1 } else { 0.0 }),
                            ));
                        }
                        for i in 0..LEAVES {
                            let k = i as f32 / LEAVES as f32;
                            t.spawn((
                                Leaf {
                                    level: 0.15 + 0.8 * ((k * 7.0) % 1.0),
                                    phase: k * TAU * 3.0,
                                    speed: 0.8 + 0.6 * ((k * 13.0) % 1.0),
                                },
                                Mesh2d(leaf.clone()),
                                MeshMaterial2d(materials.add(LEAF)),
                                Transform::from_xyz(0.0, 0.0, 0.2),
                            ));
                        }
                    });
            }
            SpellFx::Storm => {
                let strikes: Vec<(Vec2, f32, bool)> = hits
                    .iter()
                    .enumerate()
                    .map(|(i, (at, h))| {
                        (
                            *at + Vec2::new(0.0, h * 0.45),
                            windup + i as f32 * gap,
                            false,
                        )
                    })
                    .collect();
                let end = strikes.last().map_or(windup, |s| s.1) + tune.storm_linger;
                let cx = hits.iter().map(|(at, _)| at.x).sum::<f32>() / hits.len() as f32;
                let top = hits.iter().map(|(at, h)| at.y + h).fold(f32::MIN, f32::max);
                let cy = tune.storm_cloud_y.max(top + 60.0);
                let cloud = Vec2::new(
                    tune.storm_cloud_width,
                    tune.storm_cloud_width * 420.0 / 1024.0,
                );
                let bottom = cy - cloud.y * 0.25;
                let deck = hits.iter().map(|(at, _)| at.y).fold(f32::MAX, f32::min) - 30.0;
                let rain = meshes.add(Rectangle::new(2.2, 30.0));
                let rain_color = materials.add(RAIN);
                let seed = (time.elapsed_secs() * 1000.0) as u32 | 1;
                commands
                    .spawn((
                        SpellRoot,
                        Storm {
                            age: 0.0,
                            windup,
                            end,
                            strikes,
                            cloud_bottom: bottom,
                            center_x: cx,
                            width: cloud.x,
                            next_flicker: windup * 0.35,
                            seed,
                        },
                        Transform::default(),
                        Visibility::default(),
                    ))
                    .with_children(|s| {
                        s.spawn((
                            StormCloud,
                            Sprite {
                                image: assets.load("fx/storm_cloud.png"),
                                custom_size: Some(cloud),
                                color: Color::WHITE.with_alpha(0.0),
                                ..default()
                            },
                            Transform::from_xyz(cx, cy, Z_CLOUD),
                        ));
                        let mut r = seed;
                        for _ in 0..tune.rain_drops {
                            let x = cx + (next(&mut r) - 0.5) * cloud.x * 0.85;
                            s.spawn((
                                Raindrop {
                                    x,
                                    phase: next(&mut r),
                                    top: bottom,
                                    bottom: deck,
                                },
                                Mesh2d(rain.clone()),
                                MeshMaterial2d(rain_color.clone()),
                                Transform::from_xyz(x, bottom, Z_RAIN)
                                    .with_rotation(Quat::from_rotation_z(0.18))
                                    .with_scale(Vec3::ZERO),
                            ));
                        }
                    });
            }
            SpellFx::Thrust => {
                // Укол: веретено света вдоль выпада, остриё — в цели.
                let hit = first + Vec2::new(0.0, first_h * 0.5);
                let dir = (hit - (home + Vec2::new(0.0, height * 0.5))).normalize_or(Vec2::X);
                let size = Vec2::new(height * 1.3, height * 0.32);
                commands.spawn(blades(
                    vec![
                        (
                            windup - 0.05,
                            Piece::Slash {
                                at: hit - dir * size.x * 0.3,
                                angle: dir.y.atan2(dir.x),
                                size,
                                thrust: true,
                                flip: false,
                            },
                        ),
                        (
                            windup,
                            Piece::Glow {
                                at: hit,
                                size: first_h * 0.9,
                            },
                        ),
                        (
                            windup,
                            Piece::Arcs {
                                at: hit,
                                radius: first_h * 0.35,
                            },
                        ),
                    ],
                    &time,
                ));
            }
            SpellFx::WhirlSlash => {
                // Оборот на бегу (кадры листа), кольцо взмаха вокруг бойца; к удару — широкий
                // разрез по цели. Клип ближнего боя растянут так, что к замаху боец у цели.
                let clip = windup / crate::fx::MELEE_ARRIVE;
                let hit = first + Vec2::new(0.0, first_h * 0.5);
                let w = first_h * 1.8;
                let size = Vec2::new(w, w * 200.0 / 512.0);
                let flip = facing < 0.0;
                let cut = |angle: f32, k: f32| Piece::Slash {
                    at: hit,
                    angle: angle * facing,
                    size: size * k,
                    thrust: false,
                    flip,
                };
                let mut steps = vec![
                    (windup - gap.max(0.1), cut(0.25, 0.8)),
                    (windup, cut(-0.2, 1.0)),
                    (
                        windup,
                        Piece::Glow {
                            at: hit,
                            size: first_h * 1.2,
                        },
                    ),
                    (
                        windup,
                        Piece::Arcs {
                            at: hit,
                            radius: first_h * 0.45,
                        },
                    ),
                ];
                // Кольцо — по росту рисунка (рисованный боец бывает выше своего слота).
                let stature = screen
                    .field
                    .animated
                    .get(&actor.def.id)
                    .map_or(height, |art| (art.top - art.foot) * art.height * height);
                if let Some(rig) = rig_of(*unit) {
                    steps.push((
                        clip * 0.06,
                        Piece::Whirl {
                            rig,
                            height: stature,
                            seconds: windup - clip * 0.06 + 0.12,
                        },
                    ));
                }
                commands.spawn(blades(steps, &time));
            }
            SpellFx::ThunderStrike => {
                // Молния с неба в поднятые сабли, пока боец заряжается; потом — в цель и крест.
                let sabers = home + Vec2::new(0.0, height * 1.08);
                let hit = first + Vec2::new(0.0, first_h * 0.5);
                let w = first_h * 1.9;
                let size = Vec2::new(w, w * 200.0 / 512.0);
                let flip = facing < 0.0;
                let cut = |angle: f32| Piece::Slash {
                    at: hit,
                    angle: angle * facing,
                    size,
                    thrust: false,
                    flip,
                };
                commands.spawn(blades(
                    vec![
                        (
                            0.05,
                            Piece::SkyBolt {
                                to: sabers,
                                width: 0.9,
                            },
                        ),
                        (
                            0.05,
                            Piece::Glow {
                                at: sabers,
                                size: height * 0.9,
                            },
                        ),
                        (
                            0.05,
                            Piece::Arcs {
                                at: sabers,
                                radius: height * 0.3,
                            },
                        ),
                        (
                            0.35,
                            Piece::SkyBolt {
                                to: sabers,
                                width: 0.6,
                            },
                        ),
                        (
                            0.35,
                            Piece::Arcs {
                                at: sabers,
                                radius: height * 0.25,
                            },
                        ),
                        (
                            windup - 0.12,
                            Piece::SkyBolt {
                                to: hit,
                                width: 1.4,
                            },
                        ),
                        (
                            windup - 0.12,
                            Piece::Glow {
                                at: hit,
                                size: first_h * 1.2,
                            },
                        ),
                        (windup, cut(-0.75)),
                        (windup, cut(0.75)),
                        (
                            windup,
                            Piece::Glow {
                                at: hit,
                                size: first_h * 1.8,
                            },
                        ),
                        (
                            windup,
                            Piece::Arcs {
                                at: hit,
                                radius: first_h * 0.6,
                            },
                        ),
                        (windup, Piece::Flash),
                    ],
                    &time,
                ));
            }
        }
    }
}

fn blades(steps: Vec<(f32, Piece)>, time: &Time) -> impl Bundle {
    (
        SpellRoot,
        Blades {
            age: 0.0,
            steps,
            seed: (time.elapsed_secs() * 1000.0) as u32 | 1,
        },
    )
}

/// Удары клинком: по расписанию появляются разрезы, молнии, свечение, проблеск.
#[allow(clippy::too_many_arguments)]
pub fn animate_blades(
    mut commands: Commands,
    time: Res<Time>,
    screen: Res<BattleScreen>,
    mut list: Query<(Entity, &mut Blades)>,
    mut slashes: Query<(Entity, &mut SlashFx, &mut Transform, &mut Sprite)>,
    assets: Res<AssetServer>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    mut screen_fx: ResMut<crate::fx::ScreenFx>,
) {
    let dt = crate::fx::fx_delta(&time, &screen).as_secs_f32();
    let tune = &screen.field.spells.tuning;
    for (entity, mut blades) in &mut list {
        blades.age += dt;
        let age = blades.age;
        let mut seed = blades.seed;
        let due: Vec<Piece> = blades
            .steps
            .iter()
            .filter(|(at, _)| *at <= age)
            .map(|(_, p)| *p)
            .collect();
        blades.steps.retain(|(at, _)| *at > age);
        for piece in due {
            match piece {
                Piece::Slash {
                    at,
                    angle,
                    size,
                    thrust,
                    flip,
                } => {
                    let image = if thrust {
                        "fx/thrust.png"
                    } else {
                        "fx/slash.png"
                    };
                    commands.spawn((
                        SpellRoot,
                        SlashFx {
                            timer: once(tune.slash_time),
                            size,
                        },
                        Sprite {
                            image: assets.load(image),
                            custom_size: Some(size),
                            flip_x: flip,
                            ..default()
                        },
                        Transform::from_xyz(at.x, at.y, Z_BOLT + 0.3)
                            .with_rotation(Quat::from_rotation_z(angle)),
                    ));
                }
                Piece::SkyBolt { to, width } => {
                    let from = Vec2::new(to.x + (next(&mut seed) - 0.5) * 60.0, tune.sky_y);
                    let lines = zigzag(from, to, 12, 40.0, &mut seed);
                    spawn_bolt(
                        &mut commands,
                        &mut meshes,
                        &mut materials,
                        &lines,
                        width,
                        tune.bolt_time,
                    );
                }
                Piece::Glow { at, size } => {
                    commands.spawn(glow_sprite(&assets, at, size, 0.35));
                }
                Piece::Arcs { at, radius } => {
                    let lines: Vec<Vec<Vec2>> = (0..4)
                        .map(|i| {
                            let dir =
                                Vec2::from_angle(i as f32 * TAU / 4.0 + next(&mut seed) * 1.2);
                            let mut p = at + dir * radius * 0.3;
                            let mut line = vec![p];
                            for _ in 0..3 {
                                p += dir * radius * 0.25
                                    + dir.perp() * (next(&mut seed) - 0.5) * radius * 0.35;
                                line.push(p);
                            }
                            line
                        })
                        .collect();
                    spawn_bolt(
                        &mut commands,
                        &mut meshes,
                        &mut materials,
                        &lines,
                        0.45,
                        tune.bolt_time * 0.8,
                    );
                }
                Piece::Flash => {
                    commands.spawn((
                        SpellRoot,
                        Spark {
                            timer: once(0.2),
                            grow: 0.0,
                            alpha: 0.45,
                        },
                        Sprite::from_color(Color::WHITE, Vec2::new(1400.0, 2400.0)),
                        Transform::from_xyz(0.0, 0.0, Z_CLOUD - 1.0),
                    ));
                    screen_fx.shake(0.3, 12.0);
                }
                Piece::Whirl {
                    rig,
                    height,
                    seconds,
                } => {
                    // Риг мог исчезнуть (конец боя) — тогда без кольца.
                    if let Ok(mut rig) = commands.get_entity(rig) {
                        rig.with_children(|p| {
                            p.spawn((
                                WhirlFx {
                                    timer: once(seconds),
                                    turns: 2.0,
                                },
                                Transform::from_xyz(0.0, height * 0.45, 0.05)
                                    .with_scale(Vec3::new(1.0, 0.32, 1.0)),
                                Visibility::default(),
                            ))
                            .with_child((
                                Sprite {
                                    image: assets.load("fx/whirl.png"),
                                    custom_size: Some(Vec2::splat(height * 1.35)),
                                    color: Color::WHITE.with_alpha(0.0),
                                    ..default()
                                },
                                Transform::default(),
                            ));
                        });
                    }
                }
            }
        }
        blades.seed = seed;
        if blades.steps.is_empty() {
            commands.entity(entity).despawn();
        }
    }
    for (entity, mut slash, mut transform, mut sprite) in &mut slashes {
        slash.timer.tick(std::time::Duration::from_secs_f32(dt));
        let t = slash.timer.fraction();
        // Прочерчивается за первую треть (растёт вдоль), потом сужается и гаснет.
        let draw = (t / 0.3).min(1.0);
        let fade = ((t - 0.3) / 0.7).clamp(0.0, 1.0);
        transform.scale = Vec3::new(0.35 + 0.65 * draw, 1.0 - 0.5 * fade, 1.0);
        sprite.custom_size = Some(slash.size);
        sprite.color = Color::WHITE.with_alpha(1.0 - fade * fade);
        if slash.timer.is_finished() {
            commands.entity(entity).despawn();
        }
    }
}

/// Круговой взмах: кольцо крутится, голова спереди — перед бойцом, сзади — за ним.
pub fn animate_whirls(
    mut commands: Commands,
    time: Res<Time>,
    screen: Res<BattleScreen>,
    mut whirls: Query<(Entity, &mut WhirlFx, &mut Transform, &Children)>,
    mut rings: Query<(&mut Transform, &mut Sprite), Without<WhirlFx>>,
) {
    let dt = crate::fx::fx_delta(&time, &screen);
    for (entity, mut whirl, mut transform, children) in &mut whirls {
        whirl.timer.tick(dt);
        let t = whirl.timer.fraction();
        let angle = t * whirl.turns * TAU;
        // Нижняя половина эллипса ближе к зрителю.
        transform.translation.z = if angle.sin() < 0.0 { 0.05 } else { -0.05 };
        let grow = 0.85 + 0.2 * t;
        transform.scale = Vec3::new(grow, 0.32 * grow, 1.0);
        let alpha = (t / 0.12).min(1.0) * ((1.0 - t) / 0.25).min(1.0);
        for child in children.iter() {
            if let Ok((mut ring, mut sprite)) = rings.get_mut(child) {
                ring.rotation = Quat::from_rotation_z(angle);
                sprite.color = Color::WHITE.with_alpha(alpha);
            }
        }
        if whirl.timer.is_finished() {
            commands.entity(entity).despawn();
        }
    }
}

/// Эффекты умений во времени: полёт, движение смерча, туча, дождь и молнии.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub fn animate_spells(
    mut commands: Commands,
    time: Res<Time>,
    screen: Res<BattleScreen>,
    mut gusts: Query<(Entity, &mut GustFx, &mut Transform, &mut Sprite), Without<Puff>>,
    mut puffs: Query<(Entity, &mut Puff, &mut Transform, &mut Sprite), Without<GustFx>>,
    mut tornados: Query<
        (Entity, &mut Tornado, &mut Transform, &Children),
        (
            Without<GustFx>,
            Without<Puff>,
            Without<Leaf>,
            Without<TornadoLayer>,
        ),
    >,
    mut layers: Query<(&TornadoLayer, &mut Sprite), (Without<GustFx>, Without<Puff>)>,
    mut leaves: Query<
        (&Leaf, &mut Transform),
        (
            Without<GustFx>,
            Without<Puff>,
            Without<Tornado>,
            Without<Raindrop>,
        ),
    >,
    mut storms: Query<(Entity, &mut Storm, &Children)>,
    mut clouds: Query<
        &mut Sprite,
        (
            With<StormCloud>,
            Without<TornadoLayer>,
            Without<GustFx>,
            Without<Puff>,
        ),
    >,
    mut drops: Query<
        (&Raindrop, &mut Transform),
        (
            Without<GustFx>,
            Without<Puff>,
            Without<Tornado>,
            Without<Leaf>,
            Without<Bolt>,
        ),
    >,
    mut bolts: Query<(Entity, &mut Bolt, &MeshMaterial2d<ColorMaterial>), Without<Spark>>,
    mut sparks: Query<
        (Entity, &mut Spark, &mut Transform, &mut Sprite),
        (
            Without<GustFx>,
            Without<Puff>,
            Without<Tornado>,
            Without<Leaf>,
            Without<Raindrop>,
            Without<TornadoLayer>,
            Without<StormCloud>,
        ),
    >,
    assets: Res<AssetServer>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    let dt = crate::fx::fx_delta(&time, &screen).as_secs_f32();
    let now = time.elapsed_secs();
    let tune = &screen.field.spells.tuning;

    for (entity, mut gust, mut transform, mut sprite) in &mut gusts {
        gust.timer.tick(std::time::Duration::from_secs_f32(dt));
        let t = gust.timer.fraction();
        let at = gust.from.lerp(gust.to, t * t * (3.0 - 2.0 * t));
        let dir = (gust.to - gust.from).normalize_or(Vec2::X);
        transform.translation.x = at.x;
        transform.translation.y = at.y;
        transform.rotation = Quat::from_rotation_z(dir.y.atan2(dir.x));
        // Разгоняется из узкого в полный серп.
        let grow = 0.5 + 0.5 * (t * 2.0).min(1.0);
        sprite.custom_size = Some(gust.size * grow);
        if gust.timer.is_finished() {
            let mut puff = sprite.clone();
            puff.custom_size = Some(gust.size);
            commands.spawn((SpellRoot, Puff { timer: once(0.3) }, puff, *transform));
            commands.entity(entity).despawn();
        }
    }
    for (entity, mut puff, mut transform, mut sprite) in &mut puffs {
        puff.timer.tick(std::time::Duration::from_secs_f32(dt));
        let t = puff.timer.fraction();
        transform.scale = Vec3::splat(1.0 + 0.8 * t);
        sprite.color = Color::WHITE.with_alpha(1.0 - t);
        if puff.timer.is_finished() {
            commands.entity(entity).despawn();
        }
    }

    for (entity, mut tornado, mut transform, children) in &mut tornados {
        tornado.age += dt;
        let age = tornado.age;
        let end = tornado.path.last().map_or(0.0, |p| p.1);
        // Где смерч сейчас: между точками пути по времени.
        let mut at = tornado.path[0].0;
        for pair in tornado.path.windows(2) {
            let ((a, ta), (b, tb)) = (pair[0], pair[1]);
            if age >= ta {
                let k = ((age - ta) / (tb - ta).max(0.01)).min(1.0);
                at = a.lerp(b, k);
            }
        }
        transform.translation.x = at.x;
        transform.translation.y = at.y;
        // Вырастает из ветра в начале и тает в конце.
        let appear = (age / 0.25).min(1.0);
        let fade = ((end - age) / 0.3).clamp(0.0, 1.0);
        let wobble = (now * 7.0).sin() * 0.06;
        transform.scale = Vec3::new(appear * (1.0 + wobble), appear, 1.0);
        transform.rotation = Quat::from_rotation_z((now * 5.0).sin() * 0.05);
        // Рисунок и его отражение сменяют друг друга — так смерч крутится.
        let turn = (age * tornado.spin_hz * TAU).sin() * 0.5 + 0.5;
        let size = tornado.size;
        for child in children.iter() {
            if let Ok((layer, mut sprite)) = layers.get_mut(child) {
                let a = if layer.mirrored {
                    turn
                } else {
                    1.0 - turn * 0.6
                };
                sprite.color = Color::WHITE.with_alpha(a * fade);
            }
            if let Ok((leaf, mut leaf_t)) = leaves.get_mut(child) {
                let angle = leaf.phase + age * tornado.spin_hz * leaf.speed * TAU;
                // Радиус смерча растёт кверху.
                let radius = size.x * (0.12 + 0.36 * leaf.level);
                let depth = angle.sin();
                leaf_t.translation = Vec3::new(
                    angle.cos() * radius,
                    size.y * leaf.level,
                    0.2 * depth.signum(),
                );
                leaf_t.rotation = Quat::from_rotation_z(angle * 1.7);
                leaf_t.scale = Vec3::splat((1.0 + 0.35 * depth) * fade);
            }
        }
        if age >= end {
            commands.entity(entity).despawn();
        }
    }

    for (entity, mut storm, children) in &mut storms {
        storm.age += dt;
        let age = storm.age;
        // Туча сгущается за замах и уходит после последней молнии.
        let gather = (age / (storm.windup * 0.7).max(0.05)).min(1.0);
        let leave = ((storm.end - age) / 0.5).clamp(0.0, 1.0);
        let presence = gather.min(leave);
        // Свечение тучи изнутри сразу после удара молнии.
        let lit = storm
            .strikes
            .iter()
            .filter(|s| s.2)
            .map(|s| (1.0 - (age - s.1) / 0.15).clamp(0.0, 1.0))
            .fold(0.0, f32::max);
        for child in children.iter() {
            if let Ok(mut sprite) = clouds.get_mut(child) {
                let k = 1.0 + 1.6 * lit;
                sprite.color = Color::linear_rgba(k, k, k * 1.1, presence);
            }
            if let Ok((drop, mut t)) = drops.get_mut(child) {
                let span = drop.top - drop.bottom;
                let fall = (drop.phase + age * 2.2) % 1.0;
                t.translation.x = drop.x - fall * span * 0.18;
                t.translation.y = drop.top - fall * span;
                t.scale = Vec3::splat(presence);
            }
        }
        let bottom = storm.cloud_bottom;
        let mut seed = storm.seed;
        // Пока туча висит, в ней то и дело сверкает: короткие молнии вниз, ни в кого.
        if age >= storm.next_flicker && age < storm.end - 0.4 {
            storm.next_flicker = age + 0.2 + next(&mut seed) * 0.3;
            let x = storm.center_x + (next(&mut seed) - 0.5) * storm.width * 0.8;
            let mut p = Vec2::new(x, bottom + 30.0);
            let mut line = vec![p];
            for _ in 0..4 {
                p += Vec2::new(
                    (next(&mut seed) - 0.5) * 40.0,
                    -22.0 - next(&mut seed) * 18.0,
                );
                line.push(p);
            }
            commands.spawn((
                SpellRoot,
                Bolt {
                    timer: once(tune.bolt_time * 0.7),
                    glow: true,
                },
                Mesh2d(meshes.add(bolt_mesh(&[line], 9.0))),
                MeshMaterial2d(materials.add(BOLT_GLOW)),
                Transform::from_xyz(0.0, 0.0, Z_BOLT),
            ));
        }
        for strike in storm.strikes.iter_mut() {
            if strike.2 || age < strike.1 {
                continue;
            }
            strike.2 = true;
            // Ломаная от тучи к цели: зигзаг, чуть отклоняясь от прямой.
            let to = strike.0;
            let from = Vec2::new(to.x + (next(&mut seed) - 0.5) * 80.0, bottom);
            let lines = zigzag(from, to, 9, 34.0, &mut seed);
            spawn_bolt(
                &mut commands,
                &mut meshes,
                &mut materials,
                &lines,
                1.0,
                tune.bolt_time,
            );
            // Свечение в месте удара и белый проблеск на весь экран.
            commands.spawn(glow_sprite(&assets, to, 150.0, 0.3));
            commands.spawn((
                SpellRoot,
                Spark {
                    timer: once(0.14),
                    grow: 0.0,
                    alpha: 0.3,
                },
                Sprite::from_color(Color::WHITE, Vec2::new(1400.0, 2400.0)),
                Transform::from_xyz(0.0, 0.0, Z_CLOUD - 1.0),
            ));
        }
        storm.seed = seed;
        if age >= storm.end {
            commands.entity(entity).despawn();
        }
    }

    for (entity, mut bolt, material) in &mut bolts {
        bolt.timer.tick(std::time::Duration::from_secs_f32(dt));
        let t = bolt.timer.fraction();
        // Молния мерцает и гаснет.
        let flicker = if ((t * 14.0) as u32).is_multiple_of(2) {
            1.0
        } else {
            0.55
        };
        let base = if bolt.glow { BOLT_GLOW } else { BOLT_CORE };
        if let Some(mut m) = materials.get_mut(&material.0) {
            m.color = base.with_alpha(base.alpha() * (1.0 - t) * flicker);
        }
        if bolt.timer.is_finished() {
            commands.entity(entity).despawn();
        }
    }
    for (entity, mut spark, mut transform, mut sprite) in &mut sparks {
        spark.timer.tick(std::time::Duration::from_secs_f32(dt));
        let t = spark.timer.fraction();
        transform.scale = Vec3::splat(1.0 + spark.grow * t);
        sprite.color = sprite.color.with_alpha(spark.alpha * (1.0 - t));
        if spark.timer.is_finished() {
            commands.entity(entity).despawn();
        }
    }
}
