//! Эффекты боя: клипы моделей по событиям, всплывающие цифры над головами, баннер действия,
//! тряска камеры и затемнение неба на ульте и залпе. Размах и длительность — в
//! `data/presentation.json`.

use std::f32::consts::PI;

use battle_core::{AbilityKind, Event, Side};
use bevy::prelude::*;
use bevy::sprite::Text2dShadow;

use crate::battle_screen::{BattleScreen, Shown};
use crate::battlefield::{Clip, ModelAnim, SkyDim, UnitModel, Z_FLOATING};
use crate::view::{Banner, Fonts};

const DAMAGE: Color = Color::WHITE;
const WEAKNESS: Color = Color::srgb(1.0, 0.85, 0.30);
const BROADSIDE: Color = Color::srgb(1.0, 0.55, 0.20);
const HEAL: Color = Color::srgb(0.45, 1.0, 0.55);
const SHIFT: Color = Color::srgb(0.70, 0.85, 1.0);
const BANNER_ULT: Color = Color::srgb(1.0, 0.85, 0.35);
const BANNER_SKILL: Color = Color::srgb(0.85, 0.95, 1.0);
const BANNER_BASIC: Color = Color::srgba(1.0, 1.0, 1.0, 0.85);
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

#[derive(Resource, Default)]
pub struct BannerFx {
    timer: Option<Timer>,
    color: Color,
}

/// Тряска камеры и затемнение неба — для ульт и залпа.
#[derive(Resource, Default)]
pub struct ScreenFx {
    shake: Option<(Timer, f32)>,
    dim: Option<(Timer, f32)>,
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
    mut banner: Query<(&mut Text, &mut TextFont), With<Banner>>,
    mut banner_fx: ResMut<BannerFx>,
    mut screen_fx: ResMut<ScreenFx>,
) {
    let look = &screen.look;
    // Где стоит модель и какого она роста.
    let place = |models: &Query<(&UnitModel, &mut ModelAnim)>, id| {
        models
            .iter()
            .find(|(m, _)| m.id == id)
            .map(|(m, _)| (m.home, m.height))
    };
    let float =
        |commands: &mut Commands, at: Vec2, height: f32, label: String, color: Color, size: f32| {
            let from = Vec3::new(at.x, at.y + height * 0.95, Z_FLOATING);
            commands.spawn((
                Floating {
                    timer: once(look.float_time),
                    from,
                    rise: look.float_px,
                    pop: look.pop_scale,
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
                Transform::from_translation(from).with_scale(Vec3::splat(look.pop_scale)),
            ));
        };
    let mut set_banner = |label: String, color: Color, size: f32, seconds: f32| {
        for (mut text, mut font) in &mut banner {
            text.0 = label.clone();
            *font = fonts.text(true, size);
        }
        banner_fx.timer = Some(once(seconds));
        banner_fx.color = color;
    };

    for Shown { event, focus } in shown.read() {
        match event {
            Event::AbilityUsed { unit, kind, name } => {
                let actor = screen.battle.unit(*unit);
                let who = &actor.def.name;
                let big = *kind == AbilityKind::Ultimate;
                match kind {
                    AbilityKind::Ultimate => {
                        set_banner(
                            format!("{who}\n«{name}»"),
                            BANNER_ULT,
                            34.0,
                            look.banner_ult,
                        );
                        screen_fx.dim = Some((once(look.dim_time), look.dim_alpha));
                        screen_fx.shake =
                            Some((once(look.screen_shake_time), look.screen_shake_px));
                    }
                    AbilityKind::Skill => set_banner(
                        format!("{who}: «{name}»"),
                        BANNER_SKILL,
                        22.0,
                        look.banner_skill,
                    ),
                    AbilityKind::Basic => set_banner(
                        format!("{who}: {name}"),
                        BANNER_BASIC,
                        17.0,
                        look.banner_basic,
                    ),
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
                if let Some((_, mut anim)) = models.iter_mut().find(|(m, _)| m.id == *unit) {
                    anim.play(Clip::Attack { to, big }, seconds);
                }
            }
            Event::BroadsideFired => {
                set_banner("ЗАЛП КОРАБЛЯ!".into(), BROADSIDE, 34.0, look.banner_ult);
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
                let label = match (crit, weakness) {
                    (true, _) => format!("-{amount}!"),
                    (_, true) => format!("-{amount}\nслабость"),
                    _ => format!("-{amount}"),
                };
                float(&mut commands, at, height, label, color, size);
            }
            Event::Healed { target, amount, .. } => {
                if let Some((at, height)) = place(&models, *target) {
                    float(
                        &mut commands,
                        at,
                        height,
                        format!("+{amount}"),
                        HEAL,
                        DAMAGE_SIZE * 0.9,
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
                    float(&mut commands, at, height * 0.7, label.into(), SHIFT, 16.0);
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
fn fx_delta(time: &Time, screen: &BattleScreen) -> std::time::Duration {
    if screen.paused {
        std::time::Duration::ZERO
    } else {
        time.delta().mul_f32(screen.speed as f32)
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

pub fn animate_banner(
    time: Res<Time>,
    screen: Res<BattleScreen>,
    mut fx: ResMut<BannerFx>,
    mut banner: Query<&mut TextColor, With<Banner>>,
) {
    let dt = fx_delta(&time, &screen);
    let color = fx.color;
    let alpha = match &mut fx.timer {
        Some(timer) => {
            timer.tick(dt);
            // Держим баннер, затем быстро гасим.
            let t = timer.fraction();
            if t < 0.7 { 1.0 } else { 1.0 - (t - 0.7) / 0.3 }
        }
        None => 0.0,
    };
    for mut text in &mut banner {
        text.0 = color.with_alpha(alpha * color.alpha());
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
