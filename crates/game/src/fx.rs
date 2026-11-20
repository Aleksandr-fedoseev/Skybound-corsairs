//! Эффекты боя: клипы моделей по событиям, всплывающие цифры и названия умений над головами,
//! полоса через экран, тряска камеры и затемнение неба на ульте и залпе. Размах и длительность — в
//! `data/presentation.json`.

use std::f32::consts::PI;

use battle_core::{AbilityKind, Event, Side};
use bevy::prelude::*;
use bevy::sprite::Text2dShadow;

use crate::battle_screen::{BattleScreen, Shown};
use crate::battlefield::{Clip, ModelAnim, Shot, SkyDim, UnitModel, Z_FLOATING};
use crate::view::{Fonts, UltBand, UltBandText};

const DAMAGE: Color = Color::WHITE;
const WEAKNESS: Color = Color::srgb(1.0, 0.85, 0.30);
const BROADSIDE: Color = Color::srgb(1.0, 0.55, 0.20);
const HEAL: Color = Color::srgb(0.45, 1.0, 0.55);
const SHIFT: Color = Color::srgb(0.70, 0.85, 1.0);
const BANNER_ULT: Color = Color::srgb(1.0, 0.85, 0.35);
const SKILL_LABEL: Color = Color::srgb(0.80, 0.93, 1.0);
const BAND: Color = Color::srgba(0.03, 0.05, 0.12, 0.7);
/// Цифры подряд по одной цели разъезжаются в стороны, чтобы не слипаться.
const DAMAGE_JITTER: [f32; 4] = [0.0, 22.0, -22.0, 11.0];
/// Полоса ульты выезжает слева на столько пикселей.
const BAND_SLIDE: f32 = 80.0;
/// Доля клипа ближнего боя, к которой боец добежал до цели (рывок кончается на 0.35, держится до
/// 0.5): удар своего эффекта (spells.rs) приходится сюда.
pub const MELEE_ARRIVE: f32 = 0.42;
const DAMAGE_SIZE: f32 = 28.0;
/// Победная анимация идёт, пока игрок не начнёт новый бой.
const VICTORY_SECONDS: f32 = 3600.0;
/// Если цель не найдена (например, лечение себя), рывок — к центру поля.
const FIELD_CENTER: Vec2 = Vec2::new(0.0, -20.0);

#[derive(Component)]
pub struct Floating {
    timer: Timer,
    from: Vec3,
    rise: f32,
    pop: f32,
    color: Color,
}

/// Снаряд дальнобойного бойца: летит от руки к цели за время замаха, к удару долетает.
#[derive(Component)]
pub struct Projectile {
    timer: Timer,
    from: Vec2,
    to: Vec2,
    /// Высота дуги в пикселях (склянка летит навесом).
    arc: f32,
    /// Вращение в оборотах за полёт (склянка кувыркается).
    spin: f32,
}

/// Короткая вспышка у ствола: гаснет и растёт.
#[derive(Component)]
pub struct Flash {
    timer: Timer,
    color: Color,
}

#[derive(Resource, Default)]
pub struct BandFx {
    timer: Option<Timer>,
    color: Color,
}

/// Тряска камеры и затемнение неба — для ульт и залпа.
#[derive(Resource, Default)]
pub struct ScreenFx {
    shake: Option<(Timer, f32)>,
    dim: Option<(Timer, f32)>,
}

impl ScreenFx {
    /// Тряхнуть камеру: на столько секунд, на столько пикселей.
    pub fn shake(&mut self, seconds: f32, px: f32) {
        self.shake = Some((once(seconds), px));
    }
}

fn once(seconds: f32) -> Timer {
    Timer::from_seconds(seconds.max(0.01), TimerMode::Once)
}

/// Запускает клипы моделей и эффекты по событиям, которые экран только что показал.
#[allow(clippy::too_many_arguments)]
pub fn spawn_effects(
    mut commands: Commands,
    mut shown: MessageReader<Shown>,
    screen: Res<BattleScreen>,
    fonts: Res<Fonts>,
    mut models: Query<(&UnitModel, &mut ModelAnim)>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    mut band: Query<&mut Text, With<UltBandText>>,
    mut band_fx: ResMut<BandFx>,
    mut screen_fx: ResMut<ScreenFx>,
    mut hits: Local<usize>,
) {
    let look = &screen.look;
    // Где стоит модель и какого она роста.
    let place = |models: &Query<(&UnitModel, &mut ModelAnim)>, id| {
        models
            .iter()
            .find(|(m, _)| m.id == id)
            .map(|(m, _)| (m.home, m.height))
    };
    // Надпись над головой: цифра урона, лечение или название умения.
    let float = |commands: &mut Commands,
                 from: Vec3,
                 label: String,
                 color: Color,
                 size: f32,
                 seconds: f32,
                 rise: f32,
                 pop: f32| {
        commands.spawn((
            Floating {
                timer: once(seconds),
                from,
                rise,
                pop,
                color,
            },
            Text2d::new(label),
            fonts.text(true, size),
            TextColor(color),
            Text2dShadow {
                offset: Vec2::new(2.0, -2.0),
                color: Color::srgba(0.0, 0.0, 0.0, 0.85),
            },
            TextLayout::justify(Justify::Center),
            Transform::from_translation(from).with_scale(Vec3::splat(pop)),
        ));
    };
    let head =
        |at: Vec2, height: f32, dx: f32| Vec3::new(at.x + dx, at.y + height * 0.95, Z_FLOATING);
    let mut set_band = |label: String, color: Color, seconds: f32| {
        for mut text in &mut band {
            text.0 = label.clone();
        }
        band_fx.timer = Some(once(seconds));
        band_fx.color = color;
    };

    for Shown { event, focus } in shown.read() {
        match event {
            Event::AbilityUsed { unit, kind, name } => {
                let actor = screen.battle.unit(*unit);
                let who = &actor.def.name;
                let big = *kind == AbilityKind::Ultimate;
                match kind {
                    // Ульта — событие: полоса через экран, тряска, затемнение неба.
                    AbilityKind::Ultimate => {
                        set_band(format!("{who} — «{name}»"), BANNER_ULT, look.banner_ult);
                        screen_fx.dim = Some((once(look.dim_time), look.dim_alpha));
                        screen_fx.shake =
                            Some((once(look.screen_shake_time), look.screen_shake_px));
                    }
                    // Умение — название над головой бойца; обычная атака — без надписи.
                    AbilityKind::Skill => {
                        if let Some((at, height)) = place(&models, *unit) {
                            let from = Vec3::new(at.x, at.y + height + 34.0, Z_FLOATING);
                            float(
                                &mut commands,
                                from,
                                format!("«{name}»"),
                                SKILL_LABEL,
                                17.0,
                                look.banner_skill,
                                16.0,
                                1.0,
                            );
                        }
                    }
                    AbilityKind::Basic => {}
                }
                // Рывок к первой цели; к своим (лечение) — короче, к центру поля — если цели нет.
                let to = focus
                    .and_then(|t| place(&models, t))
                    .map(|(at, _)| at)
                    .unwrap_or(FIELD_CENTER);
                let seconds = match kind {
                    AbilityKind::Basic => look.attack_time,
                    AbilityKind::Skill => look.attack_time * 1.3,
                    AbilityKind::Ultimate => look.ult_time,
                };
                // Дальнобойный: снаряд от руки к цели за время замаха (урон покажется к прилёту).
                let shooter = models
                    .iter()
                    .find(|(m, _)| m.id == *unit)
                    .and_then(|(m, _)| m.shot.map(|shot| (shot, m.home, m.height, m.facing)));
                let mut seconds = seconds;
                // Ближний бой со своим эффектом (spells.rs): клип растягивается на замах — к удару
                // боец уже у цели и стоит там, пока эффект бьёт.
                if shooter.is_none() && screen.field.spells.get(&actor.def.id, *kind).is_some() {
                    let arrive = if big { 0.62 } else { MELEE_ARRIVE };
                    seconds = seconds.max(screen.windup(*unit, *kind) / arrive);
                }
                if let Some((shot, home, height, facing)) = shooter
                    && let Some((target, target_h)) = focus.and_then(|t| place(&models, t))
                {
                    let flight = screen.windup(*unit, *kind).max(0.1);
                    let hand = home + Vec2::new(facing * height * 0.28, height * 0.55);
                    let aim = target + Vec2::new(0.0, target_h * 0.5);
                    // У умения свой эффект (spells.rs) — общий снаряд не нужен.
                    let own_fx = screen.field.spells.get(&actor.def.id, *kind).is_some();
                    if !own_fx {
                        spawn_shot(
                            &mut commands,
                            &mut meshes,
                            &mut materials,
                            shot,
                            hand,
                            aim,
                            flight,
                        );
                    }
                    // Выстрел виден дольше рывка: клип спрайта не сжимается в мелькание.
                    seconds = seconds.max(flight + 0.35);
                }
                if let Some((_, mut anim)) = models.iter_mut().find(|(m, _)| m.id == *unit) {
                    anim.play(
                        Clip::Attack {
                            to,
                            big,
                            skill: *kind == AbilityKind::Skill,
                        },
                        seconds,
                    );
                }
            }
            Event::BroadsideFired => {
                set_band("ЗАЛП КОРАБЛЯ!".into(), BROADSIDE, look.banner_ult);
                screen_fx.shake = Some((once(look.screen_shake_time), look.screen_shake_px * 1.3));
            }
            Event::Damage {
                source,
                target,
                amount,
                weakness,
                crit,
            } => {
                let Some((at, height)) = place(&models, *target) else {
                    continue;
                };
                // Отброс — от атакующего; залп корабля бьёт «с нашей стороны».
                let from = source
                    .and_then(|s| place(&models, s))
                    .map(|(p, _)| p)
                    .unwrap_or(Vec2::new(-270.0, -480.0));
                let dir = (at - from).normalize_or_zero();
                if let Some((_, mut anim)) = models.iter_mut().find(|(m, _)| m.id == *target) {
                    anim.play(
                        Clip::Hit { dir },
                        look.hit_time * if *crit || *weakness { 1.4 } else { 1.0 },
                    );
                    anim.flash = 1.0;
                }
                let color = match (source, weakness) {
                    (None, _) => BROADSIDE,
                    (_, true) => WEAKNESS,
                    _ => DAMAGE,
                };
                let size = if *crit {
                    DAMAGE_SIZE * look.crit_scale
                } else {
                    DAMAGE_SIZE
                };
                // Слабость — золотая цифра, крит — крупная с «!».
                let label = if *crit {
                    format!("-{amount}!")
                } else {
                    format!("-{amount}")
                };
                *hits += 1;
                let dx = DAMAGE_JITTER[*hits % DAMAGE_JITTER.len()];
                float(
                    &mut commands,
                    head(at, height, dx),
                    label,
                    color,
                    size,
                    look.float_time,
                    look.float_px,
                    look.pop_scale,
                );
            }
            Event::Healed { target, amount, .. } => {
                if let Some((at, height)) = place(&models, *target) {
                    let from = head(at, height, 0.0);
                    float(
                        &mut commands,
                        from,
                        format!("+{amount}"),
                        HEAL,
                        DAMAGE_SIZE * 0.9,
                        look.float_time,
                        look.float_px,
                        look.pop_scale,
                    );
                }
            }
            Event::Advanced { unit, .. } | Event::Delayed { unit, .. } => {
                let label = if matches!(event, Event::Advanced { .. }) {
                    "ускорен"
                } else {
                    "отброшен"
                };
                if let Some((at, height)) = place(&models, *unit) {
                    let from = head(at, height * 0.7, 0.0);
                    float(
                        &mut commands,
                        from,
                        label.into(),
                        SHIFT,
                        16.0,
                        look.float_time,
                        look.float_px * 0.5,
                        1.0,
                    );
                }
            }
            Event::Defeated { unit } => {
                if let Some((_, mut anim)) = models.iter_mut().find(|(m, _)| m.id == *unit) {
                    anim.play(Clip::Death, look.death_time);
                }
            }
            Event::BattleEnded { outcome } => {
                // Победители радуются.
                let winners = if *outcome == battle_core::Outcome::Victory {
                    Side::Ally
                } else {
                    Side::Enemy
                };
                for (model, mut anim) in &mut models {
                    if screen.battle.unit(model.id).side == winners
                        && screen.shown_hp[model.id.0] > 0
                    {
                        anim.play(Clip::Victory, VICTORY_SECONDS);
                    }
                }
            }
            _ => {}
        }
    }
}

/// Время эффектов идёт вместе со скоростью боя и стоит на паузе.
pub fn fx_delta(time: &Time, screen: &BattleScreen) -> std::time::Duration {
    if screen.paused {
        std::time::Duration::ZERO
    } else {
        time.delta().mul_f32(screen.speed as f32)
    }
}

/// Снаряд и вспышка у ствола. Простые формы — это эффекты, а не персонажи.
fn spawn_shot(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<ColorMaterial>,
    shot: Shot,
    from: Vec2,
    to: Vec2,
    flight: f32,
) {
    let (mesh, color, arc, spin, flash): (Mesh, Color, f32, f32, Option<Color>) = match shot {
        Shot::Bullet => (
            Capsule2d::new(2.5, 16.0).into(),
            Color::srgb(1.0, 0.92, 0.55),
            0.0,
            0.0,
            Some(Color::srgb(1.0, 0.8, 0.3)),
        ),
        Shot::Flask => (
            Circle::new(7.0).into(),
            Color::srgb(0.45, 1.0, 0.5),
            60.0,
            1.5,
            None,
        ),
        Shot::Wind => (
            Ellipse::new(16.0, 5.0).into(),
            Color::srgba(0.8, 1.0, 0.95, 0.85),
            10.0,
            0.0,
            None,
        ),
        Shot::Spark => (
            Capsule2d::new(2.0, 20.0).into(),
            Color::srgb(0.6, 0.9, 1.0),
            0.0,
            0.0,
            Some(Color::srgb(0.6, 0.9, 1.0)),
        ),
    };
    commands.spawn((
        Projectile {
            timer: once(flight),
            from,
            to,
            arc,
            spin,
        },
        Mesh2d(meshes.add(mesh)),
        MeshMaterial2d(materials.add(color)),
        Transform::from_xyz(from.x, from.y, Z_FLOATING - 1.0),
    ));
    if let Some(color) = flash {
        commands.spawn((
            Flash {
                timer: once(0.12),
                color,
            },
            Mesh2d(meshes.add(Circle::new(9.0))),
            MeshMaterial2d(materials.add(color)),
            Transform::from_xyz(from.x, from.y, Z_FLOATING - 1.0),
        ));
    }
}

/// Полёт снарядов и угасание вспышек.
pub fn animate_projectiles(
    mut commands: Commands,
    time: Res<Time>,
    screen: Res<BattleScreen>,
    mut shots: Query<(Entity, &mut Projectile, &mut Transform), Without<Flash>>,
    mut flashes: Query<(
        Entity,
        &mut Flash,
        &mut Transform,
        &MeshMaterial2d<ColorMaterial>,
    )>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    let dt = fx_delta(&time, &screen);
    for (entity, mut shot, mut transform) in &mut shots {
        shot.timer.tick(dt);
        let t = shot.timer.fraction();
        let at = shot.from.lerp(shot.to, t) + Vec2::Y * shot.arc * 4.0 * t * (1.0 - t);
        // Направление полёта с учётом дуги — пуля смотрит носом вперёд.
        let ahead = shot.from.lerp(shot.to, (t + 0.02).min(1.0))
            + Vec2::Y * shot.arc * 4.0 * (t + 0.02) * (1.0 - t - 0.02);
        let dir = (ahead - at).normalize_or(shot.to - shot.from);
        let angle = if shot.spin != 0.0 {
            t * shot.spin * 2.0 * PI
        } else {
            dir.y.atan2(dir.x) - PI / 2.0
        };
        transform.translation.x = at.x;
        transform.translation.y = at.y;
        transform.rotation = Quat::from_rotation_z(angle);
        if shot.timer.is_finished() {
            commands.entity(entity).despawn();
        }
    }
    for (entity, mut flash, mut transform, material) in &mut flashes {
        flash.timer.tick(dt);
        let t = flash.timer.fraction();
        transform.scale = Vec3::splat(1.0 + t);
        if let Some(mut m) = materials.get_mut(&material.0) {
            m.color = flash.color.with_alpha(1.0 - t);
        }
        if flash.timer.is_finished() {
            commands.entity(entity).despawn();
        }
    }
}

pub fn animate_floating(
    mut commands: Commands,
    time: Res<Time>,
    screen: Res<BattleScreen>,
    mut texts: Query<(Entity, &mut Floating, &mut Transform, &mut TextColor)>,
) {
    let dt = fx_delta(&time, &screen);
    for (entity, mut float, mut transform, mut color) in &mut texts {
        float.timer.tick(dt);
        let t = float.timer.fraction();
        // Цифра «выстреливает» крупной и быстро садится в обычный размер, затем всплывает.
        let pop = (t / 0.15).min(1.0);
        transform.scale = Vec3::splat(float.pop + (1.0 - float.pop) * pop);
        transform.translation = float.from + Vec3::Y * float.rise * t;
        color.0 = float.color.with_alpha(1.0 - t.powi(3));
        if float.timer.is_finished() {
            commands.entity(entity).despawn();
        }
    }
}

/// Полоса ульты: выезжает слева, держится и гаснет.
pub fn animate_band(
    time: Res<Time>,
    screen: Res<BattleScreen>,
    mut fx: ResMut<BandFx>,
    mut band: Query<(&mut BackgroundColor, &mut UiTransform), With<UltBand>>,
    mut text: Query<&mut TextColor, With<UltBandText>>,
) {
    let dt = fx_delta(&time, &screen);
    let color = fx.color;
    let (alpha, slide) = match &mut fx.timer {
        Some(timer) => {
            timer.tick(dt);
            let t = timer.fraction();
            let alpha = if t < 0.75 {
                1.0
            } else {
                1.0 - (t - 0.75) / 0.25
            };
            let slide = if t < 0.12 { 1.0 - t / 0.12 } else { 0.0 };
            (alpha, slide)
        }
        None => (0.0, 0.0),
    };
    for (mut bg, mut transform) in &mut band {
        bg.0 = BAND.with_alpha(BAND.alpha() * alpha);
        transform.translation = Val2::px(-BAND_SLIDE * slide, 0.0);
    }
    for mut text in &mut text {
        text.0 = color.with_alpha(alpha);
    }
}

pub fn animate_screen(
    time: Res<Time>,
    screen: Res<BattleScreen>,
    mut fx: ResMut<ScreenFx>,
    mut camera: Query<&mut Transform, With<Camera2d>>,
    mut dim: Query<&mut Sprite, With<SkyDim>>,
) {
    let dt = fx_delta(&time, &screen);
    let mut offset = Vec2::ZERO;
    if let Some((timer, size)) = &mut fx.shake {
        timer.tick(dt);
        let t = timer.fraction();
        let amp = *size * (1.0 - t);
        offset = Vec2::new(
            amp * (t * 7.0 * 2.0 * PI).sin(),
            amp * 0.6 * (t * 5.0 * 2.0 * PI).cos(),
        );
        if timer.is_finished() {
            fx.shake = None;
        }
    }
    let mut alpha = 0.0;
    if let Some((timer, strength)) = &mut fx.dim {
        timer.tick(dt);
        let t = timer.fraction();
        // Быстро темнеет, плавно светлеет.
        alpha = *strength
            * if t < 0.15 {
                t / 0.15
            } else {
                1.0 - (t - 0.15) / 0.85
            };
        if timer.is_finished() {
            fx.dim = None;
        }
    }
    for mut transform in &mut camera {
        transform.translation.x = offset.x;
        transform.translation.y = offset.y;
    }
    for mut sprite in &mut dim {
        sprite.color = Color::srgba(0.02, 0.04, 0.12, alpha);
    }
}
