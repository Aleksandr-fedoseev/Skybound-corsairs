//! Ивенты по календарю: неделя за неделей по циклу из `data/events.json`.
//!
//! Пока готова «Буря»: буревая тварь с огромным запасом здоровья, счёт — урон за бой,
//! засчитывается лучший бой недели, за пороги урона — награды. По выходным —
//! «Попутный ветер»: добыча с конвоев больше.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::Screen;
use crate::battle_screen::{CustomFight, FightKind};
use crate::menu::{GO, GO_EDGE, GOLD, MUTED, PARCHMENT, Profile, Voyage, pressable, text};
use crate::save::now;
use crate::ship::clock;
use crate::story::VoyageRequest;
use crate::view::Fonts;

const SECONDS_PER_DAY: u64 = 86_400;
/// 1 января 1970 года — четверг; сдвиг, чтобы неделя начиналась в понедельник.
const THURSDAY_SHIFT: u64 = 3;
const STORM: Color = Color::srgb(0.30, 0.20, 0.45);

#[derive(Resource, Deserialize)]
pub struct EventData {
    pub cycle: Vec<String>,
    pub storm: StormData,
    pub tailwind: Tailwind,
}

#[derive(Deserialize)]
pub struct StormData {
    pub name: String,
    pub about: String,
    pub enemies: Vec<(String, u32)>,
    pub milestones: Vec<Milestone>,
}

#[derive(Deserialize)]
pub struct Milestone {
    pub damage: u32,
    pub reward: Prize,
}

#[derive(Deserialize, Default, Clone, Copy)]
#[serde(default)]
pub struct Prize {
    pub charts: u32,
    pub doubloons: u32,
    pub storm_cores: u32,
}

impl Prize {
    fn line(&self) -> String {
        let mut parts = Vec::new();
        for (n, name) in [
            (self.charts, "лоции"),
            (self.doubloons, "дублонов"),
            (self.storm_cores, "буревых ядер"),
        ] {
            if n > 0 {
                parts.push(format!("+{n} {name}"));
            }
        }
        parts.join(" · ")
    }
}

#[derive(Deserialize)]
pub struct Tailwind {
    /// Дни недели (0 — понедельник), когда дует попутный ветер.
    pub days: Vec<u64>,
    pub convoy_multiplier: u32,
}

impl EventData {
    fn bundled() -> Self {
        serde_json::from_str(include_str!("../../../data/events.json"))
            .expect("data/events.json корректен")
    }

    /// Ивент текущей недели.
    pub fn current(&self) -> &str {
        &self.cycle[(week() as usize) % self.cycle.len()]
    }

    /// Во сколько раз больше добыча с конвоя сегодня.
    pub fn convoy_multiplier(&self) -> u32 {
        if self.tailwind.days.contains(&weekday()) {
            self.tailwind.convoy_multiplier
        } else {
            1
        }
    }

    pub fn storm_fight(&self) -> CustomFight {
        CustomFight {
            title: format!("{} · буревая тварь", self.storm.name),
            kind: FightKind::Storm,
            enemies: self.storm.enemies.clone(),
            silver: 0,
            planks: 0,
            journals: 0,
        }
    }
}

fn days() -> u64 {
    now() / SECONDS_PER_DAY
}

pub fn week() -> u64 {
    (days() + THURSDAY_SHIFT) / 7
}

fn weekday() -> u64 {
    (days() + THURSDAY_SHIFT) % 7
}

/// Сколько секунд до конца недели (до понедельника 00:00 UTC).
pub fn week_left() -> u64 {
    let next = (week() + 1) * 7 - THURSDAY_SHIFT;
    (next * SECONDS_PER_DAY).saturating_sub(now())
}

/// «3д 5ч» для долгих сроков, «1:23:45» — для коротких.
fn days_left(seconds: u64) -> String {
    let (d, h) = (seconds / SECONDS_PER_DAY, seconds % SECONDS_PER_DAY / 3600);
    if d > 0 {
        format!("{d}д {h}ч")
    } else {
        clock(seconds as f32)
    }
}

/// Прогресс «Бури» за неделю (сохраняется).
#[derive(Resource, Clone, Default, Serialize, Deserialize)]
pub struct StormEvent {
    pub week: u64,
    pub best: u32,
    /// Сколько порогов уже выдано.
    pub reached: usize,
}

impl StormEvent {
    fn roll_week(&mut self) {
        let w = week();
        if self.week != w {
            *self = Self {
                week: w,
                ..default()
            };
        }
    }

    /// Записать урон боя; выдать награды за новые пороги. Возвращает строку итога.
    pub fn record(&mut self, damage: u32, data: &StormData, profile: &mut Profile) -> String {
        self.roll_week();
        self.best = self.best.max(damage);
        let mut got = Vec::new();
        while let Some(m) = data.milestones.get(self.reached) {
            if self.best < m.damage {
                break;
            }
            profile.charts += m.reward.charts;
            profile.doubloons += m.reward.doubloons;
            profile.storm_cores += m.reward.storm_cores;
            got.push(m.reward.line());
            self.reached += 1;
        }
        let mut note = format!("Урон по буре: {damage} · лучший за неделю: {}", self.best);
        if !got.is_empty() {
            note.push_str(&format!("\nНаграды: {}", got.join(" · ")));
        }
        note
    }
}

pub fn plugin(app: &mut App) {
    app.insert_resource(EventData::bundled())
        .init_resource::<StormEvent>();
}

/// Карточка ивента недели на экране «Активности».
pub fn storm_card(
    col: &mut ChildSpawnerCommands,
    fonts: &Fonts,
    data: &EventData,
    storm: &StormEvent,
) {
    if data.current() != "storm" {
        return;
    }
    let best = if storm.week == week() { storm.best } else { 0 };
    let reached = if storm.week == week() {
        storm.reached
    } else {
        0
    };
    let next = data.storm.milestones.get(reached);
    col.spawn((
        Node {
            padding: UiRect::all(px(14)),
            flex_direction: FlexDirection::Column,
            row_gap: px(5),
            border: UiRect::all(px(2)),
            border_radius: BorderRadius::all(px(14)),
            ..default()
        },
        BackgroundColor(STORM),
        BorderColor::all(GOLD),
    ))
    .with_children(|card| {
        card.spawn(text(
            fonts,
            format!(
                "Ивент недели: {} · осталось {}",
                data.storm.name,
                days_left(week_left())
            ),
            true,
            18.0,
            PARCHMENT,
        ));
        card.spawn(text(fonts, data.storm.about.clone(), false, 12.0, MUTED));
        card.spawn(text(
            fonts,
            match next {
                Some(m) => format!(
                    "Лучший урон: {best} · следующий порог {} — {}",
                    m.damage,
                    m.reward.line()
                ),
                None => format!("Лучший урон: {best} · все награды недели получены"),
            },
            true,
            13.0,
            GOLD,
        ));
        let mut go = card.spawn((
            Node {
                margin: UiRect::top(px(4)),
                height: px(48),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                border: UiRect::all(px(2)),
                border_radius: BorderRadius::all(px(12)),
                ..default()
            },
            BackgroundColor(GO),
            BorderColor::all(GO_EDGE),
        ));
        go.with_child(text(fonts, "Атаковать бурю", true, 18.0, Color::WHITE));
        pressable(&mut go);
        go.observe(
            |_: On<PointerClick>,
             data: Res<EventData>,
             mut voyage: ResMut<Voyage>,
             mut requests: MessageWriter<VoyageRequest>| {
                *voyage = Voyage {
                    stage: None,
                    custom: Some(data.storm_fight()),
                    from: Screen::Activities,
                };
                requests.write(VoyageRequest);
            },
        );
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::menu::MenuData;

    #[test]
    fn milestones_granted_once_by_best_damage() {
        let data = EventData::bundled();
        let mut storm = StormEvent::default();
        let mut profile = Profile::new(&MenuData::bundled().start);
        let first = data.storm.milestones[0].damage;
        storm.record(first - 1, &data.storm, &mut profile);
        assert_eq!(storm.reached, 0);
        storm.record(first, &data.storm, &mut profile);
        assert_eq!(storm.reached, 1);
        let charts = profile.charts;
        // Слабый бой не отнимает лучший результат и не выдаёт награду второй раз.
        storm.record(10, &data.storm, &mut profile);
        assert_eq!(storm.reached, 1);
        assert_eq!(profile.charts, charts);
    }
}
