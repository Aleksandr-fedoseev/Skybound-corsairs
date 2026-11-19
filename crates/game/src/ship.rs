//! Корабль-база: отсеки с уровнями, стройка по таймеру и то, что отсеки дают игре.
//!
//! Правила из концепта: уровень отсека не выше Корпуса; для Корпуса N нужны Трюм и Пушечная
//! палуба уровня N−1; один слот стройки; ускорение — за дублоны. Каждый отсек меняет бой
//! или сессию: залп, вместимость, доход эфира и журналов, бесплатные наймы.

use std::collections::HashMap;

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::Screen;
use crate::audio::Sfx;
use crate::menu::{
    BRASS, GO, GO_EDGE, GOLD, MUTED, MenuRebuild, PARCHMENT, Profile, Toast, pressable, text,
};
use crate::view::Fonts;

const DISABLED: Color = Color::srgb(0.36, 0.38, 0.43);
const LACKING: Color = Color::srgb(1.0, 0.55, 0.45);
const BETTER: Color = Color::srgb(0.55, 0.88, 0.55);
const HULL: &str = "hull";

/// Данные корабля: `data/ship.json`.
#[derive(Resource, Deserialize)]
pub struct ShipData {
    pub max_level: u32,
    pub speedup_doubloons_per_minute: u32,
    /// Корпус дороже обычного отсека во столько раз.
    pub hull_cost_factor: f32,
    /// Цена и время перехода на уровень `level` (1 — постройка отсека).
    pub levels: Vec<LevelCost>,
    pub ship_art: Vec<ShipArt>,
    pub compartments: Vec<Compartment>,
}

#[derive(Deserialize, Clone, Copy)]
pub struct LevelCost {
    pub level: u32,
    pub seconds: f32,
    pub silver: u32,
    pub planks: u32,
    pub ether: u32,
}

#[derive(Deserialize)]
pub struct ShipArt {
    pub from_hull: u32,
    pub image: String,
}

#[derive(Deserialize)]
pub struct Compartment {
    pub id: String,
    pub name: String,
    /// Подпись под иконкой, если полное имя не влезает.
    #[serde(default)]
    pub short: Option<String>,
    pub icon: String,
    /// С какого уровня Корпуса отсек можно построить.
    pub unlock_hull: u32,
    /// Уровень в начале игры (0 — ещё не построен).
    pub start_level: u32,
    /// Экран, который открывает отсек (`"tavern"`).
    #[serde(default)]
    pub opens: Option<String>,
    /// Что даёт отсек; без него отсек пока только на картинке.
    #[serde(default)]
    pub effect: Option<Effect>,
}

/// Польза отсека по уровням: значение `values[уровень − 1]`.
#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Effect {
    Hull,
    Broadside {
        values: Vec<u32>,
    },
    Hold {
        silver: Vec<u32>,
        planks: Vec<u32>,
        ether: Vec<u32>,
    },
    EtherPerMinute {
        values: Vec<u32>,
    },
    FreeHires {
        values: Vec<u32>,
    },
    JournalsPerMinute {
        values: Vec<u32>,
    },
}

/// Значение по уровню: на уровне 0 (не построен) — ноль, выше таблицы — последнее.
fn at(values: &[u32], level: u32) -> u32 {
    if level == 0 {
        return 0;
    }
    values
        .get(level as usize - 1)
        .or(values.last())
        .copied()
        .unwrap_or(0)
}

/// Вместимость Трюма.
#[derive(Clone, Copy)]
pub struct Capacity {
    pub silver: u32,
    pub planks: u32,
    pub ether: u32,
}

impl ShipData {
    pub fn bundled() -> Self {
        serde_json::from_str(include_str!("../../../data/ship.json"))
            .expect("data/ship.json корректен")
    }

    pub fn compartment(&self, id: &str) -> Option<&Compartment> {
        self.compartments.iter().find(|c| c.id == id)
    }

    pub fn ship_image(&self, hull: u32) -> &str {
        self.ship_art
            .iter()
            .rfind(|art| art.from_hull <= hull)
            .or(self.ship_art.first())
            .map_or("", |art| art.image.as_str())
    }

    /// Цена перехода отсека `id` на уровень `to`.
    pub fn cost(&self, id: &str, to: u32) -> Option<LevelCost> {
        let mut cost = *self.levels.iter().find(|l| l.level == to)?;
        if id == HULL {
            let k = self.hull_cost_factor;
            cost.seconds *= k;
            cost.silver = (cost.silver as f32 * k).round() as u32;
            cost.planks = (cost.planks as f32 * k).round() as u32;
            cost.ether = (cost.ether as f32 * k).round() as u32;
        }
        Some(cost)
    }
}

/// Идущая стройка.
#[derive(Clone, Serialize, Deserialize)]
pub struct Build {
    pub id: String,
    pub to: u32,
    pub left: f32,
}

/// Уровни отсеков и стройка. Пока на клиенте; сохраняются в файл (`save.rs`).
#[derive(Resource, Clone, Serialize, Deserialize)]
pub struct Ship {
    levels: HashMap<String, u32>,
    pub build: Option<Build>,
    /// Журналы Тренировочной палубы, которые ещё не дошли до целого.
    journals_pending: f32,
}

impl Ship {
    pub fn new(data: &ShipData) -> Self {
        Self {
            levels: data
                .compartments
                .iter()
                .map(|c| (c.id.clone(), c.start_level))
                .collect(),
            build: None,
            journals_pending: 0.0,
        }
    }

    pub fn level(&self, id: &str) -> u32 {
        self.levels.get(id).copied().unwrap_or(0)
    }

    pub fn hull(&self) -> u32 {
        self.level(HULL).max(1)
    }

    /// Закончить стройку: отсек получает уровень. Возвращает, что достроено.
    pub fn finish_build(&mut self) -> Option<(String, u32)> {
        let build = self.build.take()?;
        self.levels.insert(build.id.clone(), build.to);
        Some((build.id, build.to))
    }

    /// Тестовая клавиша: следующий уровень Корпуса без стройки (1 → 7 → 1).
    #[cfg(feature = "dev")]
    pub fn dev_next_hull(&mut self) {
        let next = self.hull() % 7 + 1;
        self.levels.insert(HULL.into(), next);
    }

    fn value(&self, data: &ShipData, pick: impl Fn(&Effect) -> Option<&Vec<u32>>) -> u32 {
        data.compartments
            .iter()
            .filter_map(|c| {
                let values = c.effect.as_ref().and_then(&pick)?;
                Some(at(values, self.level(&c.id)))
            })
            .sum()
    }

    pub fn broadside(&self, data: &ShipData) -> u32 {
        self.value(data, |e| match e {
            Effect::Broadside { values } => Some(values),
            _ => None,
        })
    }

    pub fn ether_per_minute(&self, data: &ShipData) -> u32 {
        self.value(data, |e| match e {
            Effect::EtherPerMinute { values } => Some(values),
            _ => None,
        })
    }

    pub fn journals_per_minute(&self, data: &ShipData) -> u32 {
        self.value(data, |e| match e {
            Effect::JournalsPerMinute { values } => Some(values),
            _ => None,
        })
    }

    pub fn free_hires(&self, data: &ShipData) -> u32 {
        self.value(data, |e| match e {
            Effect::FreeHires { values } => Some(values),
            _ => None,
        })
    }

    pub fn capacity(&self, data: &ShipData) -> Capacity {
        let level = self.level("hold");
        match data.compartment("hold").and_then(|c| c.effect.as_ref()) {
            Some(Effect::Hold {
                silver,
                planks,
                ether,
            }) => Capacity {
                silver: at(silver, level),
                planks: at(planks, level),
                ether: at(ether, level),
            },
            _ => Capacity {
                silver: u32::MAX,
                planks: u32::MAX,
                ether: u32::MAX,
            },
        }
    }

    /// Почему нельзя улучшить отсек прямо сейчас (не считая ресурсов), или цена.
    pub fn next_step(&self, data: &ShipData, id: &str) -> Result<LevelCost, String> {
        let Some(compartment) = data.compartment(id) else {
            return Err("Нет такого отсека".into());
        };
        if compartment.effect.is_none() {
            return Err("Отсек появится в следующих обновлениях".into());
        }
        let hull = self.hull();
        if compartment.unlock_hull > hull {
            return Err(format!("Откроется на Корпусе {}", compartment.unlock_hull));
        }
        let to = self.level(id) + 1;
        if to > data.max_level {
            return Err("Максимальный уровень".into());
        }
        if id == HULL {
            let need = to - 1;
            if self.level("cannon_deck") < need || self.level("hold") < need {
                return Err(format!("Нужны Пушечная палуба и Трюм ур. {need}"));
            }
        } else if to > hull {
            return Err(format!("Нужен Корпус ур. {to}"));
        }
        if let Some(build) = &self.build {
            let name = data.compartment(&build.id).map_or("", |c| c.name.as_str());
            return Err(format!("Слот стройки занят: {name}"));
        }
        data.cost(id, to).ok_or_else(|| "Нет цены уровня".into())
    }
}

/// Отсек, открытый в панели на корабле.
#[derive(Resource, Default)]
pub struct OpenCompartment(pub Option<String>);

#[derive(Component)]
struct BuildTimer;

pub fn plugin(app: &mut App) {
    let data = ShipData::bundled();
    app.insert_resource(Ship::new(&data))
        .insert_resource(data)
        .init_resource::<OpenCompartment>()
        .add_systems(Update, (tick_build, income, refresh_timer));
}

pub fn clock(seconds: f32) -> String {
    let s = seconds.ceil().max(0.0) as u32;
    let (h, m, s) = (s / 3600, s / 60 % 60, s % 60);
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}

/// Стройка идёт всегда, даже во время боя.
fn tick_build(
    time: Res<Time>,
    data: Res<ShipData>,
    mut ship: ResMut<Ship>,
    mut toast: ResMut<Toast>,
    mut rebuild: ResMut<MenuRebuild>,
    mut sfx: MessageWriter<Sfx>,
) {
    let Some(build) = &mut ship.build else {
        return;
    };
    build.left -= time.delta_secs();
    if build.left > 0.0 {
        return;
    }
    let Some((id, to)) = ship.finish_build() else {
        return;
    };
    let name = data.compartment(&id).map_or("", |c| c.name.as_str());
    toast.show(&format!("Готово: {name} — уровень {to}"));
    sfx.write(Sfx("done"));
    rebuild.0 = true;
}

/// Ветроловы копят эфир (собирается тапом), Тренировочная палуба — журналы (сразу в запас).
fn income(
    time: Res<Time>,
    data: Res<ShipData>,
    mut ship: ResMut<Ship>,
    mut profile: ResMut<Profile>,
) {
    let minutes = time.delta_secs() / 60.0;
    let cap = ship.capacity(&data).ether as f32;
    let ether = ship.ether_per_minute(&data) as f32;
    profile.ether_pending = (profile.ether_pending + ether * minutes).min(cap);
    ship.journals_pending += ship.journals_per_minute(&data) as f32 * minutes;
    let whole = ship.journals_pending.floor();
    if whole >= 1.0 {
        ship.journals_pending -= whole;
        profile.journals += whole as u32;
    }
}

fn refresh_timer(ship: Res<Ship>, mut texts: Query<&mut Text, With<BuildTimer>>) {
    let Some(build) = &ship.build else {
        return;
    };
    for mut text in &mut texts {
        text.0 = format!("Строится: осталось {}", clock(build.left));
    }
}

/// Строка «что даёт отсек» на уровне `level`.
fn effect_line(effect: &Effect, level: u32) -> String {
    match effect {
        Effect::Hull => format!("Потолок отсеков: ур. {level}"),
        Effect::Broadside { values } => format!("Сила залпа в бою: {}", at(values, level)),
        Effect::Hold {
            silver,
            planks,
            ether,
        } => format!(
            "Вмещает: серебро {} · доски {} · эфир {}",
            at(silver, level),
            at(planks, level),
            at(ether, level)
        ),
        Effect::EtherPerMinute { values } => format!("Эфир: {} в минуту", at(values, level)),
        Effect::FreeHires { values } => {
            format!("Бесплатных наймов в день: {}", at(values, level))
        }
        Effect::JournalsPerMinute { values } => {
            format!("Судовые журналы: {} в минуту", at(values, level))
        }
    }
}

/// Панель отсека поверх корабля: польза сейчас и на следующем уровне, цена, кнопки.
pub fn panel(
    root: &mut ChildSpawnerCommands,
    fonts: &Fonts,
    data: &ShipData,
    ship: &Ship,
    profile: &Profile,
    id: &str,
) {
    let Some(compartment) = data.compartment(id) else {
        return;
    };
    let level = ship.level(id);
    let step = ship.next_step(data, id);
    let building_here = ship.build.as_ref().is_some_and(|b| b.id == id);

    root.spawn((
        Node {
            position_type: PositionType::Absolute,
            left: px(10),
            // Над рядом отсеков, чтобы можно было переключаться между ними.
            bottom: px(272),
            width: px(520),
            padding: UiRect::all(px(14)),
            flex_direction: FlexDirection::Column,
            row_gap: px(6),
            border: UiRect::all(px(2)),
            border_radius: BorderRadius::all(px(14)),
            ..default()
        },
        BackgroundColor(Color::srgb(0.11, 0.075, 0.05)),
        BorderColor::all(BRASS),
        GlobalZIndex(10),
    ))
    .with_children(|p| {
        p.spawn(Node {
            justify_content: JustifyContent::SpaceBetween,
            align_items: AlignItems::Center,
            ..default()
        })
        .with_children(|row| {
            row.spawn(text(
                fonts,
                if level == 0 {
                    format!("{} · не построен", compartment.name)
                } else {
                    format!("{} · ур. {level}", compartment.name)
                },
                true,
                20.0,
                PARCHMENT,
            ));
            let mut close = row.spawn((
                Node {
                    width: px(44),
                    height: px(36),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    border_radius: BorderRadius::all(px(8)),
                    ..default()
                },
                BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.35)),
            ));
            close.with_child(text(fonts, "x", true, 18.0, MUTED));
            pressable(&mut close);
            close.observe(
                |_: On<PointerClick>,
                 mut open: ResMut<OpenCompartment>,
                 mut rebuild: ResMut<MenuRebuild>| {
                    open.0 = None;
                    rebuild.0 = true;
                },
            );
        });

        if let Some(effect) = &compartment.effect {
            if level > 0 {
                p.spawn(text(
                    fonts,
                    format!("Сейчас: {}", effect_line(effect, level)),
                    false,
                    14.0,
                    PARCHMENT,
                ));
            }
            if level < data.max_level {
                p.spawn(text(
                    fonts,
                    format!("На ур. {}: {}", level + 1, effect_line(effect, level + 1)),
                    false,
                    14.0,
                    BETTER,
                ));
            }
        }

        if building_here {
            p.spawn((BuildTimer, text(fonts, "", true, 15.0, GOLD)));
        }
        match &step {
            Ok(cost) => {
                let lack = |have: u32, need: u32| if have >= need { PARCHMENT } else { LACKING };
                p.spawn(Node {
                    column_gap: px(14),
                    margin: UiRect::top(px(4)),
                    ..default()
                })
                .with_children(|row| {
                    row.spawn(text(
                        fonts,
                        format!("серебро {}", cost.silver),
                        true,
                        14.0,
                        lack(profile.silver, cost.silver),
                    ));
                    row.spawn(text(
                        fonts,
                        format!("доски {}", cost.planks),
                        true,
                        14.0,
                        lack(profile.planks, cost.planks),
                    ));
                    if cost.ether > 0 {
                        row.spawn(text(
                            fonts,
                            format!("эфир {}", cost.ether),
                            true,
                            14.0,
                            lack(profile.ether, cost.ether),
                        ));
                    }
                    row.spawn(text(
                        fonts,
                        format!("время {}", clock(cost.seconds)),
                        false,
                        14.0,
                        MUTED,
                    ));
                });
            }
            Err(reason) if !building_here => {
                p.spawn(text(fonts, reason.clone(), false, 14.0, MUTED));
            }
            Err(_) => {}
        }

        p.spawn(Node {
            margin: UiRect::top(px(6)),
            column_gap: px(10),
            ..default()
        })
        .with_children(|buttons| {
            if building_here {
                let left = ship.build.as_ref().map_or(0.0, |b| b.left);
                let price = speedup_price(data, left);
                action_button(
                    buttons,
                    fonts,
                    "Ускорить",
                    format!("{price} дублонов"),
                    profile.doubloons >= price,
                    Action::Speedup,
                );
            } else {
                let ready = step.as_ref().is_ok_and(|c| {
                    profile.silver >= c.silver
                        && profile.planks >= c.planks
                        && profile.ether >= c.ether
                });
                action_button(
                    buttons,
                    fonts,
                    if level == 0 {
                        "Построить"
                    } else {
                        "Улучшить"
                    },
                    match &step {
                        Ok(_) => format!("до ур. {}", level + 1),
                        Err(_) => "недоступно".to_string(),
                    },
                    ready,
                    Action::Upgrade(id.to_string()),
                );
            }
            if compartment.opens.as_deref() == Some("tavern") && level > 0 {
                action_button(
                    buttons,
                    fonts,
                    "В Таверну",
                    "нанять героев".to_string(),
                    true,
                    Action::Open(Screen::Tavern),
                );
            }
        });
    });
}

pub fn speedup_price(data: &ShipData, left: f32) -> u32 {
    (left / 60.0).ceil().max(1.0) as u32 * data.speedup_doubloons_per_minute
}

#[derive(Clone)]
enum Action {
    Upgrade(String),
    Speedup,
    Open(Screen),
}

fn action_button(
    buttons: &mut ChildSpawnerCommands,
    fonts: &Fonts,
    title: &str,
    sub: String,
    ready: bool,
    action: Action,
) {
    let mut button = buttons.spawn((
        Node {
            flex_grow: 1.0,
            flex_basis: px(0),
            height: px(62),
            flex_direction: FlexDirection::Column,
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            border: UiRect::all(px(2)),
            border_radius: BorderRadius::all(px(12)),
            ..default()
        },
        BackgroundColor(if ready { GO } else { DISABLED }),
        BorderColor::all(if ready { GO_EDGE } else { MUTED }),
    ));
    button.with_children(|b| {
        b.spawn(text(fonts, title, true, 18.0, Color::WHITE));
        b.spawn(text(fonts, sub, false, 12.0, PARCHMENT));
    });
    pressable(&mut button);
    button.observe(move |_: On<PointerClick>, mut commands: Commands| {
        commands.run_system_cached_with(act, action.clone());
    });
}

/// Нажатие в панели отсека: начать стройку, ускорить или открыть экран.
#[allow(clippy::too_many_arguments)]
fn act(
    In(action): In<Action>,
    data: Res<ShipData>,
    mut ship: ResMut<Ship>,
    mut profile: ResMut<Profile>,
    mut toast: ResMut<Toast>,
    mut rebuild: ResMut<MenuRebuild>,
    mut next: ResMut<NextState<Screen>>,
    mut sfx: MessageWriter<Sfx>,
) {
    match action {
        Action::Upgrade(id) => match ship.next_step(&data, &id) {
            Err(reason) => toast.show(&reason),
            Ok(cost) => {
                if profile.silver < cost.silver
                    || profile.planks < cost.planks
                    || profile.ether < cost.ether
                {
                    toast.show("Не хватает ресурсов — добудьте в боях и соберите эфир");
                    return;
                }
                profile.silver -= cost.silver;
                profile.planks -= cost.planks;
                profile.ether -= cost.ether;
                ship.build = Some(Build {
                    id,
                    to: cost.level,
                    left: cost.seconds,
                });
                sfx.write(Sfx("hammer"));
                rebuild.0 = true;
            }
        },
        Action::Speedup => {
            let Some(build) = &mut ship.build else {
                return;
            };
            let price = speedup_price(&data, build.left);
            if profile.doubloons < price {
                toast.show(&format!("Не хватает дублонов: нужно {price}"));
                return;
            }
            profile.doubloons -= price;
            // Закончится на следующем кадре в `tick_build`.
            build.left = 0.0;
        }
        Action::Open(screen) => next.set(screen),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hull_needs_cannon_and_hold() {
        let data = ShipData::bundled();
        let mut ship = Ship::new(&data);
        // На старте всё на 1-м уровне: Корпус 2 требует отсеки ур. 1 — можно.
        assert!(ship.next_step(&data, HULL).is_ok());
        ship.levels.insert(HULL.into(), 2);
        // Корпус 3 требует Пушечную палубу и Трюм ур. 2.
        assert!(ship.next_step(&data, HULL).is_err());
        ship.levels.insert("cannon_deck".into(), 2);
        ship.levels.insert("hold".into(), 2);
        assert!(ship.next_step(&data, HULL).is_ok());
    }

    #[test]
    fn compartment_capped_by_hull() {
        let data = ShipData::bundled();
        let mut ship = Ship::new(&data);
        // Пушечная палуба ур. 2 при Корпусе 1 — нельзя.
        assert!(ship.next_step(&data, "cannon_deck").is_err());
        ship.levels.insert(HULL.into(), 2);
        assert!(ship.next_step(&data, "cannon_deck").is_ok());
    }

    #[test]
    fn one_build_slot() {
        let data = ShipData::bundled();
        let mut ship = Ship::new(&data);
        ship.levels.insert(HULL.into(), 3);
        ship.build = Some(Build {
            id: "hold".into(),
            to: 2,
            left: 10.0,
        });
        assert!(ship.next_step(&data, "cannon_deck").is_err());
    }

    #[test]
    fn effects_follow_levels() {
        let data = ShipData::bundled();
        let mut ship = Ship::new(&data);
        let before = ship.broadside(&data);
        ship.levels.insert("cannon_deck".into(), 2);
        assert!(ship.broadside(&data) > before);
        // Тренировочная палуба не построена — журналов нет.
        assert_eq!(ship.journals_per_minute(&data), 0);
    }
}
