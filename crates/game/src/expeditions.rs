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
use crate::menu::{
    BRASS, GO, GO_EDGE, GOLD, MUTED, PARCHMENT, Spoils, Toast, WOOD, pressable, text,
};
use crate::save::now;
use crate::ship::{Ship, ShipData, clock};
use crate::view::Fonts;

const DONE: Color = Color::srgb(0.30, 0.62, 0.38);
const DISABLED: Color = Color::srgb(0.36, 0.38, 0.43);

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
            return Err("Все шлюпки в пути — больше шлюпок даёт отсек «Шлюпки»".into());
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
    fonts: &Fonts,
    data: &ExpeditionData,
    exps: &Expeditions,
    boats: u32,
    screen: &BattleScreen,
    now: u64,
) {
    col.spawn((
        Node {
            padding: UiRect::all(px(14)),
            flex_direction: FlexDirection::Column,
            row_gap: px(8),
            border: UiRect::all(px(1)),
            border_radius: BorderRadius::all(px(14)),
            ..default()
        },
        BackgroundColor(WOOD),
        BorderColor::all(BRASS),
    ))
    .with_children(|card| {
        card.spawn(text(
            fonts,
            format!(
                "Экспедиции на шлюпках · шлюпок в пути {} из {boats}",
                exps.running.len()
            ),
            true,
            17.0,
            PARCHMENT,
        ));
        card.spawn(text(
            fonts,
            "Идут герои не из боевого отряда. Пока они в пути, в отряд их не взять.",
            false,
            12.0,
            MUTED,
        ));
        for m in &data.missions {
            let running = exps.running.iter().find(|r| r.mission == m.id);
            let (status, label, fill) = match running {
                Some(r) if now >= r.ends_at => ("вернулись с добычей!".to_string(), "Забрать", GO),
                Some(r) => {
                    let names: Vec<String> = r
                        .heroes
                        .iter()
                        .map(|h| screen.data.hero(h).map_or(h.clone(), |d| d.name.clone()))
                        .collect();
                    (
                        format!(
                            "{} · вернутся через {}",
                            names.join(", "),
                            clock((r.ends_at - now) as f32)
                        ),
                        "В пути",
                        DISABLED,
                    )
                }
                None => (
                    format!("{} ч · героев: {} · {}", m.hours, m.heroes, m.reward.line()),
                    "Отправить",
                    DONE,
                ),
            };
            card.spawn((
                Node {
                    padding: UiRect::axes(px(10), px(6)),
                    column_gap: px(10),
                    align_items: AlignItems::Center,
                    border_radius: BorderRadius::all(px(10)),
                    ..default()
                },
                BackgroundColor(Color::srgba(1.0, 1.0, 1.0, 0.04)),
            ))
            .with_children(|row| {
                row.spawn(Node {
                    flex_grow: 1.0,
                    flex_shrink: 1.0,
                    flex_direction: FlexDirection::Column,
                    row_gap: px(2),
                    ..default()
                })
                .with_children(|c| {
                    c.spawn(text(fonts, m.name.clone(), true, 14.0, PARCHMENT));
                    c.spawn(text(
                        fonts,
                        status,
                        false,
                        12.0,
                        if running.is_some() { GOLD } else { MUTED },
                    ));
                });
                let mut button = row.spawn((
                    Node {
                        width: px(108),
                        height: px(42),
                        flex_shrink: 0.0,
                        justify_content: JustifyContent::Center,
                        align_items: AlignItems::Center,
                        border: UiRect::all(px(2)),
                        border_radius: BorderRadius::all(px(10)),
                        ..default()
                    },
                    BackgroundColor(fill),
                    BorderColor::all(if fill == DISABLED { MUTED } else { GO_EDGE }),
                ));
                button.with_child(text(fonts, label, true, 14.0, Color::WHITE));
                if fill != DISABLED {
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
