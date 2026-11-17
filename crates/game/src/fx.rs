//! Эффекты боя: выпад к цели, тряска и вспышка при ударе, всплывающие цифры, баннер действия,
//! тряска экрана и затемнение неба на ульте и залпе. Размах и длительность — в
//! `data/presentation.json`.

use std::f32::consts::PI;

use battle_core::{AbilityKind, Event, Side};
use bevy::prelude::*;

use crate::battle_screen::{BattleScreen, Shown};
use crate::view::{Banner, BattleRoot, Fonts, SkyDim, UnitCard};

const DAMAGE: Color = Color::WHITE;
const WEAKNESS: Color = Color::srgb(1.0, 0.85, 0.30);
const BROADSIDE: Color = Color::srgb(1.0, 0.55, 0.20);
const HEAL: Color = Color::srgb(0.45, 1.0, 0.55);
const SHIFT: Color = Color::srgb(0.70, 0.85, 1.0);
const BANNER_ULT: Color = Color::srgb(1.0, 0.85, 0.35);
const BANNER_SKILL: Color = Color::srgb(0.85, 0.95, 1.0);
const BANNER_BASIC: Color = Color::srgba(1.0, 1.0, 1.0, 0.8);
const FLASH_STRENGTH: f32 = 0.6;
const SHAKE_WAVES: f32 = 5.0;
/// Откуда всплывает цифра: середина карточки, поверх портрета.
const FLOAT_START: f32 = 40.0;
const DAMAGE_SIZE: f32 = 30.0;
/// Чей ход — карточка чуть крупнее; готовая ульта — карточка пульсирует.
const CURRENT_SCALE: f32 = 1.07;
const READY_PULSE: f32 = 0.04;
const READY_PULSE_SPEED: f32 = 7.0;

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
    pop: f32,
    color: Color,
}

#[derive(Resource, Default)]
pub struct BannerFx {
    timer: Option<Timer>,
    color: Color,
}

/// Тряска всего экрана и затемнение неба — для ульт и залпа.
#[derive(Resource, Default)]
pub struct ScreenFx {
    shake: Option<(Timer, f32)>,
    dim: Option<(Timer, f32)>,
}

fn once(seconds: f32) -> Timer {
    Timer::from_seconds(seconds.max(0.01), TimerMode::Once)
}

/// Запускает эффекты по событиям, которые экран только что показал.
#[allow(clippy::too_many_arguments)]
pub fn spawn_effects(
    mut commands: Commands,
    mut shown: MessageReader<Shown>,
    screen: Res<BattleScreen>,
    fonts: Res<Fonts>,
    mut cards: Query<(Entity, &UnitCard, &mut CardMotion)>,
    mut banner: Query<(&mut Text, &mut TextFont), With<Banner>>,
    mut banner_fx: ResMut<BannerFx>,
    mut screen_fx: ResMut<ScreenFx>,
) {
    let look = &screen.look;
    let float = |commands: &mut Commands, card: Entity, label: String, color: Color, size: f32| {
        let text = commands
            .spawn((
                Floating {
                    timer: once(look.float_time),
                    rise: look.float_px,
                    pop: look.pop_scale,
                    color,
                },
                Text::new(label),
                fonts.text(true, size),
                TextColor(color),
                TextShadow {
                    offset: Vec2::new(2.0, 2.0),
                    color: Color::srgba(0.0, 0.0, 0.0, 0.85),
                },
                TextLayout::justify(Justify::Center),
                Node {
                    position_type: PositionType::Absolute,
                    top: px(FLOAT_START),
                    width: percent(100),
                    ..default()
                },
                UiTransform::from_scale(Vec2::splat(look.pop_scale)),
                GlobalZIndex(5),
                Pickable::IGNORE,
            ))
            .id();
        commands.entity(card).add_child(text);
    };
    let mut set_banner = |label: String, color: Color, size: f32, seconds: f32| {
        for (mut text, mut font) in &mut banner {
            text.0 = label.clone();
            *font = fonts.text(true, size);
        }
        banner_fx.timer = Some(once(seconds));
        banner_fx.color = color;
    };

    for Shown(event) in shown.read() {
        match event {
            Event::AbilityUsed { unit, kind, name } => {
                let actor = screen.battle.unit(*unit);
                let who = &actor.def.name;
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
                        24.0,
                        look.banner_skill,
                    ),
                    AbilityKind::Basic => set_banner(
                        format!("{who}: {name}"),
                        BANNER_BASIC,
                        18.0,
                        look.banner_basic,
                    ),
                }
                // Союзники делают выпад вверх, к врагам; враги — вниз.
                let dir = if actor.side == Side::Ally { -1.0 } else { 1.0 };
                let size = if *kind == AbilityKind::Ultimate {
                    1.5
                } else {
                    1.0
                };
                if let Some((_, _, mut motion)) = cards.iter_mut().find(|(_, c, _)| c.0 == *unit) {
                    motion.lunge = Some((once(look.lunge_time), dir * size * look.lunge_px));
                }
            }
            Event::BroadsideFired => {
                set_banner("ЗАЛП КОРАБЛЯ!".into(), BROADSIDE, 36.0, look.banner_ult);
                screen_fx.shake = Some((once(look.screen_shake_time), look.screen_shake_px * 1.3));
            }
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
                    let strong = *crit || *weakness;
                    motion.shake = Some((
                        once(look.shake_time),
                        look.shake_px * if strong { 1.7 } else { 1.0 },
                    ));
                    motion.flash = Some(once(look.flash_time));
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
                    float(&mut commands, card, label, color, size);
                }
            }
            Event::Healed { target, amount, .. } => {
                if let Some((card, _, _)) = cards.iter().find(|(_, c, _)| c.0 == *target) {
                    float(
                        &mut commands,
                        card,
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
                if let Some((card, _, _)) = cards.iter().find(|(_, c, _)| c.0 == *unit) {
                    float(&mut commands, card, label.into(), SHIFT, 18.0);
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
    mut cards: Query<(
        &UnitCard,
        &mut CardMotion,
        &mut UiTransform,
        &mut BackgroundColor,
    )>,
) {
    let dt = fx_delta(&time, &screen);
    let now = time.elapsed_secs();
    for (UnitCard(id), mut motion, mut transform, mut bg) in &mut cards {
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
        let alive = screen.shown_hp[id.0] > 0;
        let scale = if !alive || screen.shown_outcome.is_some() {
            1.0
        } else if screen.shown_current == Some(*id) {
            CURRENT_SCALE
        } else if screen.battle.unit(*id).ultimate_ready() {
            1.0 + READY_PULSE * (0.5 + 0.5 * (now * READY_PULSE_SPEED).sin())
        } else {
            1.0
        };
        transform.translation = Val2::px(offset.x, offset.y);
        transform.scale = Vec2::splat(scale);
    }
}

pub fn animate_floating(
    mut commands: Commands,
    time: Res<Time>,
    screen: Res<BattleScreen>,
    mut texts: Query<(
        Entity,
        &mut Floating,
        &mut Node,
        &mut TextColor,
        &mut UiTransform,
    )>,
) {
    let dt = fx_delta(&time, &screen);
    for (entity, mut float, mut node, mut color, mut transform) in &mut texts {
        float.timer.tick(dt);
        let t = float.timer.fraction();
        // Цифра «выстреливает» крупной и быстро садится в обычный размер, затем всплывает.
        let pop = (t / 0.15).min(1.0);
        transform.scale = Vec2::splat(float.pop + (1.0 - float.pop) * pop);
        node.top = px(FLOAT_START - float.rise * t);
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
    mut root: Query<&mut UiTransform, With<BattleRoot>>,
    mut dim: Query<&mut BackgroundColor, With<SkyDim>>,
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
    for mut transform in &mut root {
        transform.translation = Val2::px(offset.x, offset.y);
    }
    for mut bg in &mut dim {
        bg.0 = Color::srgba(0.02, 0.04, 0.12, alpha);
    }
}
