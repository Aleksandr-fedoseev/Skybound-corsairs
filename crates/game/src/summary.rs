//! Итог боя для экрана результата: короткая фраза, добыча иконками, плашки и отряд со здоровьем.
//!
//! Подсчёт (menu.rs, events.rs, ruins.rs, arena.rs) собирает `Summary`, экран боя (view.rs) рисует.
//! Голого текста «+150 серебра · 30 журналов» на экране нет — всё иконками и плашками.

use bevy::prelude::*;

use crate::events::Bundle;

/// Цвет плашки: хорошее, плохое, справка, особое.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tone {
    Good,
    Bad,
    Info,
    Gold,
}

impl Tone {
    pub fn color(self) -> Color {
        match self {
            Tone::Good => Color::srgb(0.22, 0.5, 0.26),
            Tone::Bad => Color::srgb(0.62, 0.2, 0.16),
            Tone::Info => Color::srgb(0.24, 0.2, 0.3),
            Tone::Gold => Color::srgb(0.7, 0.5, 0.12),
        }
    }
}

#[derive(Default, Clone, Debug)]
pub struct Summary {
    /// Одна короткая фраза: «Этаж 3 взят!».
    pub line: String,
    /// Ресурсы: имя иконки (`ui/icon_<имя>.png`) и сколько (отрицательное — потрачено).
    pub gains: Vec<(&'static str, i64)>,
    pub chips: Vec<(String, Tone)>,
    /// Здоровье героев (доля, 0 — пал) — для боёв с продолжением.
    pub team: Vec<(String, f32)>,
}

impl Summary {
    pub fn new(line: impl Into<String>) -> Self {
        Self {
            line: line.into(),
            ..default()
        }
    }

    pub fn gain(mut self, kind: &'static str, n: i64) -> Self {
        if n != 0 {
            self.gains.push((kind, n));
        }
        self
    }

    pub fn bundle(mut self, b: &Bundle) -> Self {
        for (kind, n) in b.items() {
            self = self.gain(kind, n as i64);
        }
        self
    }

    pub fn chip(mut self, text: impl Into<String>, tone: Tone) -> Self {
        self.chips.push((text.into(), tone));
        self
    }

    pub fn team(mut self, hp: &[(String, f32)]) -> Self {
        self.team = hp.to_vec();
        self
    }
}

/// Итог последнего боя: его рисует экран результата.
#[derive(Resource, Default)]
pub struct ResultSummary {
    pub summary: Option<Summary>,
    /// Нарисовать заново.
    pub dirty: bool,
}

impl ResultSummary {
    pub fn set(&mut self, summary: Summary) {
        self.summary = Some(summary);
        self.dirty = true;
    }
}
