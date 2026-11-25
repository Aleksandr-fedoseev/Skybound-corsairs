//! Руины старого мира — недельное подземелье на 10 этажей.
//!
//! Здоровье отряда переходит из боя в бой, павшие выбывают до конца забега. На каждом этаже —
//! выбор из двух залов с разными условиями; на 5-м и 10-м — страж. Награда за этаж выдаётся раз
//! в неделю (сброс в понедельник), заново спуститься можно сколько угодно. Уровень стражей растёт
//! с прогрессом главы. Настройки — `data/ruins.json`.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::Screen;
use crate::battle_screen::{BattleScreen, CustomFight, FightKind};
use crate::events::{Bundle, days_left, week, week_left};
use crate::menu::{PARCHMENT, Voyage, pressable, text};
use crate::story::VoyageRequest;
use crate::style::{self, Look, Skin};
use crate::summary::{Summary, Tone};
use crate::view::Fonts;

#[derive(Resource, Deserialize)]
pub struct RuinsData {
    pub name: String,
    /// Руины открываются, когда пройдено столько этапов.
    pub unlock_cleared: usize,
    pub base_level: u32,
    /// Насколько уровень стражей растёт за каждый пройденный этап главы.
    pub level_per_stage: f32,
    pub floors: Vec<FloorDef>,
    pub rooms: Vec<Room>,
    pub boss_room: Room,
    pub reward: RewardRule,
}

#[derive(Deserialize)]
pub struct FloorDef {
    pub enemies: Vec<String>,
    /// Поправка уровня этажа к общему.
    #[serde(default)]
    pub add: i32,
    /// Имя стража: этаж без выбора залов.
    #[serde(default)]
    pub boss: Option<String>,
}

#[derive(Deserialize, Clone, Default)]
#[serde(default)]
pub struct Room {
    pub id: String,
    pub name: String,
    pub about: String,
    /// Лечение выживших перед боем (доля от максимума).
    pub heal: f32,
    pub level_add: u32,
    /// Множитель добычи; 0 — как 1.
    pub reward_times: u32,
    /// Залп заряжен с первого хода.
    pub charged: bool,
}

#[derive(Deserialize)]
pub struct RewardRule {
    pub silver: u32,
    pub silver_per_floor: u32,
    pub details: u32,
    pub details_per_floor: u32,
    pub charts_every: u32,
    pub charts: u32,
    pub boss_figureheads: u32,
    pub last_doubloons: u32,
}

impl RuinsData {
    pub fn bundled() -> Self {
        serde_json::from_str(include_str!("../../../data/ruins.json"))
            .expect("data/ruins.json корректен")
    }

    pub fn depth(&self) -> u32 {
        self.floors.len() as u32
    }

    fn floor(&self, floor: u32) -> &FloorDef {
        &self.floors[(floor - 1) as usize]
    }

    /// Уровень стражей этажа при `cleared` пройденных этапах главы.
    pub fn level(&self, cleared: usize, floor: u32) -> u32 {
        let base = self.base_level as f32 + self.level_per_stage * cleared as f32;
        (base.round() as i32 + self.floor(floor).add).max(1) as u32
    }

    /// Залы на выбор: на этаже стража — один зал стража, иначе два разных, свои на каждой неделе.
    pub fn rooms(&self, week: u64, floor: u32) -> Vec<&Room> {
        if self.floor(floor).boss.is_some() {
            return vec![&self.boss_room];
        }
        let n = self.rooms.len() as u64;
        let first = (week * 7 + floor as u64 * 3) % n;
        let second = (first + 1 + (week + floor as u64) % (n - 1)) % n;
        vec![&self.rooms[first as usize], &self.rooms[second as usize]]
    }

    /// Награда этажа без множителя зала.
    pub fn reward(&self, floor: u32) -> Bundle {
        let r = &self.reward;
        let def = self.floor(floor);
        Bundle {
            silver: r.silver + r.silver_per_floor * floor,
            details: r.details + r.details_per_floor * floor,
            charts: if floor.is_multiple_of(r.charts_every) {
                r.charts
            } else {
                0
            },
            figureheads: if def.boss.is_some() {
                r.boss_figureheads
            } else {
                0
            },
            doubloons: if floor == self.depth() {
                r.last_doubloons
            } else {
                0
            },
            ..default()
        }
    }

    pub fn room_reward(&self, floor: u32, room: &Room) -> Bundle {
        self.reward(floor).times(room.reward_times.max(1))
    }

    pub fn enemies(&self, cleared: usize, floor: u32, room: &Room) -> Vec<(String, u32)> {
        let level = self.level(cleared, floor) + room.level_add;
        self.floor(floor)
            .enemies
            .iter()
            .map(|id| (id.clone(), level))
            .collect()
    }

    /// Бой в зале: здоровье после лечения зала, павшие так и остаются павшими.
    pub fn fight(
        &self,
        cleared: usize,
        week: u64,
        floor: u32,
        choice: usize,
        hp: &[(String, f32)],
    ) -> CustomFight {
        let room = self.rooms(week, floor)[choice];
        let healed = hp
            .iter()
            .map(|(id, f)| {
                let f = if *f > 0.0 {
                    (f + room.heal).min(1.0)
                } else {
                    0.0
                };
                (id.clone(), f)
            })
            .collect();
        let title = match &self.floor(floor).boss {
            Some(boss) => format!("Руины · этаж {floor} · {boss}"),
            None => format!("Руины · этаж {floor} · {}", room.name),
        };
        CustomFight {
            title,
            kind: FightKind::Ruins(floor, choice),
            enemies: self.enemies(cleared, floor, room),
            silver: 0,
            planks: 0,
            journals: 0,
            energy_rate: 1,
            ally_hp: healed,
            charged: room.charged,
            boost: Default::default(),
            wind: None,
        }
    }
}

/// Недельный прогресс Руин (сохраняется).
#[derive(Resource, Clone, Default, Serialize, Deserialize)]
pub struct Ruins {
    pub week: u64,
    /// Глубина, за которую на этой неделе уже выдана награда.
    pub best: u32,
    pub run: Option<RuinsRun>,
}

/// Идущий спуск: следующий этаж и здоровье героев (доля от максимума, 0 — пал).
#[derive(Clone, Serialize, Deserialize)]
pub struct RuinsRun {
    pub floor: u32,
    pub hp: Vec<(String, f32)>,
}

impl Ruins {
    pub fn roll_week(&mut self) {
        let w = week();
        if self.week != w {
            *self = Self {
                week: w,
                ..default()
            };
        }
    }

    /// Этаж взят: здоровье — дальше вниз, награда — если этаж новый за неделю.
    pub fn win(
        &mut self,
        floor: u32,
        choice: usize,
        hp: Vec<(String, f32)>,
        data: &RuinsData,
        profile: &mut crate::menu::Profile,
    ) -> Summary {
        self.roll_week();
        let room = data.rooms(self.week, floor)[choice];
        let mut sum = Summary::new(format!("Этаж {floor} взят!")).team(&hp);
        if floor > self.best {
            self.best = floor;
            let reward = data.room_reward(floor, room);
            reward.put(profile);
            sum = sum.bundle(&reward);
        }
        if floor >= data.depth() {
            self.run = None;
            sum = sum.chip("Руины пройдены до самого сердца!", Tone::Gold);
        } else if hp.iter().all(|(_, f)| *f <= 0.0) {
            self.run = None;
        } else {
            self.run = Some(RuinsRun {
                floor: floor + 1,
                hp,
            });
        }
        sum
    }

    pub fn lose(&mut self, floor: u32) -> Summary {
        self.roll_week();
        self.run = None;
        Summary::new(format!("Отряд пал на этаже {floor}"))
            .chip(format!("глубина недели: {}", self.best), Tone::Info)
            .chip("спуститься снова можно сразу", Tone::Info)
    }
}

pub fn plugin(app: &mut App) {
    app.insert_resource(RuinsData::bundled())
        .init_resource::<Ruins>();
}

/// Строка героев с полосками здоровья; павшие — тусклые.
pub fn team_hp(
    parent: &mut ChildSpawnerCommands,
    assets: &AssetServer,
    fonts: &Fonts,
    hp: &[(String, f32)],
) {
    parent
        .spawn(Node {
            column_gap: px(10),
            ..default()
        })
        .with_children(|row| {
            for (id, f) in hp {
                row.spawn(Node {
                    flex_direction: FlexDirection::Column,
                    align_items: AlignItems::Center,
                    row_gap: px(3),
                    ..default()
                })
                .with_children(|c| {
                    c.spawn(style::medallion(assets, id, 64.0, *f <= 0.0));
                    c.spawn((
                        Node {
                            width: px(60),
                            height: px(8),
                            border_radius: BorderRadius::all(px(4)),
                            ..default()
                        },
                        BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.6)),
                    ))
                    .with_child((
                        Node {
                            width: percent(f * 100.0),
                            height: percent(100),
                            border_radius: BorderRadius::all(px(4)),
                            ..default()
                        },
                        BackgroundColor(if *f > 0.5 {
                            Color::srgb(0.35, 0.8, 0.35)
                        } else {
                            Color::srgb(0.9, 0.55, 0.2)
                        }),
                    ));
                    let label = if *f > 0.0 {
                        format!("{}%", (f * 100.0).round())
                    } else {
                        "пал".to_string()
                    };
                    c.spawn(text(fonts, label, true, 12.0, PARCHMENT));
                });
            }
        });
}

/// Раздел Руин в Активностях.
#[allow(clippy::too_many_arguments)]
pub fn section(
    col: &mut ChildSpawnerCommands,
    assets: &AssetServer,
    fonts: &Fonts,
    data: &RuinsData,
    ruins: &Ruins,
    screen: &BattleScreen,
    cleared: usize,
    team: &[String],
) {
    let w = week();
    let best = if ruins.week == w { ruins.best } else { 0 };
    let run = ruins.run.as_ref().filter(|_| ruins.week == w);
    style::banner(
        col,
        assets,
        fonts,
        "ruins",
        format!("Глубина {best} из {}", data.depth()),
        &[(
            format!("новые залы через {}", days_left(week_left())),
            Color::srgb(0.45, 0.18, 0.14),
        )],
    );
    if cleared < data.unlock_cleared {
        tile(col).with_children(|card| {
            card.spawn(text(
                fonts,
                "Вход в Руины откроется после главы 1: пройди Грозового левиафана",
                true,
                16.0,
                PARCHMENT,
            ));
        });
        return;
    }
    match run {
        // Спуск начинается с «Войти в зал» на первом этаже — всем отрядом, с полным здоровьем.
        None => {
            let full: Vec<(String, f32)> = team.iter().map(|id| (id.clone(), 1.0)).collect();
            floor_rooms(col, assets, fonts, data, screen, cleared, w, 1, best, &full);
        }
        Some(r) => {
            tile(col).with_children(|card| {
                team_hp(card, assets, fonts, &r.hp);
                retreat_button(card, fonts);
            });
            floor_rooms(
                col, assets, fonts, data, screen, cleared, w, r.floor, best, &r.hp,
            );
        }
    }
}

/// Карточки залов этажа: враги, награда, «Войти в зал» с этим здоровьем отряда.
#[allow(clippy::too_many_arguments)]
fn floor_rooms(
    col: &mut ChildSpawnerCommands,
    assets: &AssetServer,
    fonts: &Fonts,
    data: &RuinsData,
    screen: &BattleScreen,
    cleared: usize,
    week: u64,
    floor: u32,
    best: u32,
    hp: &[(String, f32)],
) {
    let boss = data.floor(floor).boss.clone();
    for (choice, room) in data.rooms(week, floor).into_iter().enumerate() {
        col.spawn((
            Node {
                padding: UiRect::axes(px(16), px(12)),
                flex_direction: FlexDirection::Column,
                row_gap: px(6),
                border_radius: BorderRadius::all(px(12)),
                ..default()
            },
            Skin(Look::Parchment),
            Outline::new(px(2), px(0), Color::srgb(0.72, 0.55, 0.26)),
        ))
        .with_children(|card| {
            let title = match &boss {
                Some(b) => format!("Этаж {floor} · {b}"),
                None => format!("Этаж {floor} · {}", room.name),
            };
            card.spawn(style::ink(fonts, title, 19.0, style::INK));
            card.spawn(text(
                fonts,
                room.about.clone(),
                false,
                13.0,
                style::INK_SOFT,
            ));
            let enemies = data.enemies(cleared, floor, room);
            let ids: Vec<String> = enemies.iter().map(|(id, _)| id.clone()).collect();
            crate::map::enemies_row(card, assets, fonts, &ids, None, style::INK);
            let reward = data.room_reward(floor, room);
            card.spawn(Node {
                column_gap: px(8),
                align_items: AlignItems::Center,
                ..default()
            })
            .with_children(|row| {
                if floor > best {
                    row.spawn(text(fonts, "награда", false, 13.0, style::INK_SOFT));
                    style::costs(row, assets, fonts, &reward.items(), 15.0, style::INK);
                }
            });
            enter_button(card, fonts, floor, choice, hp.to_vec());
        });
    }
    let _ = screen;
}

fn tile<'a>(col: &'a mut ChildSpawnerCommands) -> EntityCommands<'a> {
    col.spawn((
        Node {
            padding: UiRect::axes(px(16), px(14)),
            flex_direction: FlexDirection::Column,
            row_gap: px(8),
            border: UiRect::all(px(2)),
            border_radius: BorderRadius::all(px(14)),
            ..default()
        },
        Skin(Look::Tile),
    ))
}

fn wide_button<'a>(
    card: &'a mut ChildSpawnerCommands,
    fonts: &Fonts,
    label: &str,
    look: Look,
) -> EntityCommands<'a> {
    let mut b = card.spawn((
        Node {
            height: px(50),
            padding: UiRect::horizontal(px(14)),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            border: UiRect::all(px(2)),
            border_radius: BorderRadius::all(px(12)),
            ..default()
        },
        Skin(look),
    ));
    b.with_child(text(fonts, label, true, 18.0, Color::WHITE));
    pressable(&mut b);
    b
}

fn retreat_button(card: &mut ChildSpawnerCommands, fonts: &Fonts) {
    wide_button(card, fonts, "Отступить наверх", Look::Off).observe(
        |_: On<PointerClick>,
         mut ruins: ResMut<Ruins>,
         mut dirty: ResMut<crate::abyss::ActivitiesDirty>| {
            ruins.run = None;
            dirty.0 = true;
        },
    );
}

fn enter_button(
    card: &mut ChildSpawnerCommands,
    fonts: &Fonts,
    floor: u32,
    choice: usize,
    hp: Vec<(String, f32)>,
) {
    wide_button(card, fonts, "Войти в зал", Look::Go).observe(
        move |_: On<PointerClick>,
              data: Res<RuinsData>,
              mut ruins: ResMut<Ruins>,
              progress: Res<crate::menu::Progress>,
              mut voyage: ResMut<Voyage>,
              mut requests: MessageWriter<VoyageRequest>| {
            // Первый зал — это и начало спуска.
            ruins.roll_week();
            if ruins.run.is_none() {
                ruins.run = Some(RuinsRun {
                    floor,
                    hp: hp.clone(),
                });
            }
            *voyage = Voyage {
                stage: None,
                custom: Some(data.fight(progress.cleared, ruins.week, floor, choice, &hp)),
                from: Screen::Activities,
            };
            requests.write(VoyageRequest);
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::menu::{MenuData, Profile};

    #[test]
    fn rooms_differ_and_bosses_stand_alone() {
        let data = RuinsData::bundled();
        for week in 0..20 {
            for floor in 1..=data.depth() {
                let rooms = data.rooms(week, floor);
                if data.floor(floor).boss.is_some() {
                    assert_eq!(rooms.len(), 1);
                } else {
                    assert_eq!(rooms.len(), 2);
                    assert_ne!(rooms[0].id, rooms[1].id, "неделя {week}, этаж {floor}");
                }
            }
        }
    }

    #[test]
    fn floor_reward_paid_once_a_week_and_hp_carries() {
        let data = RuinsData::bundled();
        let mut ruins = Ruins::default();
        let mut p = Profile::new(&MenuData::bundled().start);
        ruins.roll_week();
        let before = p.details;
        ruins.win(
            1,
            0,
            vec![("ren".into(), 0.4), ("mei".into(), 0.0)],
            &data,
            &mut p,
        );
        assert!(p.details > before);
        let run = ruins.run.clone().expect("спуск идёт дальше");
        assert_eq!(run.floor, 2);
        assert_eq!(run.hp[0].1, 0.4);
        // Тот же этаж второй раз за неделю — без награды.
        let again = p.details;
        ruins.win(1, 0, run.hp.clone(), &data, &mut p);
        assert_eq!(p.details, again);
        // Павший не встаёт от родника, живой лечится.
        let spring = data.rooms.iter().position(|r| r.heal > 0.0).unwrap();
        let week = (0..50)
            .find(|w| data.rooms(*w, 2).iter().any(|r| r.heal > 0.0))
            .unwrap();
        let choice = data
            .rooms(week, 2)
            .iter()
            .position(|r| r.id == data.rooms[spring].id)
            .unwrap();
        let fight = data.fight(10, week, 2, choice, &run.hp);
        assert!(fight.ally_hp[0].1 > 0.4);
        assert_eq!(fight.ally_hp[1].1, 0.0);
    }

    #[test]
    fn deeper_and_later_is_harder() {
        let data = RuinsData::bundled();
        assert!(data.level(20, 1) > data.level(10, 1));
        assert!(data.reward(10).doubloons > 0);
        assert!(data.reward(5).figureheads > 0);
    }
}
