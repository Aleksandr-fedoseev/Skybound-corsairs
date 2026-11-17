//! Эффекты боя: выпад к цели, тряска и вспышка при ударе, всплывающие цифры, баннер умения.
//! Размах и длительность — в `data/presentation.json`.

use std::f32::consts::PI;

use battle_core::{AbilityKind, Event, Side};
use bevy::prelude::*;

use crate::battle_screen::{BattleScreen, Shown};
use crate::view::{Banner, Fonts, UnitCard};

const DAMAGE: Color = Color::WHITE;
const WEAKNESS: Color = Color::srgb(1.0, 0.85, 0.30);
const BROADSIDE: Color = Color::srgb(1.0, 0.55, 0.20);
const HEAL: Color = Color::srgb(0.45, 1.0, 0.55);
const SHIFT: Color = Color::srgb(0.70, 0.85, 1.0);
const BANNER_ULT: Color = Color::srgb(1.0, 0.85, 0.35);
const BANNER_SKILL: Color = Color::srgb(0.85, 0.95, 1.0);
const BANNER_BASIC: Color = Color::srgba(1.0, 1.0, 1.0, 0.75);
const FLASH_STRENGTH: f32 = 0.55;
const SHAKE_WAVES: f32 = 5.0;
/// Откуда всплывает цифра: середина карточки, поверх портрета.
const FLOAT_START: f32 = 30.0;

/// Движение карточки: выпад при своём действии, тряска и вспышка при попадании.
#[derive(Component, Default)]
pub struct CardMotion {
    lunge: Option<(Timer, f32)>,
    shake: Option<(Timer, f32)>,
    flash: Option<Timer>,
}

#[derive(Component)]
pub struct Floating {
    timer: Timer,
    rise: f32,
    color: Color,
}

#[derive(Resource, Default)]
pub struct BannerFx {
    timer: Option<Timer>,
    color: Color,
}

fn once(seconds: f32) -> Timer {
    Timer::from_seconds(seconds.max(0.01), TimerMode::Once)
}

/// Запускает эффекты по событиям, которые экран только что показал.
pub fn spawn_effects(
    mut commands: Commands,
    mut shown: MessageReader<Shown>,
    screen: Res<BattleScreen>,
    fonts: Res<Fonts>,
    mut cards: Query<(Entity, &UnitCard, &mut CardMotion)>,
    mut banner: Query<&mut Text, With<Banner>>,
    mut banner_fx: ResMut<BannerFx>,
) {
    let look = &screen.look;
    let float = |commands: &mut Commands, card: Entity, label: String, color: Color, size: f32| {
        let text = commands
            .spawn((
                Floating {
                    timer: once(look.float_time),
                    rise: look.float_px,
                    color,
                },
                Text::new(label),
                fonts.text(true, size),
                TextColor(color),
                TextShadow::default(),
                TextLayout::justify(Justify::Center),
                Node {
                    position_type: PositionType::Absolute,
                    top: px(FLOAT_START),
                    width: percent(100),
                    ..default()
                },
                GlobalZIndex(5),
            ))
            .id();
        commands.entity(card).add_child(text);
    };
    let mut set_banner = |label: String, color: Color| {
        for mut text in &mut banner {
            text.0 = label.clone();
        }
        banner_fx.timer = Some(once(look.banner_time));
        banner_fx.color = color;
    };

    for Shown(event) in shown.read() {
        match event {
            Event::AbilityUsed { unit, kind, name } => {
                let (label, color) = match kind {
                    AbilityKind::Ultimate => (
                        format!("{}\n«{name}»", screen.battle.unit(*unit).def.name),
                        BANNER_ULT,
                    ),
                    AbilityKind::Skill => (format!("«{name}»"), BANNER_SKILL),
                    AbilityKind::Basic => (name.clone(), BANNER_BASIC),
                };
                set_banner(label, color);
                // Союзники делают выпад вверх, к врагам; враги — вниз.
                let dir = if screen.battle.unit(*unit).side == Side::Ally {
                    -1.0
                } else {
                    1.0
                };
                if let Some((_, _, mut motion)) = cards.iter_mut().find(|(_, c, _)| c.0 == *unit) {
                    motion.lunge = Some((once(look.lunge_time), dir * look.lunge_px));
                }
            }
            Event::BroadsideFired => set_banner("ЗАЛП КОРАБЛЯ!".into(), BROADSIDE),
            Event::Damage {
                source,
                target,
                amount,
                weakness,
                crit,
            } => {
                if let Some((card, _, mut motion)) =
                    cards.iter_mut().find(|(_, c, _)| c.0 == *target)
                {
                    motion.shake = Some((
                        once(look.shake_time),
                        look.shake_px * if *crit { 1.6 } else { 1.0 },
                    ));
                    motion.flash = Some(once(look.flash_time));
                    let color = match (source, weakness) {
                        (None, _) => BROADSIDE,
                        (_, true) => WEAKNESS,
                        _ => DAMAGE,
                    };
                    let size = if *crit { 24.0 * look.crit_scale } else { 24.0 };
                    let label = if *crit {
                        format!("-{amount}!")
                    } else {
                        format!("-{amount}")
                    };
                    float(&mut commands, card, label, color, size);
                }
            }
            Event::Healed { target, amount, .. } => {
                if let Some((card, _, _)) = cards.iter().find(|(_, c, _)| c.0 == *target) {
                    float(&mut commands, card, format!("+{amount}"), HEAL, 22.0);
                }
            }
            Event::Advanced { unit, .. } | Event::Delayed { unit, .. } => {
                let label = if matches!(event, Event::Advanced { .. }) {
                    "ускорен"
                } else {
                    "отброшен"
                };
                if let Some((card, _, _)) = cards.iter().find(|(_, c, _)| c.0 == *unit) {
                    float(&mut commands, card, label.into(), SHIFT, 15.0);
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

pub fn animate_cards(
    time: Res<Time>,
    screen: Res<BattleScreen>,
    mut cards: Query<(&mut CardMotion, &mut UiTransform, &mut BackgroundColor)>,
) {
    let dt = fx_delta(&time, &screen);
    for (mut motion, mut transform, mut bg) in &mut cards {
        let mut offset = Vec2::ZERO;
        if let Some((timer, size)) = &mut motion.lunge {
            timer.tick(dt);
            offset.y += *size * (PI * timer.fraction()).sin();
            if timer.is_finished() {
                motion.lunge = None;
            }
        }
        if let Some((timer, size)) = &mut motion.shake {
            timer.tick(dt);
            let t = timer.fraction();
            offset.x += *size * (1.0 - t) * (t * SHAKE_WAVES * 2.0 * PI).sin();
            if timer.is_finished() {
                motion.shake = None;
            }
        }
        if let Some(timer) = &mut motion.flash {
            timer.tick(dt);
            let k = FLASH_STRENGTH * (1.0 - timer.fraction());
            bg.0 = bg.0.mix(&Color::WHITE, k);
            if timer.is_finished() {
                motion.flash = None;
            }
        }
        transform.translation = Val2::px(offset.x, offset.y);
    }
}

pub fn animate_floating(
    mut commands: Commands,
    time: Res<Time>,
    screen: Res<BattleScreen>,
    mut texts: Query<(Entity, &mut Floating, &mut Node, &mut TextColor)>,
) {
    let dt = fx_delta(&time, &screen);
    for (entity, mut float, mut node, mut color) in &mut texts {
        float.timer.tick(dt);
        let t = float.timer.fraction();
        node.top = px(FLOAT_START - float.rise * t);
        color.0 = float.color.with_alpha(1.0 - t * t);
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
            if t < 0.6 { 1.0 } else { 1.0 - (t - 0.6) / 0.4 }
        }
        None => 0.0,
    };
    for mut text in &mut banner {
        text.0 = color.with_alpha(alpha);
    }
}
