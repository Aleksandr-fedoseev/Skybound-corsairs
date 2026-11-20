//! Мягкие области рисунка бойца: рисунок натянут на сетку, отдельные области пружинят.
//!
//! «Пружина» (bounce) — упругое смещение (подол, грудь, бёдра): раскачивается от разгона и
//! торможения рига (рывок, отдача, удар), от смены позы и чуть-чуть от дыхания в покое.
//! «Колыхание» (sway) — волосы, плащ, шарф: отстаёт при движении и плавно возвращается.
//! Области — в `data/softbody.json`: боец → поза (клип листа) → список областей в долях кадра.

use std::collections::HashMap;
use std::f32::consts::TAU;

use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use serde::Deserialize;

use crate::battle_screen::BattleScreen;
use crate::battlefield::{Flipbook, ModelAnim, Rig};

const COLS: usize = 10;
const ROWS: usize = 14;
/// Жёсткость пружин (1/с²) и доля критического затухания: меньше — дольше колышется.
const BOUNCE_STIFFNESS: f32 = 140.0;
const BOUNCE_DAMPING: f32 = 0.16;
const SWAY_STIFFNESS: f32 = 45.0;
const SWAY_DAMPING: f32 = 0.35;
/// Насколько область отзывается на разгон рига (с²) и предел смещения (доля высоты рисунка).
const INERTIA: f32 = 0.012;
const BOUNCE_LIMIT: f32 = 0.03;
const SWAY_LIMIT: f32 = 0.05;
/// Толчок вниз при смене позы (доля высоты в секунду) и дыхание в покое.
const POSE_KICK: f32 = 0.35;
const BREATH_PERIOD: f32 = 2.2;
const BREATH_AMP: f32 = 0.004;

#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Bounce,
    Sway,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Region {
    /// Центр в долях кадра: x слева направо, y сверху вниз.
    pub at: [f32; 2],
    /// Радиус в долях высоты кадра.
    pub r: f32,
    pub kind: Kind,
    #[serde(default = "one")]
    pub weight: f32,
}

fn one() -> f32 {
    1.0
}

/// Боец → поза → области.
pub type SoftData = HashMap<String, HashMap<String, Vec<Region>>>;

pub fn bundled() -> SoftData {
    serde_json::from_str(include_str!("../../../data/softbody.json"))
        .expect("data/softbody.json корректен")
}

/// Сетка вместо прямоугольника: вершины можно смещать, UV — кадр листа (через uv_transform).
pub fn grid_mesh(size: Vec2) -> (Mesh, Vec<[f32; 3]>, Vec<[f32; 2]>) {
    let mut positions = Vec::with_capacity((COLS + 1) * (ROWS + 1));
    let mut uvs = Vec::with_capacity(positions.capacity());
    for j in 0..=ROWS {
        for i in 0..=COLS {
            let (u, v) = (i as f32 / COLS as f32, j as f32 / ROWS as f32);
            positions.push([(u - 0.5) * size.x, (0.5 - v) * size.y, 0.0]);
            uvs.push([u, v]);
        }
    }
    let mut indices = Vec::with_capacity(COLS * ROWS * 6);
    let w = (COLS + 1) as u32;
    for j in 0..ROWS as u32 {
        for i in 0..COLS as u32 {
            let a = j * w + i;
            indices.extend_from_slice(&[a, a + w, a + 1, a + 1, a + w, a + w + 1]);
        }
    }
    let mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions.clone())
    .with_inserted_attribute(
        Mesh::ATTRIBUTE_NORMAL,
        vec![[0.0, 0.0, 1.0]; positions.len()],
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uvs.clone())
    .with_inserted_indices(Indices::U32(indices));
    (mesh, positions, uvs)
}

#[derive(Default, Clone, Copy)]
struct Spring {
    off: Vec2,
    vel: Vec2,
}

/// Состояние мягких областей модели (висит на корне модели рядом с Flipbook).
#[derive(Component)]
pub struct SoftBody {
    pub mesh: Handle<Mesh>,
    base: Vec<[f32; 3]>,
    uvs: Vec<[f32; 2]>,
    size: Vec2,
    poses: HashMap<String, Vec<Region>>,
    springs: Vec<Spring>,
    pose: String,
    last_pos: Option<Vec2>,
    last_vel: Vec2,
}

impl SoftBody {
    pub fn new(
        mesh: Handle<Mesh>,
        base: Vec<[f32; 3]>,
        uvs: Vec<[f32; 2]>,
        size: Vec2,
        poses: HashMap<String, Vec<Region>>,
    ) -> Self {
        Self {
            mesh,
            base,
            uvs,
            size,
            poses,
            springs: Vec::new(),
            pose: String::new(),
            last_pos: None,
            last_vel: Vec2::ZERO,
        }
    }
}

/// Пружины от движения рига и смены позы; вершины сетки смещаются по областям.
pub fn animate_soft_bodies(
    time: Res<Time>,
    screen: Res<BattleScreen>,
    mut models: Query<(&ModelAnim, &Flipbook, &mut SoftBody, &Children)>,
    rigs: Query<&Transform, With<Rig>>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    let dt = if screen.paused {
        0.0
    } else {
        time.delta_secs() * screen.speed as f32
    };
    if dt <= 0.0 {
        return;
    }
    let now = time.elapsed_secs();
    for (anim, book, mut soft, children) in &mut models {
        let pos = children
            .iter()
            .find_map(|c| rigs.get(c).ok())
            .map(|t| t.translation.truncate())
            .unwrap_or(Vec2::ZERO);
        let vel = soft.last_pos.map_or(Vec2::ZERO, |p| (pos - p) / dt);
        let accel = (vel - soft.last_vel) / dt;
        soft.last_pos = Some(pos);
        soft.last_vel = vel;

        let pose = book.art.clip_name(anim.clip).to_string();
        let regions = soft.poses.get(&pose).cloned().unwrap_or_default();
        let h = soft.size.y;
        if soft.springs.len() != regions.len() {
            soft.springs = vec![Spring::default(); regions.len()];
        }
        let kick = pose != soft.pose;
        soft.pose = pose;
        let breath = (TAU * now / BREATH_PERIOD).sin() * BREATH_AMP * h;

        for (spring, region) in soft.springs.iter_mut().zip(&regions) {
            let (k, zeta, limit) = match region.kind {
                Kind::Bounce => (BOUNCE_STIFFNESS, BOUNCE_DAMPING, BOUNCE_LIMIT),
                Kind::Sway => (SWAY_STIFFNESS, SWAY_DAMPING, SWAY_LIMIT),
            };
            if kick {
                spring.vel.y -= POSE_KICK * h;
            }
            // Покой — у «пружины» дыхание, у «колыхания» ноль.
            let rest = if region.kind == Kind::Bounce {
                Vec2::new(0.0, breath)
            } else {
                Vec2::ZERO
            };
            let force = -k * (spring.off - rest)
                - 2.0 * zeta * k.sqrt() * spring.vel
                - accel * INERTIA * k.sqrt();
            spring.vel += force * dt;
            spring.off += spring.vel * dt;
            spring.off = spring
                .off
                .clamp(Vec2::splat(-limit * h), Vec2::splat(limit * h));
        }

        let Some(mut mesh) = meshes.get_mut(&soft.mesh) else {
            continue;
        };
        let size = soft.size;
        let moved: Vec<[f32; 3]> = soft
            .base
            .iter()
            .zip(&soft.uvs)
            .map(|(p, uv)| {
                let mut d = Vec2::ZERO;
                for (spring, region) in soft.springs.iter().zip(&regions) {
                    let dx = (uv[0] - region.at[0]) * size.x;
                    let dy = (uv[1] - region.at[1]) * size.y;
                    let q = (dx * dx + dy * dy).sqrt() / (region.r * size.y);
                    if q < 1.0 {
                        let w = (1.0 - q * q).powi(2) * region.weight;
                        d += spring.off * w;
                    }
                }
                [p[0] + d.x, p[1] + d.y, p[2]]
            })
            .collect();
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, moved);
    }
}
