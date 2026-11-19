//! Состояние экрана боя: бой из battle-core, очередь событий и их проигрывание с паузами.
//!
//! Модель (`Battle`) считает ход сразу целиком, а экран показывает его события по одному.
//! Поэтому здоровье на экране — своя копия, которая меняется только по событиям.

use std::collections::VecDeque;
use std::time::{SystemTime, UNIX_EPOCH};

use battle_core::{AbilityKind, Battle, Event, Outcome, Ship, UnitId};
use bevy::prelude::*;
use game_data::GameData;

use crate::battlefield::FieldConfig;
use serde::Deserialize;

const SHIP: Ship = Ship {
    broadside_power: 120,
};
/// Не больше стольких событий за кадр, чтобы на x3 бой не «проскакивал».
const MAX_EVENTS_PER_FRAME: usize = 8;

/// Темп и размах анимаций боя. Лежит в `data/presentation.json`, время — в секундах на x1.
#[derive(Clone, Debug, Deserialize)]
pub struct Presentation {
    /// Пауза после каждого хода.
    pub step_pause: f32,
    /// Замах перед ударом: обычная атака короткая, ульта и залп — событие.
    pub basic_windup: f32,
    pub skill_windup: f32,
    pub ult_windup: f32,
    pub broadside_windup: f32,
    /// Пауза между попаданиями по нескольким целям.
    pub hit_gap: f32,
    /// Стоп-кадр после крита или удара по слабости.
    pub crit_hitstop: f32,
    pub defeat_pause: f32,
    /// Сколько висит название умения над головой и полоса ульты.
    pub banner_skill: f32,
    pub banner_ult: f32,
    /// Атака: рывок к цели на долю расстояния и назад.
    pub attack_time: f32,
    pub attack_reach: f32,
    /// Ульта: присед, затем рывок дальше обычного.
    pub ult_time: f32,
    pub ult_reach: f32,
    /// Попадание: отброс назад и вспышка.
    pub hit_time: f32,
    pub hit_knockback: f32,
    /// Смерть: падение и угасание.
    pub death_time: f32,
    /// Дыхание в ожидании: период (с) и размах (доля роста).
    pub idle_period: f32,
    pub idle_amp: f32,
    pub flash_time: f32,
    /// Тряска всего экрана на ульте и залпе.
    pub screen_shake_px: f32,
    pub screen_shake_time: f32,
    /// Затемнение неба на ульте.
    pub dim_alpha: f32,
    pub dim_time: f32,
    pub float_px: f32,
    pub float_time: f32,
    /// Во сколько раз цифра урона больше в момент появления.
    pub pop_scale: f32,
    pub crit_scale: f32,
}

impl Presentation {
    pub fn bundled() -> Self {
        serde_json::from_str(include_str!("../../../data/presentation.json"))
            .expect("data/presentation.json корректен")
    }
}

/// Что показать на экране: событие боя, которое уже пора проиграть.
#[derive(Message, Clone, Debug)]
pub struct Shown {
    pub event: Event,
    /// Для действия — первая цель, к которой боец делает рывок.
    pub focus: Option<UnitId>,
}

#[derive(Resource)]
pub struct BattleScreen {
    pub data: GameData,
    pub look: Presentation,
    /// Раскладки поля боя и текущая (в тестовой сцене переключается клавишей V).
    pub field: FieldConfig,
    pub layout: String,
    pub stage: usize,
    /// Отряд: герои и их уровни (собирается на экране «Экипаж»).
    pub team: Vec<(String, u32)>,
    /// Прибавка к уровню всех героев в тестовой сцене (клавиши [ и ]).
    pub gym_bonus: u32,
    pub seed: u64,
    pub battle: Battle,
    /// Здоровье, которое сейчас видит игрок.
    pub shown_hp: Vec<u32>,
    pub shown_current: Option<UnitId>,
    pub shown_outcome: Option<Outcome>,
    pub queue: VecDeque<Event>,
    pub wait: f32,
    pub speed: u32,
    pub paused: bool,
    /// АВТО: ульты нажимаются сами. Без него ульты — только по тапу игрока.
    pub auto: bool,
    /// Экран надо собрать заново (новый бой с другим составом).
    pub rebuild: bool,
    /// Сообщение для строки тестовой сцены (например, ошибка в data/).
    #[cfg_attr(not(feature = "dev"), allow(dead_code))]
    pub notice: Option<String>,
}

impl BattleScreen {
    pub fn new(
        data: GameData,
        look: Presentation,
        field: FieldConfig,
        team: Vec<(String, u32)>,
    ) -> Self {
        let seed = random_seed();
        let battle = new_battle(&data, 0, &team, 0, seed);
        let mut screen = Self {
            data,
            look,
            layout: field.default.clone(),
            field,
            stage: 0,
            team,
            gym_bonus: 0,
            seed,
            battle,
            shown_hp: Vec::new(),
            shown_current: None,
            shown_outcome: None,
            queue: VecDeque::new(),
            wait: 0.0,
            speed: 1,
            paused: false,
            auto: true,
            rebuild: true,
            notice: None,
        };
        screen.reset_view();
        screen
    }

    pub fn stage_id(&self) -> &str {
        &self.data.stages[self.stage].id
    }

    pub fn stage_name(&self) -> &str {
        &self.data.stages[self.stage].name
    }

    /// Начать бой заново с текущими этапом, уровнем и seed.
    pub fn restart(&mut self) {
        self.battle = new_battle(
            &self.data,
            self.stage,
            &self.team,
            self.gym_bonus,
            self.seed,
        );
        self.reset_view();
    }

    pub fn restart_with_new_seed(&mut self) {
        self.seed = random_seed();
        self.restart();
    }

    fn reset_view(&mut self) {
        self.shown_hp = self.battle.units().iter().map(|u| u.hp).collect();
        self.shown_current = self.battle.current();
        self.shown_outcome = None;
        self.queue.clear();
        self.wait = self.look.step_pause;
        self.rebuild = true;
    }

    /// Ульта по тапу. Работает в любой момент, даже вне хода героя.
    pub fn try_ultimate(&mut self, id: UnitId) {
        if self.shown_outcome.is_some() || !self.battle.unit(id).ultimate_ready() {
            return;
        }
        if let Ok(events) = self.battle.use_ultimate(id, None) {
            self.queue.extend(events);
        }
    }

    /// Сколько ждать после показа события.
    fn delay_after(&self, event: &Event) -> f32 {
        let look = &self.look;
        match event {
            Event::AbilityUsed { kind, .. } => match kind {
                AbilityKind::Basic => look.basic_windup,
                AbilityKind::Skill => look.skill_windup,
                AbilityKind::Ultimate => look.ult_windup,
            },
            Event::BroadsideFired => look.broadside_windup,
            Event::Damage { weakness, crit, .. } => {
                look.hit_gap
                    + if *weakness || *crit {
                        look.crit_hitstop
                    } else {
                        0.0
                    }
            }
            Event::Healed { .. } | Event::Advanced { .. } | Event::Delayed { .. } => look.hit_gap,
            Event::Defeated { .. } => look.defeat_pause,
            Event::TurnStarted { .. } | Event::BattleEnded { .. } => 0.0,
        }
    }

    /// Обновить то, что видит игрок, по событию.
    fn apply_shown(&mut self, event: &Event) {
        match *event {
            Event::Damage { target, amount, .. } => {
                let hp = &mut self.shown_hp[target.0];
                *hp = hp.saturating_sub(amount);
            }
            Event::Healed { target, amount, .. } => {
                let max = self.battle.unit(target).def.stats.max_hp;
                let hp = &mut self.shown_hp[target.0];
                *hp = (*hp + amount).min(max);
            }
            Event::Defeated { unit } => self.shown_hp[unit.0] = 0,
            Event::TurnStarted { unit } => self.shown_current = Some(unit),
            Event::BattleEnded { outcome } => {
                self.shown_outcome = Some(outcome);
                self.shown_current = None;
            }
            _ => {}
        }
    }
}

pub fn random_seed() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0)
}

fn new_battle(
    data: &GameData,
    stage: usize,
    team: &[(String, u32)],
    bonus: u32,
    seed: u64,
) -> Battle {
    let team = team
        .iter()
        .map(|(id, level)| {
            data.hero(id)
                .expect("герой есть в data/heroes.json")
                .to_unit(level + bonus)
        })
        .collect();
    let enemies = data
        .stage_enemies(&data.stages[stage].id)
        .expect("этап есть в data/stages.json");
    Battle::new(team, enemies, SHIP, seed)
}

/// Выход в поход: этап с карты или следующий непройденный.
pub fn enter(
    mut screen: ResMut<BattleScreen>,
    progress: Res<crate::menu::Progress>,
    voyage: Res<crate::menu::Voyage>,
    roster: Res<crate::crew::Roster>,
) {
    screen.team = roster.team_levels();
    let next = progress.next_stage(screen.data.stages.len());
    screen.stage = voyage.stage.unwrap_or(next);
    screen.paused = false;
    screen.restart_with_new_seed();
}

/// Возврат на корабль: убрать поле, интерфейс боя и эффекты, вернуть камеру на место.
#[allow(clippy::type_complexity)]
pub fn leave(
    mut commands: Commands,
    roots: Query<
        Entity,
        Or<(
            With<crate::view::ScreenRoot>,
            With<crate::battlefield::FieldRoot>,
            With<crate::fx::Floating>,
        )>,
    >,
    mut camera: Query<&mut Transform, With<Camera2d>>,
) {
    for root in &roots {
        commands.entity(root).despawn();
    }
    for mut transform in &mut camera {
        transform.translation = Vec3::ZERO;
    }
    commands.insert_resource(crate::fx::BandFx::default());
    commands.insert_resource(crate::fx::ScreenFx::default());
}

/// Проигрывает события по одному; когда очередь пуста — просит у боя следующий ход.
pub fn play_events(
    time: Res<Time>,
    mut screen: ResMut<BattleScreen>,
    mut shown: MessageWriter<Shown>,
) {
    if screen.paused || screen.rebuild {
        return;
    }
    screen.wait -= time.delta_secs() * screen.speed as f32;
    for _ in 0..MAX_EVENTS_PER_FRAME {
        if screen.wait > 0.0 {
            break;
        }
        if let Some(event) = screen.queue.pop_front() {
            screen.apply_shown(&event);
            screen.wait += screen.delay_after(&event);
            let focus = match event {
                Event::AbilityUsed { unit, .. } => first_target(&screen.queue, unit),
                _ => None,
            };
            shown.write(Shown { event, focus });
        } else if screen.battle.outcome().is_none() {
            let events = if screen.auto {
                screen.battle.auto_step()
            } else {
                screen.battle.auto_turn()
            };
            screen.queue.extend(events);
            screen.wait += screen.look.step_pause;
        } else {
            screen.wait = 0.0;
            break;
        }
    }
}

/// Клавиши для игры на ПК: те же действия, что у кнопок внизу экрана.
pub fn keyboard(keys: Res<ButtonInput<KeyCode>>, mut screen: ResMut<BattleScreen>) {
    use crate::view::{ButtonKind, press};
    for (key, kind) in [
        (KeyCode::KeyA, ButtonKind::Auto),
        (KeyCode::KeyX, ButtonKind::Speed),
        (KeyCode::Space, ButtonKind::Pause),
    ] {
        if keys.just_pressed(key) {
            press(&mut screen, kind);
        }
    }
    // N — новый бой, R — переиграть тот же (тот же seed).
    if keys.just_pressed(KeyCode::KeyN) {
        screen.restart_with_new_seed();
    }
    if keys.just_pressed(KeyCode::KeyR) {
        screen.restart();
    }
}

/// Первая цель действия: смотрим вперёд по очереди до следующего действия.
fn first_target(queue: &VecDeque<Event>, actor: UnitId) -> Option<UnitId> {
    for event in queue {
        match *event {
            Event::Damage {
                source: Some(source),
                target,
                ..
            } if source == actor => return Some(target),
            Event::Healed { source, target, .. } if source == actor => return Some(target),
            Event::AbilityUsed { .. } | Event::TurnStarted { .. } | Event::BattleEnded { .. } => {
                return None;
            }
            _ => {}
        }
    }
    None
}
