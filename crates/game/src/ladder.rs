//! Таблицы капитанов: Арена (очки недели) и сила отряда (лучшая за всё время).
//!
//! Сервера пока нет, поэтому соседи по таблице — капитаны из `data/ladder.json`: на Арене их очки
//! растут по ходу недели, по силе — разброс от новичков до бывалых. Игрок стоит на своём месте.
//! С сервером строки заменят настоящие игроки. Открывается кубком на главной.

use bevy::prelude::*;
use serde::Deserialize;

use crate::menu::{MUTED, MenuRebuild, PARCHMENT, pressable};
use crate::style::{self, Look, Skin};
use crate::view::Fonts;

#[derive(Resource, Deserialize)]
pub struct LadderData {
    pub names: Vec<String>,
    /// Сила капитанов таблицы, от первого места вниз.
    pub power: Vec<u32>,
    /// Очки первого места Арены к концу недели; дальше каждое место — в `arena_step` раз меньше.
    pub arena_top: u32,
    pub arena_step: f32,
    pub arena_size: usize,
    /// Сколько верхних строк видно.
    pub shown: usize,
}

/// Открыта ли таблица и какая вкладка.
#[derive(Resource, Default)]
pub struct LadderOpen(pub Option<Tab>);

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Arena,
    Power,
}

/// Строка таблицы: имя, число, это игрок.
pub struct Row {
    pub name: String,
    pub value: u32,
    pub me: bool,
}

impl LadderData {
    pub fn bundled() -> Self {
        serde_json::from_str(include_str!("../../../data/ladder.json"))
            .expect("data/ladder.json корректен")
    }

    /// Таблица Арены: доля прошедшей недели `progress` (0..1) — очки капитанов растут к концу.
    pub fn arena(&self, week: u64, progress: f32, rating: u32) -> Vec<Row> {
        let mut rng = battle_core::Rng::new(week * 7_919 + 3);
        let rows = (0..self.arena_size).map(|i| {
            let full = self.arena_top as f32 * self.arena_step.powi(i as i32);
            let jitter = rng.range_f32(0.85, 1.1);
            Row {
                name: self.names[(i + week as usize) % self.names.len()].clone(),
                value: (full * progress.clamp(0.05, 1.0) * jitter).round() as u32,
                me: false,
            }
        });
        ranked(rows.collect(), rating)
    }

    /// Таблица силы: лучшая сила отряда игрока против постоянных капитанов.
    pub fn power(&self, best: u32) -> Vec<Row> {
        let rows = self
            .power
            .iter()
            .enumerate()
            .map(|(i, p)| Row {
                name: self.names[(i * 7 + 3) % self.names.len()].clone(),
                value: *p,
                me: false,
            })
            .collect();
        ranked(rows, best)
    }
}

/// Поставить игрока на его место и отсортировать.
fn ranked(mut rows: Vec<Row>, mine: u32) -> Vec<Row> {
    rows.push(Row {
        name: "Вы".into(),
        value: mine,
        me: true,
    });
    // При равенстве игрок выше: он только что дотянулся.
    rows.sort_by(|a, b| b.value.cmp(&a.value).then(b.me.cmp(&a.me)));
    rows
}

pub fn plugin(app: &mut App) {
    app.insert_resource(LadderData::bundled())
        .init_resource::<LadderOpen>()
        .add_systems(Update, crate::power::track_best);
}

/// Кнопка с кубком под кнопкой ивента на главной.
pub fn button(root: &mut ChildSpawnerCommands, assets: &AssetServer, fonts: &Fonts) {
    let mut b = root.spawn((
        Node {
            position_type: PositionType::Absolute,
            top: px(148),
            left: px(10),
            width: px(84),
            height: px(62),
            flex_direction: FlexDirection::Column,
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            ..default()
        },
        Skin(Look::TabOff),
    ));
    b.with_children(|c| {
        c.spawn(style::icon(assets, "ladder", 30.0));
        c.spawn(style::heading(fonts, "Рейтинг", 13.0, PARCHMENT));
    });
    pressable(&mut b);
    b.observe(
        |_: On<PointerClick>, mut open: ResMut<LadderOpen>, mut rebuild: ResMut<MenuRebuild>| {
            open.0 = Some(Tab::Arena);
            rebuild.0 = true;
        },
    );
}

/// Панель таблицы поверх главной: вкладки, верхние места и место игрока.
pub fn panel(
    root: &mut ChildSpawnerCommands,
    assets: &AssetServer,
    fonts: &Fonts,
    tab: Tab,
    rows: &[Row],
    shown: usize,
) {
    root.spawn((
        Node {
            position_type: PositionType::Absolute,
            top: px(66),
            left: px(16),
            right: px(16),
            padding: UiRect::axes(px(16), px(14)),
            flex_direction: FlexDirection::Column,
            row_gap: px(6),
            border: UiRect::all(px(2)),
            border_radius: BorderRadius::all(px(14)),
            ..default()
        },
        Skin(Look::Tile),
        GlobalZIndex(12),
    ))
    .with_children(|p| {
        p.spawn(Node {
            column_gap: px(8),
            align_items: AlignItems::Center,
            margin: UiRect::bottom(px(6)),
            ..default()
        })
        .with_children(|row| {
            tab_button(row, assets, fonts, Tab::Arena, tab);
            tab_button(row, assets, fonts, Tab::Power, tab);
            row.spawn(Node {
                flex_grow: 1.0,
                ..default()
            });
            let mut close = row.spawn((
                Node {
                    width: px(40),
                    height: px(36),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    ..default()
                },
                Skin(Look::Dark),
            ));
            close.with_child(style::heading(fonts, "×", 22.0, PARCHMENT));
            pressable(&mut close);
            close.observe(
                |_: On<PointerClick>,
                 mut open: ResMut<LadderOpen>,
                 mut rebuild: ResMut<MenuRebuild>| {
                    open.0 = None;
                    rebuild.0 = true;
                },
            );
        });
        let me = rows.iter().position(|r| r.me).unwrap_or(0);
        for (i, r) in rows.iter().enumerate().take(shown) {
            line(p, assets, fonts, tab, i + 1, r);
        }
        if me >= shown {
            p.spawn(style::heading(fonts, "···", 18.0, MUTED));
            line(p, assets, fonts, tab, me + 1, &rows[me]);
        }
    });
}

fn tab_button(
    row: &mut ChildSpawnerCommands,
    assets: &AssetServer,
    fonts: &Fonts,
    tab: Tab,
    on: Tab,
) {
    let mut b = row.spawn((
        Node {
            height: px(42),
            padding: UiRect::horizontal(px(14)),
            column_gap: px(6),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            ..default()
        },
        Skin(if tab == on { Look::TabOn } else { Look::TabOff }),
    ));
    b.with_children(|c| match tab {
        Tab::Arena => {
            c.spawn(style::icon(assets, "ladder", 26.0));
            c.spawn(style::heading(fonts, "Арена", 17.0, PARCHMENT));
        }
        // Вкладка силы — только иконка: число рядом с саблями понятно и так.
        Tab::Power => {
            c.spawn(style::icon(assets, "power", 30.0));
        }
    });
    pressable(&mut b);
    b.observe(
        move |_: On<PointerClick>,
              mut open: ResMut<LadderOpen>,
              mut rebuild: ResMut<MenuRebuild>| {
            open.0 = Some(tab);
            rebuild.0 = true;
        },
    );
}

/// Строка: место (первые три — медалью), имя, число с иконкой.
fn line(
    parent: &mut ChildSpawnerCommands,
    assets: &AssetServer,
    fonts: &Fonts,
    tab: Tab,
    place: usize,
    r: &Row,
) {
    let medal = match place {
        1 => Color::srgb(0.85, 0.66, 0.2),
        2 => Color::srgb(0.66, 0.68, 0.72),
        3 => Color::srgb(0.7, 0.45, 0.25),
        _ => Color::srgb(0.22, 0.18, 0.15),
    };
    parent
        .spawn((
            Node {
                height: px(40),
                padding: UiRect::horizontal(px(8)),
                column_gap: px(10),
                align_items: AlignItems::Center,
                border: UiRect::all(px(if r.me { 2 } else { 0 })),
                border_radius: BorderRadius::all(px(10)),
                ..default()
            },
            BackgroundColor(if r.me {
                Color::srgba(0.85, 0.6, 0.2, 0.25)
            } else {
                Color::srgba(1.0, 1.0, 1.0, 0.05)
            }),
            BorderColor::all(Color::srgb(1.0, 0.8, 0.4)),
        ))
        .with_children(|row| {
            row.spawn((
                Node {
                    width: px(34),
                    height: px(30),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    border_radius: BorderRadius::all(px(15)),
                    ..default()
                },
                BackgroundColor(medal),
            ))
            .with_child(style::heading(fonts, place.to_string(), 15.0, Color::WHITE));
            row.spawn((
                Node {
                    flex_grow: 1.0,
                    ..default()
                },
                style::heading(fonts, r.name.clone(), 17.0, PARCHMENT),
            ));
            let icon = match tab {
                Tab::Arena => "ladder",
                Tab::Power => "power",
            };
            row.spawn(style::icon(assets, icon, 22.0));
            row.spawn(style::heading(
                fonts,
                crate::power::format(r.value),
                17.0,
                Color::srgb(1.0, 0.85, 0.5),
            ));
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn player_stands_in_place() {
        let data = LadderData::bundled();
        let rows = data.power(data.power[3] + 1);
        assert!(rows[3].me, "сила чуть выше 4-го места — 4-е место");
        let rows = data.power(0);
        assert!(rows.last().unwrap().me);
        // Очки Арены к концу недели больше, чем в начале.
        let early: u32 = data.arena(9, 0.1, 0).iter().map(|r| r.value).sum();
        let late: u32 = data.arena(9, 1.0, 0).iter().map(|r| r.value).sum();
        assert!(late > early);
    }
}
