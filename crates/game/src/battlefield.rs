//! Поле боя в мире: палубы, модели бойцов на слотах и их анимации.
//!
//! Модель — анимированный спрайт-лист (`data/animated.json`: 3D-модель Meshy, отрендеренная в 2D,
//! `tools/meshy/`) или, если листа нет, силуэт-заглушка из простых фигур. И то и другое висит на
//! «риге»: рывки, отбросы и падение двигают риг, а кадры листа показывают движение тела. Раскладка слотов — в `data/battlefield.json`
//! (сейчас «диагональ» и «боковая полоса», в тестовой сцене переключаются клавишей V).

use std::collections::HashMap;
use std::f32::consts::{FRAC_PI_2, PI};

use battle_core::{ENERGY_MAX, Side, UnitId};
use bevy::math::Affine2;
use bevy::prelude::*;
use bevy::sprite::Anchor;
use serde::Deserialize;

use crate::battle_screen::BattleScreen;
use crate::view::{Fonts, element_color};

const ALLY_DECK: Color = Color::srgb(0.47, 0.32, 0.19);
const ENEMY_DECK: Color = Color::srgb(0.36, 0.22, 0.20);
const RAIL: Color = Color::srgb(0.30, 0.19, 0.11);
/// Непрозрачные: полупрозрачные меши рисуются после непрозрачных и легли бы поверх палуб.
const SHADOW: Color = Color::srgba(0.0, 0.0, 0.0, 0.25);
const SKIN: Color = Color::srgb(0.98, 0.85, 0.72);
const LEGS: Color = Color::srgb(0.20, 0.22, 0.30);
const WEAPON: Color = Color::srgb(0.75, 0.78, 0.82);
const ENEMY_TINT: Color = Color::srgb(0.55, 0.20, 0.22);
const BAR_BACK: Color = Color::srgba(0.0, 0.0, 0.0, 0.55);
const HP_ALLY: Color = Color::srgb(0.38, 0.88, 0.45);
const HP_ENEMY: Color = Color::srgb(0.95, 0.30, 0.28);
const ENERGY: Color = Color::srgb(0.35, 0.65, 1.0);
const GOLD: Color = Color::srgb(1.0, 0.85, 0.35);
const BAR_WIDTH: f32 = 72.0;
/// Слои по глубине: фон, затемнение, модели (чем ниже на экране, тем ближе), цифры.
const Z_BACKGROUND: f32 = 0.0;
const Z_DECK: f32 = 1.0;
const Z_DIM: f32 = 2.0;
const Z_MODELS: f32 = 10.0;
pub const Z_FLOATING: f32 = 50.0;

#[derive(Clone, Debug, Deserialize)]
pub struct Deck {
    pub center: [f32; 2],
    pub size: [f32; 2],
    pub angle: f32,
    pub ally: bool,
    /// Непрозрачность плашки: 0 — не рисовать (герои стоят на палубе с фона).
    #[serde(default = "opaque")]
    pub alpha: f32,
}

fn opaque() -> f32 {
    1.0
}

#[derive(Clone, Debug, Deserialize)]
pub struct Layout {
    /// Название для строки тестовой сцены.
    #[cfg_attr(not(feature = "dev"), allow(dead_code))]
    pub name: String,
    pub model_height: f32,
    pub ally_slots: Vec<[f32; 2]>,
    pub enemy_slots: Vec<[f32; 2]>,
    pub decks: Vec<Deck>,
}

/// Раскладки поля боя из `data/battlefield.json`.
#[derive(Clone, Debug, Deserialize)]
pub struct FieldConfig {
    /// Фон боя во весь экран (путь в assets/).
    pub background: String,
    pub default: String,
    /// Во сколько раз крупнее модель (боссы).
    pub unit_scale: HashMap<String, f32>,
    pub layouts: HashMap<String, Layout>,
    /// Спрайт-листы бойцов из `data/animated.json`; у кого нет — заглушка.
    #[serde(skip)]
    pub animated: HashMap<String, AnimatedArt>,
    /// Дальнобойные бойцы и чем они стреляют; остальные бьют вблизи (рывок к цели).
    #[serde(default)]
    pub ranged: HashMap<String, Shot>,
}

/// Снаряд дальнобойного бойца.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Shot {
    /// Пуля: короткая яркая черта и вспышка у ствола.
    Bullet,
    /// Склянка: летит дугой, крутится.
    Flask,
    /// Порыв ветра.
    Wind,
    /// Разряд молнии.
    Spark,
}

/// Спрайт-лист бойца: строка — клип, столбец — кадр; боец смотрит вправо.
#[derive(Clone, Debug, Deserialize)]
pub struct AnimatedArt {
    /// Путь внутри `assets/`.
    pub sheet: String,
    /// Размер кадра в пикселях листа — для пропорций.
    pub frame: [f32; 2],
    pub columns: u32,
    pub rows: u32,
    /// Высота кадра в долях высоты модели.
    pub height: f32,
    /// Где в кадре ноги и макушка (доли высоты кадра снизу).
    pub foot: f32,
    pub top: f32,
    pub clips: HashMap<String, SheetClip>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct SheetClip {
    pub row: u32,
    pub frames: u32,
    /// Только у зацикленных (ожидание, победа); остальные растягиваются на время клипа рига.
    #[serde(default)]
    pub fps: Option<f32>,
}

impl AnimatedArt {
    /// Клип листа для клипа рига: ульта и умение — `skill`, если он есть.
    fn clip(&self, clip: Clip) -> Option<&SheetClip> {
        let name = match clip {
            Clip::Idle => "idle",
            Clip::Attack { big: true, .. } if self.clips.contains_key("skill") => "skill",
            Clip::Attack { .. } => "attack",
            Clip::Hit { .. } => "hit",
            Clip::Death => "death",
            Clip::Victory => "victory",
        };
        self.clips.get(name).or_else(|| self.clips.get("idle"))
    }

    /// Сдвиг UV на кадр `frame` строки `row`.
    fn uv(&self, row: u32, frame: u32) -> Affine2 {
        let size = Vec2::new(1.0 / self.columns as f32, 1.0 / self.rows as f32);
        Affine2::from_scale_angle_translation(size, 0.0, size * Vec2::new(frame as f32, row as f32))
    }
}

impl FieldConfig {
    pub fn bundled() -> Self {
        let mut config: Self = serde_json::from_str(include_str!("../../../data/battlefield.json"))
            .expect("data/battlefield.json корректен");
        config.animated = serde_json::from_str(include_str!("../../../data/animated.json"))
            .expect("data/animated.json корректен");
        config
    }

    /// Имена раскладок в постоянном порядке, чтобы V переключала их по кругу.
    #[cfg_attr(not(feature = "dev"), allow(dead_code))]
    pub fn names(&self) -> Vec<String> {
        let mut names: Vec<String> = self.layouts.keys().cloned().collect();
        names.sort();
        names
    }
}

#[derive(Component)]
pub struct FieldRoot;

#[derive(Component)]
pub struct SkyDim;

/// Корень модели бойца: стоит на слоте, по нему считаются позиции для рывков и цифр.
#[derive(Component)]
pub struct UnitModel {
    pub id: UnitId,
    pub home: Vec2,
    pub height: f32,
    /// 1 — смотрит вправо, -1 — влево.
    pub facing: f32,
    /// Чем стреляет дальнобойный боец; `None` — ближний бой.
    pub shot: Option<Shot>,
}

/// То, что двигают анимации: силуэт сейчас, спрайт или скелет потом.
#[derive(Component)]
pub struct Rig;

/// Материалы частей модели: для вспышки при ударе и угасания при смерти.
#[derive(Component)]
pub struct ModelParts {
    /// Материал, исходный цвет и есть ли текстура (её не осветлить смешиванием с белым).
    parts: Vec<(Handle<ColorMaterial>, Color, bool)>,
}

/// Модель со спрайт-листом: материал, у которого меняется кадр.
#[derive(Component)]
pub struct Flipbook {
    material: Handle<ColorMaterial>,
    art: AnimatedArt,
}

#[derive(Component)]
pub struct Ring(pub UnitId);

#[derive(Component)]
pub struct Overhead(pub UnitId);

#[derive(Component)]
pub struct HpBar(pub UnitId);

#[derive(Component)]
pub struct EnergyBar(pub UnitId);

/// Текущий клип анимации модели. Ожидание (дыхание) идёт всегда, клипы — поверх.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Clip {
    Idle,
    /// Рывок к точке и назад; `big` — ульта с приседом.
    Attack {
        to: Vec2,
        big: bool,
    },
    /// Отброс от удара в сторону `dir`.
    Hit {
        dir: Vec2,
    },
    Death,
    Victory,
}

#[derive(Component)]
pub struct ModelAnim {
    pub clip: Clip,
    pub timer: Timer,
    pub flash: f32,
}

impl ModelAnim {
    pub fn play(&mut self, clip: Clip, seconds: f32) {
        if self.clip == Clip::Death {
            return;
        }
        self.clip = clip;
        self.timer = Timer::from_seconds(seconds.max(0.01), TimerMode::Once);
    }
}

pub fn current_layout(screen: &BattleScreen) -> &Layout {
    &screen.field.layouts[&screen.layout]
}

/// Собрать поле боя: фон, палубы, модели на слотах.
pub fn spawn_field(
    commands: &mut Commands,
    assets: &AssetServer,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<ColorMaterial>,
    screen: &BattleScreen,
    fonts: &Fonts,
) {
    let layout = current_layout(screen);
    let root = commands
        .spawn((FieldRoot, Transform::default(), Visibility::default()))
        .id();

    let child = |commands: &mut Commands,
                 bundle: (Mesh2d, MeshMaterial2d<ColorMaterial>, Transform)| {
        let id = commands.spawn(bundle).id();
        commands.entity(root).add_child(id);
    };

    // Фон: палуба «Ласточки» над морем облаков, во весь экран.
    let background = commands
        .spawn((
            Sprite {
                image: assets.load(screen.field.background.clone()),
                custom_size: Some(Vec2::new(crate::WIDTH, crate::HEIGHT)),
                ..default()
            },
            Transform::from_xyz(0.0, 0.0, Z_BACKGROUND),
        ))
        .id();
    commands.entity(root).add_child(background);
    // Палубы кораблей с бортом.
    for deck in &layout.decks {
        let angle = deck.angle.to_radians();
        let [w, h] = deck.size;
        if deck.alpha <= 0.0 {
            continue;
        }
        let color = if deck.ally { ALLY_DECK } else { ENEMY_DECK }.with_alpha(deck.alpha);
        let at = Vec3::new(deck.center[0], deck.center[1], Z_DECK);
        child(
            commands,
            (
                Mesh2d(meshes.add(Rectangle::new(w, h))),
                MeshMaterial2d(materials.add(color)),
                Transform::from_translation(at).with_rotation(Quat::from_rotation_z(angle)),
            ),
        );
        let rail = Quat::from_rotation_z(angle) * Vec3::new(0.0, h / 2.0, 0.01);
        child(
            commands,
            (
                Mesh2d(meshes.add(Rectangle::new(w, 8.0))),
                MeshMaterial2d(materials.add(RAIL.with_alpha(deck.alpha))),
                Transform::from_translation(at + rail).with_rotation(Quat::from_rotation_z(angle)),
            ),
        );
    }
    // Затемнение неба на ульте: над палубами, под моделями.
    let dim = commands
        .spawn((
            SkyDim,
            Sprite {
                color: Color::NONE,
                custom_size: Some(Vec2::new(2000.0, 2000.0)),
                ..default()
            },
            Transform::from_xyz(0.0, 0.0, Z_DIM),
        ))
        .id();
    commands.entity(root).add_child(dim);

    let (mut ally_i, mut enemy_i) = (0, 0);
    for unit in screen.battle.units() {
        let (slots, i) = match unit.side {
            Side::Ally => (&layout.ally_slots, &mut ally_i),
            Side::Enemy => (&layout.enemy_slots, &mut enemy_i),
        };
        let Some(&[x, y]) = slots.get(*i) else {
            continue;
        };
        *i += 1;
        let scale = screen
            .field
            .unit_scale
            .get(&unit.def.id)
            .copied()
            .unwrap_or(1.0);
        let model = spawn_model(
            commands,
            assets,
            meshes,
            materials,
            fonts,
            screen,
            unit.id,
            Vec2::new(x, y),
            layout.model_height * scale,
        );
        commands.entity(root).add_child(model);
    }
}

#[allow(clippy::too_many_arguments)]
fn spawn_model(
    commands: &mut Commands,
    assets: &AssetServer,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<ColorMaterial>,
    fonts: &Fonts,
    screen: &BattleScreen,
    id: UnitId,
    home: Vec2,
    h: f32,
) -> Entity {
    let unit = screen.battle.unit(id);
    let ally = unit.side == Side::Ally;
    let facing = if ally { 1.0 } else { -1.0 };
    let element = element_color(unit.def.element);
    // Враги в тёмной «форме», чтобы стороны различались и на заглушках.
    let body_color = if ally {
        element
    } else {
        element.mix(&ENEMY_TINT, 0.45)
    };

    let mut parts = Vec::new();
    let mut mat = |materials: &mut Assets<ColorMaterial>, color: Color| {
        let handle = materials.add(color);
        parts.push((handle.clone(), color, false));
        MeshMaterial2d(handle)
    };

    let shape = |commands: &mut Commands,
                 mesh: Mesh2d,
                 material: MeshMaterial2d<ColorMaterial>,
                 at: Vec3| {
        commands
            .spawn((mesh, material, Transform::from_translation(at)))
            .id()
    };

    let shadow = shape(
        commands,
        Mesh2d(meshes.add(Ellipse::new(h * 0.3, h * 0.08))),
        MeshMaterial2d(materials.add(SHADOW)),
        Vec3::new(0.0, 0.0, 0.0),
    );
    let ring_mat = materials.add(Color::NONE);
    let ring = commands
        .spawn((
            Ring(id),
            Mesh2d(meshes.add(Ellipse::new(h * 0.4, h * 0.11))),
            MeshMaterial2d(ring_mat),
            Transform::from_xyz(0.0, 0.0, 0.01),
        ))
        .id();

    // Риг: ноги, корпус, голова, оружие (показывает, куда смотрит боец) и буква имени.
    let legs = shape(
        commands,
        Mesh2d(meshes.add(Rectangle::new(h * 0.22, h * 0.32))),
        mat(materials, LEGS),
        Vec3::new(0.0, h * 0.16, 0.0),
    );
    let weapon = commands
        .spawn((
            Mesh2d(meshes.add(Rectangle::new(h * 0.45, h * 0.06))),
            mat(materials, WEAPON),
            Transform::from_xyz(h * 0.28, h * 0.5, 0.01).with_rotation(Quat::from_rotation_z(0.35)),
        ))
        .id();
    let body = shape(
        commands,
        Mesh2d(meshes.add(Capsule2d::new(h * 0.17, h * 0.26))),
        mat(materials, body_color),
        Vec3::new(0.0, h * 0.5, 0.02),
    );
    let head = shape(
        commands,
        Mesh2d(meshes.add(Circle::new(h * 0.13))),
        mat(materials, SKIN),
        Vec3::new(h * 0.03, h * 0.8, 0.03),
    );
    let initial: String = unit.def.name.chars().take(1).collect();
    let letter = commands
        .spawn((
            Text2d::new(initial),
            fonts.text(true, h * 0.2),
            TextColor(Color::srgba(0.05, 0.07, 0.12, 0.85)),
            // Риг отражается для врагов; букву отражаем обратно, чтобы читалась.
            Transform::from_xyz(0.0, h * 0.5, 0.04).with_scale(Vec3::new(facing, 1.0, 1.0)),
        ))
        .id();
    let art = screen.field.animated.get(&unit.def.id).cloned();
    let mut flipbook = None;
    let rig_children = if let Some(art) = &art {
        // Спрайт-лист вместо заглушки: ноги кадра — на слоте.
        for part in [legs, weapon, body, head, letter] {
            commands.entity(part).despawn();
        }
        parts.clear();
        let size = Vec2::new(art.frame[0] / art.frame[1], 1.0) * art.height * h;
        let material = materials.add(ColorMaterial {
            texture: Some(assets.load(art.sheet.clone())),
            uv_transform: art.uv(0, 0),
            ..default()
        });
        parts.push((material.clone(), Color::WHITE, true));
        flipbook = Some(Flipbook {
            material: material.clone(),
            art: art.clone(),
        });
        let sprite = commands
            .spawn((
                Mesh2d(meshes.add(Rectangle::from_size(size))),
                MeshMaterial2d(material),
                Transform::from_xyz(0.0, size.y * (0.5 - art.foot), 0.0),
            ))
            .id();
        vec![sprite]
    } else {
        vec![legs, weapon, body, head, letter]
    };
    let rig = commands
        .spawn((
            Rig,
            Transform::from_xyz(0.0, 0.0, 0.02),
            Visibility::default(),
        ))
        .add_children(&rig_children)
        .id();

    // Над головой: имя, здоровье, у героев — энергия. Не качается вместе с ригом.
    let top = art
        .as_ref()
        .map_or(h, |art| (art.top - art.foot) * art.height * h)
        + 12.0;
    let bar =
        |commands: &mut Commands, y: f32, height: f32, color: Color, marker: Option<Entity>| {
            let back = commands
                .spawn((
                    Sprite {
                        color: BAR_BACK,
                        custom_size: Some(Vec2::new(BAR_WIDTH + 2.0, height + 2.0)),
                        ..default()
                    },
                    Transform::from_xyz(0.0, y, 0.0),
                ))
                .id();
            let fill = marker.unwrap_or_else(|| commands.spawn_empty().id());
            commands.entity(fill).insert((
                Sprite {
                    color,
                    custom_size: Some(Vec2::new(BAR_WIDTH, height)),
                    ..default()
                },
                Anchor::CENTER_LEFT,
                Transform::from_xyz(-BAR_WIDTH / 2.0, y, 0.01),
            ));
            [back, fill]
        };
    let hp_fill = commands.spawn(HpBar(id)).id();
    let mut overhead_children = Vec::from(bar(
        commands,
        top,
        7.0,
        if ally { HP_ALLY } else { HP_ENEMY },
        Some(hp_fill),
    ));
    if unit.def.ultimate.is_some() {
        let energy_fill = commands.spawn(EnergyBar(id)).id();
        overhead_children.extend(bar(commands, top - 9.0, 4.0, ENERGY, Some(energy_fill)));
    }
    let overhead = commands
        .spawn((
            Overhead(id),
            Transform::from_xyz(0.0, 0.0, 5.0),
            Visibility::default(),
        ))
        .add_children(&overhead_children)
        .id();

    let mut model = commands.spawn((
        UnitModel {
            id,
            home,
            height: h,
            facing,
            shot: screen.field.ranged.get(&unit.def.id).copied(),
        },
        ModelAnim {
            clip: Clip::Idle,
            timer: Timer::from_seconds(0.01, TimerMode::Once),
            flash: 0.0,
        },
        ModelParts { parts },
        // Ниже на экране — ближе к зрителю, значит выше по слою.
        Transform::from_xyz(home.x, home.y, Z_MODELS - home.y / 100.0),
        Visibility::default(),
    ));
    model.add_children(&[shadow, ring, rig, overhead]);
    if let Some(flipbook) = flipbook {
        model.insert(flipbook);
    }
    model.id()
}

fn ease_out(t: f32) -> f32 {
    1.0 - (1.0 - t).powi(3)
}

/// Анимации моделей: дыхание всегда, поверх — рывок, отброс, смерть, победа.
pub fn animate_models(
    time: Res<Time>,
    screen: Res<BattleScreen>,
    mut models: Query<(
        &UnitModel,
        &mut ModelAnim,
        &ModelParts,
        &Children,
        Option<&Flipbook>,
    )>,
    mut rigs: Query<&mut Transform, With<Rig>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    let look = &screen.look;
    let dt = if screen.paused {
        std::time::Duration::ZERO
    } else {
        time.delta().mul_f32(screen.speed as f32)
    };
    let now = time.elapsed_secs();
    for (model, mut anim, parts, children, flipbook) in &mut models {
        anim.timer.tick(dt);
        let t = anim.timer.fraction();
        let h = model.height;
        let phase = model.home.x * 0.01;
        let breath = (2.0 * PI * now / look.idle_period + phase).sin();
        let mut offset = Vec2::ZERO;
        let mut scale = Vec2::new(1.0, 1.0 + look.idle_amp * breath);
        let mut angle = 0.0;
        let mut fade = 1.0;
        let mut glow = 0.0;

        match anim.clip {
            Clip::Idle => {}
            Clip::Attack { to, big } => {
                let reach = if big {
                    look.ult_reach
                } else {
                    look.attack_reach
                };
                // Дальнобойный не бежит к цели: стреляет с места, отдача назад.
                let full = if model.shot.is_some() {
                    (model.home - to).normalize_or_zero() * h * 0.06
                } else {
                    (to - model.home) * reach
                };
                // Ульта: присед до 35% клипа, затем рывок; обычная атака — сразу рывок.
                let (crouch_end, dash_end, hold_end) = if big {
                    (0.35, 0.55, 0.7)
                } else {
                    (0.0, 0.35, 0.5)
                };
                if t < crouch_end {
                    let k = t / crouch_end;
                    scale = Vec2::new(1.0 + 0.12 * k, 1.0 - 0.2 * k);
                    glow = k;
                } else if t < dash_end {
                    let k = ease_out((t - crouch_end) / (dash_end - crouch_end));
                    offset = full * k;
                    scale = Vec2::new(1.1, 0.95);
                    glow = if big { 1.0 } else { 0.0 };
                } else if t < hold_end {
                    offset = full;
                    glow = if big {
                        1.0 - (t - dash_end) / (hold_end - dash_end)
                    } else {
                        0.0
                    };
                } else {
                    let k = (t - hold_end) / (1.0 - hold_end);
                    offset = full * (1.0 - k * k);
                }
            }
            Clip::Hit { dir } => {
                offset = dir * look.hit_knockback * (PI * t).sin();
                angle = -0.15 * model.facing * (PI * t).sin();
            }
            Clip::Death => {
                let k = ease_out(t);
                // У спрайт-листа падение уже нарисовано в кадрах — риг не кладём.
                if flipbook.is_none() {
                    angle = model.facing * FRAC_PI_2 * k;
                }
                offset = Vec2::new(-model.facing * h * 0.25 * k, 0.0);
                fade = 1.0 - 0.7 * k;
                scale = Vec2::ONE;
            }
            Clip::Victory => {
                offset.y = (now * 6.0 + phase).sin().abs() * h * 0.08;
            }
        }
        // Кадр спрайт-листа: зацикленные клипы — по часам, остальные — на время клипа рига
        // (смерть замирает на последнем кадре).
        if let Some(book) = flipbook
            && let Some(sheet) = book.art.clip(anim.clip)
        {
            let n = sheet.frames.max(1);
            let frame = match sheet.fps {
                Some(fps) => (now * fps + phase * 7.0) as u32 % n,
                None => ((t * n as f32) as u32).min(n - 1),
            };
            if let Some(mut material) = materials.get_mut(&book.material) {
                material.uv_transform = book.art.uv(sheet.row, frame);
            }
            // Тело уже «дышит» и шатается в кадрах: риг не сжимаем в покое и почти не наклоняем.
            if anim.clip == Clip::Idle {
                scale = Vec2::ONE;
            }
            angle *= 0.3;
        }
        if anim.timer.is_finished() && matches!(anim.clip, Clip::Attack { .. } | Clip::Hit { .. }) {
            anim.clip = Clip::Idle;
        }
        anim.flash = (anim.flash - dt.as_secs_f32() / look.flash_time.max(0.01)).max(0.0);

        for child in children.iter() {
            if let Ok(mut rig) = rigs.get_mut(child) {
                rig.translation = Vec3::new(offset.x, offset.y, rig.translation.z);
                rig.rotation = Quat::from_rotation_z(angle);
                // Риг смотрит в сторону противника.
                rig.scale = Vec3::new(scale.x * model.facing, scale.y, 1.0);
            }
        }
        for (handle, base, textured) in &parts.parts {
            if let Some(mut material) = materials.get_mut(handle) {
                let tinted = base.mix(&GOLD, 0.6 * glow);
                let lit = if *textured {
                    // Цвет умножается на текстуру: вспышку даёт множитель больше единицы.
                    let k = 1.0 + 2.5 * anim.flash;
                    let c = tinted.to_linear();
                    Color::linear_rgb(c.red * k, c.green * k, c.blue * k)
                } else {
                    tinted.mix(&Color::WHITE, 0.7 * anim.flash)
                };
                material.color = lit.with_alpha(base.alpha() * fade);
            }
        }
    }
}

/// Полоски над головой, кольцо под тем, чей ход, и золотое кольцо готовой ульты.
pub fn refresh_overhead(
    time: Res<Time>,
    screen: Res<BattleScreen>,
    mut hp: Query<(&HpBar, &mut Transform), Without<EnergyBar>>,
    mut energy: Query<(&EnergyBar, &mut Transform, &mut Sprite), Without<HpBar>>,
    mut overheads: Query<(&Overhead, &mut Visibility)>,
    rings: Query<(&Ring, &MeshMaterial2d<ColorMaterial>)>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    let battle = &screen.battle;
    for (HpBar(id), mut transform) in &mut hp {
        let max = battle.unit(*id).def.stats.max_hp.max(1);
        transform.scale.x = screen.shown_hp[id.0] as f32 / max as f32;
    }
    for (EnergyBar(id), mut transform, mut sprite) in &mut energy {
        let unit = battle.unit(*id);
        transform.scale.x = unit.energy as f32 / ENERGY_MAX as f32;
        sprite.color = if unit.ultimate_ready() { GOLD } else { ENERGY };
    }
    for (Overhead(id), mut vis) in &mut overheads {
        *vis = if screen.shown_hp[id.0] > 0 {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }
    let pulse = 0.5 + 0.5 * (time.elapsed_secs() * 6.0).sin();
    for (Ring(id), material) in &rings {
        let alive = screen.shown_hp[id.0] > 0 && screen.shown_outcome.is_none();
        let color = if !alive {
            Color::NONE
        } else if screen.shown_current == Some(*id) {
            Color::srgba(1.0, 1.0, 1.0, 0.75)
        } else if battle.unit(*id).ultimate_ready() {
            GOLD.with_alpha(0.35 + 0.4 * pulse)
        } else {
            Color::NONE
        };
        if let Some(mut m) = materials.get_mut(&material.0) {
            m.color = color;
        }
    }
}
