//! Состояние экрана боя: бой из battle-core, очередь событий и их проигрывание с паузами.
//!
//! Модель (`Battle`) считает ход сразу целиком, а экран показывает его события по одному.
//! Поэтому здоровье на экране — своя копия, которая меняется только по событиям.

use std::collections::VecDeque;
use std::time::{SystemTime, UNIX_EPOCH};

use battle_core::{AbilityKind, Battle, Event, Outcome, Ship, UnitId};
use bevy::prelude::*;
use game_data::GameData;
use serde::Deserialize;

/// Пока стартовый состав задан здесь; со временем его будет выбирать игрок.
pub const STARTER_TEAM: [&str; 4] = ["ren", "kaya", "borg", "mei"];
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
    /// Сколько висит баннер с названием действия.
    pub banner_basic: f32,
    pub banner_skill: f32,
    pub banner_ult: f32,
    pub lunge_px: f32,
    pub lunge_time: f32,
    pub shake_px: f32,
    pub shake_time: f32,
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
pub struct Shown(pub Event);

#[derive(Resource)]
pub struct BattleScreen {
    pub data: GameData,
    pub look: Presentation,
    pub stage: usize,
    pub level: u32,
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
    pub fn new(data: GameData, look: Presentation) -> Self {
        let seed = random_seed();
        let battle = new_battle(&data, 0, 1, seed);
        let mut screen = Self {
            data,
            look,
            stage: 0,
            level: 1,
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
        self.battle = new_battle(&self.data, self.stage, self.level, self.seed);
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
            Event::TurnStarted { .. }
            | Event::SkillPointsChanged { .. }
            | Event::BattleEnded { .. } => 0.0,
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

fn new_battle(data: &GameData, stage: usize, level: u32, seed: u64) -> Battle {
    let team = STARTER_TEAM
        .iter()
        .map(|id| {
            data.hero(id)
                .expect("герой есть в data/heroes.json")
                .to_unit(level)
        })
        .collect();
    let enemies = data
        .stage_enemies(&data.stages[stage].id)
        .expect("этап есть в data/stages.json");
    Battle::new(team, enemies, SHIP, seed)
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
            shown.write(Shown(event));
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
        (KeyCode::KeyN, ButtonKind::Restart),
    ] {
        if keys.just_pressed(key) {
            press(&mut screen, kind);
        }
    }
    // R — переиграть тот же бой (тот же seed).
    if keys.just_pressed(KeyCode::KeyR) {
        screen.restart();
    }
}
