//! Экспедиции на шлюпках: свободные герои уходят на задание на несколько часов.
//!
//! В экспедицию идут герои не из боевого отряда; пока они в пути, в отряд их не взять.
//! Время — настоящее (время возвращения хранится в сохранении), так что экспедиция идёт и
//! без игрока. Шлюпок — базовые плюс отсек «Шлюпки». Задания — `data/expeditions.json`.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::abyss::ActivitiesDirty;
use crate::audio::Sfx;
use crate::battle_screen::BattleScreen;
use crate::crew::Roster;
use crate::menu::{MUTED, PARCHMENT, Spoils, Toast, pressable, text};
use crate::save::now;
use crate::ship::{Ship, ShipData, clock};
use crate::style::{self, Look, Skin};
use crate::view::Fonts;

#[derive(Resource, Deserialize)]
pub struct ExpeditionData {
    pub base_boats: u32,
    pub missions: Vec<Mission>,
}

#[derive(Deserialize)]
pub struct Mission {
    pub id: String,
    pub name: String,
    pub hours: f32,
    pub heroes: usize,
    pub reward: Loot,
}

#[derive(Deserialize, Default, Clone, Copy)]
#[serde(default)]
pub struct Loot {
    pub planks: u32,
    pub silver: u32,
    pub ether: u32,
    pub charts: u32,
    pub figureheads: u32,
    pub details: u32,
}

impl Loot {
    pub fn items(&self) -> Vec<(&'static str, u32)> {
        [
            ("planks", self.planks),
            ("silver", self.silver),
            ("ether", self.ether),
            ("charts", self.charts),
            ("details", self.details),
            ("figureheads", self.figureheads),
        ]
        .into_iter()
        .filter(|(_, n)| *n > 0)
        .collect()
    }

    fn line(&self) -> String {
        let mut parts = Vec::new();
        for (n, name) in [
            (self.planks, "досок"),
            (self.silver, "серебра"),
            (self.ether, "эфира"),
            (self.charts, "лоции"),
            (self.details, "деталей"),
            (self.figureheads, "носовая фигура"),
        ] {
            if n > 0 {
                parts.push(format!("+{n} {name}"));
            }
        }
        parts.join(" · ")
    }
}

impl ExpeditionData {
    fn bundled() -> Self {
        serde_json::from_str(include_str!("../../../data/expeditions.json"))
            .expect("data/expeditions.json корректен")
    }

    fn mission(&self, id: &str) -> Option<&Mission> {
        self.missions.iter().find(|m| m.id == id)
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Running {
    pub mission: String,
    pub heroes: Vec<String>,
    /// Время возвращения, секунды Unix.
    pub ends_at: u64,
}

/// Экспедиции в пути (сохраняются).
#[derive(Resource, Clone, Default, Serialize, Deserialize)]
pub struct Expeditions {
    pub running: Vec<Running>,
}

impl Expeditions {
    pub fn is_away(&self, hero: &str) -> bool {
        self.running
            .iter()
            .any(|r| r.heroes.iter().any(|h| h == hero))
    }

    /// Отправить: свободные герои не из отряда, сильнейшие первыми.
    pub fn send(
        &mut self,
        mission: &Mission,
        roster: &Roster,
        all_heroes: &[String],
        boats: u32,
        now: u64,
    ) -> Result<(), String> {
        if self.running.iter().any(|r| r.mission == mission.id) {
            return Err("Эта экспедиция уже в пути".into());
        }
        if self.running.len() as u32 >= boats {
            return Err(format!(
                "Все шлюпки в пути: {boats} из {boats}. Ещё шлюпки даёт отсек «Шлюпки»"
            ));
        }
        let mut free: Vec<&String> = all_heroes
            .iter()
            .filter(|h| roster.owns(h) && !roster.team.contains(h) && !self.is_away(h))
            .collect();
        free.sort_by_key(|h| std::cmp::Reverse(roster.level(h)));
        if free.len() < mission.heroes {
            return Err(format!(
                "Нужно свободных героев (не в отряде): {}, есть {}",
                mission.heroes,
                free.len()
            ));
        }
        self.running.push(Running {
            mission: mission.id.clone(),
            heroes: free[..mission.heroes]
                .iter()
                .map(|h| (*h).clone())
                .collect(),
            ends_at: now + (mission.hours * 3600.0) as u64,
        });
        Ok(())
    }
}

pub fn plugin(app: &mut App) {
    app.insert_resource(ExpeditionData::bundled())
        .init_resource::<Expeditions>();
}

/// Раздел экспедиций на экране «Активности».
#[allow(clippy::too_many_arguments)]
pub fn section(
    col: &mut ChildSpawnerCommands,
    assets: &AssetServer,
    fonts: &Fonts,
    data: &ExpeditionData,
    exps: &Expeditions,
    boats: u32,
    screen: &BattleScreen,
    now: u64,
) {
    let _ = screen;
    col.spawn((
        Node {
            padding: UiRect::axes(px(16), px(14)),
            flex_direction: FlexDirection::Column,
            row_gap: px(10),
            border: UiRect::all(px(2)),
            border_radius: BorderRadius::all(px(14)),
            ..default()
        },
        Skin(Look::Tile),
    ))
    .with_children(|card| {
        card.spawn(Node {
            column_gap: px(10),
            align_items: AlignItems::Center,
            ..default()
        })
        .with_children(|row| {
            row.spawn(style::heading(fonts, "Шлюпки", 20.0, PARCHMENT));
            // Каждая шлюпка — картинкой: свободная яркая, ушедшая — тусклая.
            let away = exps.running.len() as u32;
            for i in 0..boats {
                let busy = i < away;
                row.spawn((
                    Node {
                        width: px(46),
                        height: px(44),
                        flex_shrink: 0.0,
                        ..default()
                    },
                    ImageNode {
                        image: assets.load("ui/act_expeditions.png"),
                        color: if busy {
                            Color::srgba(0.4, 0.4, 0.45, 0.7)
                        } else {
                            Color::WHITE
                        },
                        ..default()
                    },
                ));
            }
            row.spawn(Node {
                flex_grow: 1.0,
                ..default()
            });
            let free = boats.saturating_sub(away);
            style::pill(
                row,
                fonts,
                format!("свободно {free} из {boats}"),
                if free > 0 {
                    Color::srgb(0.22, 0.5, 0.26)
                } else {
                    Color::srgb(0.45, 0.18, 0.14)
                },
            );
        });
        card.spawn(text(
            fonts,
            "Идут герои не из боевого отряда. Пока они в пути, в отряд их не взять.",
            false,
            12.0,
            MUTED,
        ));
        for m in &data.missions {
            let running = exps.running.iter().find(|r| r.mission == m.id);
            let back = running.is_some_and(|r| now >= r.ends_at);
            card.spawn((
                Node {
                    padding: UiRect::axes(px(10), px(8)),
                    column_gap: px(10),
                    align_items: AlignItems::Center,
                    border_radius: BorderRadius::all(px(10)),
                    ..default()
                },
                BackgroundColor(Color::srgba(1.0, 1.0, 1.0, if back { 0.12 } else { 0.05 })),
            ))
            .with_children(|row| {
                row.spawn(Node {
                    flex_grow: 1.0,
                    flex_shrink: 1.0,
                    flex_direction: FlexDirection::Column,
                    row_gap: px(5),
                    ..default()
                })
                .with_children(|c| {
                    c.spawn(text(fonts, m.name.clone(), true, 14.0, PARCHMENT));
                    c.spawn(Node {
                        column_gap: px(6),
                        align_items: AlignItems::Center,
                        ..default()
                    })
                    .with_children(|info| match running {
                        Some(r) => {
                            for h in &r.heroes {
                                info.spawn(style::medallion(assets, h, 32.0, false));
                            }
                            if back {
                                style::pill(
                                    info,
                                    fonts,
                                    "вернулись с добычей!",
                                    Color::srgb(0.7, 0.5, 0.12),
                                );
                            } else {
                                style::pill(
                                    info,
                                    fonts,
                                    format!("вернутся через {}", clock((r.ends_at - now) as f32)),
                                    Color::srgb(0.24, 0.2, 0.3),
                                );
                            }
                        }
                        None => {
                            style::pill(
                                info,
                                fonts,
                                format!("{} ч", m.hours),
                                Color::srgb(0.24, 0.2, 0.3),
                            );
                            style::pill(
                                info,
                                fonts,
                                format!("героев: {}", m.heroes),
                                Color::srgb(0.24, 0.2, 0.3),
                            );
                        }
                    });
                    style::costs(c, assets, fonts, &m.reward.items(), 14.0, PARCHMENT);
                });
                let (label, active) = match running {
                    Some(_) if back => ("Забрать", true),
                    Some(_) => ("В пути", false),
                    None => ("Отправить", true),
                };
                let mut button = row.spawn((
                    Node {
                        width: px(108),
                        height: px(44),
                        flex_shrink: 0.0,
                        justify_content: JustifyContent::Center,
                        align_items: AlignItems::Center,
                        border: UiRect::all(px(2)),
                        border_radius: BorderRadius::all(px(10)),
                        ..default()
                    },
                    Skin(if active { Look::Go } else { Look::Off }),
                ));
                button.with_child(text(fonts, label, true, 15.0, Color::WHITE));
                if active {
                    let id = m.id.clone();
                    pressable(&mut button);
                    button.observe(move |_: On<PointerClick>, mut commands: Commands| {
                        commands.run_system_cached_with(act, id.clone());
                    });
                }
            });
        }
    });
}

/// Отправить экспедицию или забрать вернувшуюся.
#[allow(clippy::too_many_arguments)]
fn act(
    In(id): In<String>,
    data: Res<ExpeditionData>,
    screen: Res<BattleScreen>,
    ship: (Res<Ship>, Res<ShipData>),
    roster: Res<Roster>,
    mut exps: ResMut<Expeditions>,
    mut spoils: Spoils,
    mut toast: ResMut<Toast>,
    mut dirty: ResMut<ActivitiesDirty>,
    mut sfx: MessageWriter<Sfx>,
) {
    let Some(mission) = data.mission(&id) else {
        return;
    };
    let t = now();
    if let Some(i) = exps.running.iter().position(|r| r.mission == id) {
        if t < exps.running[i].ends_at {
            return;
        }
        exps.running.remove(i);
        let loot = mission.reward;
        let cap = ship.0.capacity(&ship.1);
        spoils.give(loot.silver, loot.planks, 0);
        let p = &mut spoils.profile;
        p.ether = (p.ether + loot.ether).min(cap.ether);
        p.charts += loot.charts;
        p.figureheads += loot.figureheads;
        p.details += loot.details;
        toast.show(&format!("Экспедиция вернулась: {}", loot.line()));
        sfx.write(Sfx("coin"));
    } else {
        let boats = data.base_boats + ship.0.extra_boats(&ship.1);
        let all: Vec<String> = screen.data.heroes.iter().map(|h| h.id.clone()).collect();
        match exps.send(mission, &roster, &all, boats, t) {
            Ok(()) => {
                toast.show(&format!("Шлюпка ушла: {}", mission.name));
                sfx.write(Sfx("flip"));
            }
            Err(reason) => toast.show(&reason),
        }
    }
    dirty.0 = true;
}

/// Сколько шлюпок есть сейчас.
pub fn boats(data: &ExpeditionData, ship: &Ship, ship_data: &ShipData) -> u32 {
    data.base_boats + ship.extra_boats(ship_data)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crew::CrewData;

    fn all() -> Vec<String> {
        ["ren", "sora", "kaya", "lira", "mei", "borg"]
            .map(String::from)
            .to_vec()
    }

    #[test]
    fn sends_only_free_heroes() {
        let data = ExpeditionData::bundled();
        let mut roster = Roster::new(&CrewData::bundled());
        let mut exps = Expeditions::default();
        let two = data.missions.iter().find(|m| m.heroes == 2).unwrap();
        // Стартовая четвёрка вся в отряде — свободных нет.
        assert!(exps.send(two, &roster, &all(), 3, 0).is_err());
        roster.recruit("sora");
        roster.recruit("lira");
        assert!(exps.send(two, &roster, &all(), 3, 0).is_ok());
        assert!(exps.is_away("sora") && exps.is_away("lira"));
    }

    #[test]
    fn boats_limit_running() {
        let data = ExpeditionData::bundled();
        let mut roster = Roster::new(&CrewData::bundled());
        roster.recruit("sora");
        roster.recruit("lira");
        let mut exps = Expeditions::default();
        let one = data.missions.iter().find(|m| m.heroes == 1).unwrap();
        assert!(exps.send(one, &roster, &all(), 1, 0).is_ok());
        let other = data
            .missions
            .iter()
            .find(|m| m.id != one.id && m.heroes == 1);
        if let Some(other) = other {
            assert!(exps.send(other, &roster, &all(), 1, 0).is_err());
        }
    }
}
