//! Корабль-база: отсеки с уровнями, стройка по таймеру и то, что отсеки дают игре.
//!
//! Корабли покупаются на верфи (решения 57–58, 64): у каждого свой ранг (какие отсеки на нём
//! есть) и потолок уровня отсеков. Отсеки и их уровни переезжают на новый корабль. Один слот
//! стройки; ускорение — за дублоны. Каждый отсек меняет бой или сессию: залп, вместимость, доход
//! эфира и журналов, бесплатные наймы.

use std::collections::HashMap;

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::Screen;
use crate::audio::Sfx;
use crate::contracts::Deed;
use crate::menu::{GOLD, MUTED, MenuRebuild, PARCHMENT, Profile, Progress, Toast, pressable, text};
use crate::style::{INK, INK_GOOD, Look as Skins, Skin, heading, icon, ink};
use crate::view::Fonts;

const LACKING: Color = Color::srgb(1.0, 0.45, 0.38);
const BRIGHT: Color = Color::srgb(1.0, 0.93, 0.78);

/// Данные корабля: `data/ship.json`.
#[derive(Resource, Deserialize)]
pub struct ShipData {
    pub max_level: u32,
    pub speedup_doubloons_per_minute: u32,
    /// Цена и время перехода на уровень `level` (1 — постройка отсека).
    pub levels: Vec<LevelCost>,
    pub compartments: Vec<Compartment>,
    /// Корабли верфи; первый — стартовый.
    pub ships: Vec<ShipDef>,
    pub scene: Scene,
}

/// Общее для всех кораблей на главной: небо, облака, птицы, леса стройки, как корабль стоит.
#[derive(Deserialize)]
pub struct Scene {
    pub look: Look,
    /// Панорама неба за кораблём: едет влево по кругу.
    pub backdrop: Backdrop,
    /// Стайка птиц изредка обгоняет корабль.
    #[serde(default)]
    pub birds: Option<Birds>,
    /// Облака фона главной: плывут мимо корабля.
    #[serde(default)]
    pub clouds: Vec<CloudDef>,
    /// Леса на комнате, где идёт стройка.
    pub scaffold: String,
}

/// Корабль верфи. Рисунки — `<art>/exterior.png` (снаружи), `hull.png` (разрез), `locked.png`
/// (разрез с заколоченными комнатами): одна поза, сторона `SHIP_ART` px.
#[derive(Deserialize)]
pub struct ShipDef {
    pub id: String,
    pub name: String,
    /// Строка о корабле на верфи.
    pub about: String,
    /// Отсеки с рангом не выше этого — на корабле.
    pub rank: u32,
    /// Потолок уровня отсеков на этом корабле.
    pub max_level: u32,
    /// Продаётся после этой главы (0 — сразу).
    #[serde(default)]
    pub after_chapter: u32,
    /// Цена: ресурсы (основные корабли) или дублоны (особые). Ни того ни другого — стартовый.
    #[serde(default)]
    pub price: Option<Price>,
    #[serde(default)]
    pub doubloons: Option<u32>,
    pub art: String,
    /// Где корабль на рисунке (x0, y0, x1, y1 в px рисунка): по нему он стоит на главной.
    pub bounds: [f32; 4],
    /// Пузырь эфира: точка на рисунке (над ветроловами).
    pub bubble: [f32; 2],
    /// Паруса снаружи: раскатываются, когда корабль идёт в полёт.
    #[serde(default)]
    pub sails: Option<Sails>,
    /// Пропеллер на корме: крутится всегда.
    #[serde(default)]
    pub propeller: Option<Propeller>,
    /// Комнаты: отсеки и просто жилые (без отсека — для красоты, не нажимаются).
    pub rooms: Vec<Room>,
}

#[derive(Deserialize, Clone, Copy)]
pub struct Price {
    pub silver: u32,
    pub planks: u32,
    pub ether: u32,
}

/// Сторона рисунков корабля (px): рамки комнат даны в его пикселях.
pub const SHIP_ART: f32 = 1024.0;

impl ShipDef {
    pub fn exterior(&self) -> String {
        format!("{}/exterior.png", self.art)
    }
    pub fn hull(&self) -> String {
        format!("{}/hull.png", self.art)
    }
    pub fn locked(&self) -> String {
        format!("{}/locked.png", self.art)
    }
    /// Рамка всех комнат: на осмотре она целиком по ширине экрана.
    pub fn rooms_bounds(&self) -> [f32; 4] {
        self.rooms.iter().fold(
            [f32::MAX, f32::MAX, f32::MIN, f32::MIN],
            |[x0, y0, x1, y1], r| {
                let [x, y, w, h] = r.rect;
                [x0.min(x), y0.min(y), x1.max(x + w), y1.max(y + h)]
            },
        )
    }
}

#[derive(Deserialize)]
pub struct CloudDef {
    pub image: String,
    /// Высота над верхом экрана и ширина (логические px), скорость (px/с), где на пути в начале (0–1).
    pub top: f32,
    pub width: f32,
    pub speed: f32,
    pub phase: f32,
    pub alpha: f32,
    /// Перед кораблём (иначе — за ним).
    #[serde(default)]
    pub front: bool,
}

/// Паруса снаружи: падают с рей и наполняются ветром, когда корабль снова идёт в полёт (при
/// старте и после осмотра), а на осмотре сворачиваются. Каждый следующий — на `stagger` позже.
#[derive(Deserialize)]
pub struct Sails {
    /// Сколько секунд парус раскатывается и сворачивается.
    pub unfurl: f32,
    pub furl: f32,
    pub stagger: f32,
    /// На сколько полос резать парус: каждая колышется сама, волна бежит вниз по полотну.
    pub slices: usize,
    /// Размах ряби у нижней кромки — доля ширины полосы.
    pub ripple: f32,
    pub layers: Vec<SailLayer>,
}

/// Как корабль стоит на экране: снаружи (главная) и на осмотре (весь разрез по ширине экрана).
#[derive(Deserialize)]
pub struct Look {
    /// Сколько секунд длится приближение и отдаление.
    pub seconds: f32,
    /// Снаружи: ширина корабля на экране (px) и куда встаёт его середина.
    pub outside_width: f32,
    pub outside_center: [f32; 2],
    /// На осмотре все комнаты влезают в эту рамку экрана (x0, y0, x1, y1), середина — по её центру.
    pub inside_frame: [f32; 4],
}

/// Кадр корабля: сторона рисунка на экране (px) и в какую точку экрана `center` попадает точка
/// рисунка `focus`.
pub struct Shot {
    pub side: f32,
    pub focus: Vec2,
    pub center: Vec2,
}

impl Look {
    pub fn outside(&self, ship: &ShipDef) -> Shot {
        let [x0, y0, x1, y1] = ship.bounds;
        Shot {
            side: self.outside_width * SHIP_ART / (x1 - x0),
            focus: Vec2::new((x0 + x1) / 2.0, (y0 + y1) / 2.0),
            center: Vec2::from(self.outside_center),
        }
    }

    pub fn inside(&self, ship: &ShipDef) -> Shot {
        let [x0, y0, x1, y1] = ship.rooms_bounds();
        let [fx0, fy0, fx1, fy1] = self.inside_frame;
        let k = ((fx1 - fx0) / (x1 - x0)).min((fy1 - fy0) / (y1 - y0));
        Shot {
            side: k * SHIP_ART,
            focus: Vec2::new((x0 + x1) / 2.0, (y0 + y1) / 2.0),
            center: Vec2::new((fx0 + fx1) / 2.0, (fy0 + fy1) / 2.0),
        }
    }
}

/// Парус: скатка и раскрытый парус в одной рамке (x, y, ширина, высота в px рисунка).
#[derive(Deserialize)]
pub struct SailLayer {
    pub furled: String,
    pub open: String,
    pub rect: [f32; 4],
}

#[derive(Deserialize)]
pub struct Backdrop {
    pub image: String,
    /// Ширина кольца панорамы (логические px при высоте экрана) и скорость (px/с).
    pub width: f32,
    pub speed: f32,
}

#[derive(Deserialize)]
pub struct Birds {
    pub image: String,
    pub count: usize,
    /// Скорость (px/с), раз в сколько секунд пролетает стайка, высота полёта (px от верха).
    pub speed: f32,
    pub every: f32,
    pub top: f32,
}

#[derive(Deserialize)]
pub struct Propeller {
    pub image: String,
    /// Центр втулки и размах — в пикселях рисунка корпуса.
    pub at: [f32; 2],
    pub size: f32,
    pub turns_per_second: f32,
}

/// Комната отсека: рамка на рисунке корпуса и облики по уровням.
#[derive(Deserialize)]
pub struct Room {
    pub id: String,
    /// x, y, ширина, высота — в пикселях рисунка корпуса.
    pub rect: [f32; 4],
    /// С какого уровня какой рисунок; ниже первого — то, что нарисовано на корпусе.
    #[serde(default)]
    pub tiers: Vec<RoomTier>,
    /// Где плашка уровня — левый нижний угол в долях рамки; нет — внизу слева.
    #[serde(default)]
    pub tag: [f32; 2],
    /// Комната видна и снаружи (над палубой): её облик рисуется и на главной.
    #[serde(default)]
    pub outside: bool,
}

#[derive(Deserialize)]
pub struct RoomTier {
    pub from: u32,
    pub image: String,
}

impl Room {
    /// Рисунок комнаты на этом уровне; `None` — облик с корпуса.
    pub fn image(&self, level: u32) -> Option<&str> {
        self.tiers
            .iter()
            .filter(|t| level >= t.from)
            .max_by_key(|t| t.from)
            .map(|t| t.image.as_str())
    }
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
pub struct Compartment {
    pub id: String,
    pub name: String,
    /// Подпись под иконкой, если полное имя не влезает.
    #[serde(default)]
    pub short: Option<String>,
    pub icon: String,
    /// На кораблях какого ранга отсек есть (1 — на любом).
    pub rank: u32,
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
    /// Дополнительные шлюпки для экспедиций.
    Boats {
        values: Vec<u32>,
    },
    /// Сколько кают для «Соседей по каюте».
    Cabins {
        values: Vec<u32>,
    },
    /// Высшая редкость, которую создаёт Мастерская (1 — обычная).
    Workshop {
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

    pub fn ship(&self, id: &str) -> Option<&ShipDef> {
        self.ships.iter().find(|s| s.id == id)
    }

    /// Первый основной корабль нужного ранга — его называем, когда отсеку тесно.
    pub fn ship_of_rank(&self, rank: u32) -> Option<&ShipDef> {
        self.ships
            .iter()
            .find(|s| s.rank >= rank && s.doubloons.is_none())
    }

    /// Цена перехода отсека на уровень `to`.
    pub fn cost(&self, to: u32) -> Option<LevelCost> {
        self.levels.iter().find(|l| l.level == to).copied()
    }
}

/// Идущая стройка.
#[derive(Clone, Serialize, Deserialize)]
pub struct Build {
    pub id: String,
    pub to: u32,
    pub left: f32,
}

/// Уровни отсеков, купленные корабли и стройка. Пока на клиенте; сохраняются в файл (`save.rs`).
#[derive(Resource, Clone, Serialize, Deserialize)]
pub struct Ship {
    levels: HashMap<String, u32>,
    pub build: Option<Build>,
    /// Журналы Тренировочной палубы, которые ещё не дошли до целого.
    journals_pending: f32,
    /// Купленные корабли и тот, на котором команда сейчас.
    #[serde(default)]
    owned: Vec<String>,
    #[serde(default)]
    current: String,
}

/// Старые сохранения: уровень Корпуса превращается в корабли (Корпус 3+ — второй ранг, 6+ — третий).
const HULL: &str = "hull";

impl Ship {
    pub fn new(data: &ShipData) -> Self {
        let mut ship = Self {
            levels: data
                .compartments
                .iter()
                .map(|c| (c.id.clone(), c.start_level))
                .collect(),
            build: None,
            journals_pending: 0.0,
            owned: Vec::new(),
            current: String::new(),
        };
        ship.migrate(data);
        ship
    }

    /// Привести сохранение к кораблям: Корпус — в купленные корабли, пустой список — стартовый.
    pub fn migrate(&mut self, data: &ShipData) {
        if self.build.as_ref().is_some_and(|b| b.id == HULL) {
            self.finish_build();
        }
        if let Some(hull) = self.levels.remove(HULL) {
            let rank = match hull {
                0..=2 => 1,
                3..=5 => 2,
                _ => 3,
            };
            for def in data
                .ships
                .iter()
                .filter(|s| s.doubloons.is_none() && s.rank <= rank)
            {
                if !self.owned.contains(&def.id) {
                    self.owned.push(def.id.clone());
                }
            }
        }
        if let Some(first) = data.ships.first()
            && !self.owned.contains(&first.id)
        {
            self.owned.insert(0, first.id.clone());
        }
        self.owned.retain(|id| data.ship(id).is_some());
        if data.ship(&self.current).is_none() || !self.owned.contains(&self.current) {
            // На лучший из купленных.
            self.current = self
                .owned
                .iter()
                .filter_map(|id| data.ship(id))
                .max_by_key(|s| (s.rank, s.max_level))
                .map(|s| s.id.clone())
                .unwrap_or_default();
        }
    }

    pub fn level(&self, id: &str) -> u32 {
        self.levels.get(id).copied().unwrap_or(0)
    }

    /// Корабль, на котором команда сейчас.
    pub fn current<'a>(&self, data: &'a ShipData) -> &'a ShipDef {
        data.ship(&self.current).unwrap_or(&data.ships[0])
    }

    pub fn owns(&self, id: &str) -> bool {
        self.owned.iter().any(|o| o == id)
    }

    pub fn buy(&mut self, id: &str) {
        if !self.owns(id) {
            self.owned.push(id.to_string());
        }
        self.current = id.to_string();
    }

    /// Перейти на другой купленный корабль.
    pub fn board(&mut self, id: &str) {
        if self.owns(id) {
            self.current = id.to_string();
        }
    }

    /// Закончить стройку: отсек получает уровень. Возвращает, что достроено.
    pub fn finish_build(&mut self) -> Option<(String, u32)> {
        let build = self.build.take()?;
        self.levels.insert(build.id.clone(), build.to);
        Some((build.id, build.to))
    }

    /// Тестовая клавиша: следующий корабль верфи без покупки (по кругу).
    #[cfg(feature = "dev")]
    pub fn dev_next_ship(&mut self, data: &ShipData) {
        let at = data
            .ships
            .iter()
            .position(|s| s.id == self.current)
            .unwrap_or(0);
        let next = &data.ships[(at + 1) % data.ships.len()];
        self.buy(&next.id);
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

    /// Шлюпок сверх базовых (отсек «Шлюпки»).
    pub fn extra_boats(&self, data: &ShipData) -> u32 {
        self.value(data, |e| match e {
            Effect::Boats { values } => Some(values),
            _ => None,
        })
    }

    /// Высшая редкость в Мастерской (индекс), `None` — Мастерская не построена.
    pub fn workshop_rarity(&self, data: &ShipData) -> Option<usize> {
        let v = self.value(data, |e| match e {
            Effect::Workshop { values } => Some(values),
            _ => None,
        });
        (v > 0).then(|| v as usize - 1)
    }

    pub fn cabins(&self, data: &ShipData) -> u32 {
        self.value(data, |e| match e {
            Effect::Cabins { values } => Some(values),
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
        let current = self.current(data);
        if compartment.rank > current.rank {
            return Err(match data.ship_of_rank(compartment.rank) {
                Some(bigger) => format!(
                    "Нет места на «{}» — нужен корабль «{}»",
                    current.name, bigger.name
                ),
                None => format!("Нет места на «{}»", current.name),
            });
        }
        let to = self.level(id) + 1;
        if to > data.max_level {
            return Err("Максимальный уровень".into());
        }
        if to > current.max_level {
            return Err(format!(
                "Потолок «{}» — ур. {}: выше — на корабле крупнее (Верфь)",
                current.name, current.max_level
            ));
        }
        if let Some(build) = &self.build {
            let name = data.compartment(&build.id).map_or("", |c| c.name.as_str());
            return Err(format!("Слот стройки занят: {name}"));
        }
        data.cost(to).ok_or_else(|| "Нет цены уровня".into())
    }
}

/// Отсек, открытый в панели на корабле.
#[derive(Resource, Default)]
pub struct OpenCompartment(pub Option<String>);

#[derive(Component)]
struct BuildTimer;

/// Таймер стройки на комнате корабля: только время.
#[derive(Component)]
pub(crate) struct RoomTimer;

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

fn refresh_timer(
    ship: Res<Ship>,
    mut texts: Query<&mut Text, (With<BuildTimer>, Without<RoomTimer>)>,
    mut rooms: Query<&mut Text, With<RoomTimer>>,
) {
    let Some(build) = &ship.build else {
        return;
    };
    for mut text in &mut texts {
        text.0 = format!("Строится: осталось {}", clock(build.left));
    }
    for mut text in &mut rooms {
        text.0 = clock(build.left);
    }
}

/// Строка «что даёт отсек» на уровне `level`.
fn effect_line(effect: &Effect, level: u32) -> String {
    match effect {
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
        Effect::Boats { values } => {
            format!("Шлюпок для экспедиций: +{}", at(values, level))
        }
        Effect::Cabins { values } => format!("Кают для соседей: {}", at(values, level)),
        Effect::Workshop { values } => {
            let name = ["обычное", "редкое", "эпическое"]
                .get(at(values, level).saturating_sub(1) as usize)
                .copied()
                .unwrap_or("обычное");
            format!("Создаёт снаряжение до редкости: {name}")
        }
    }
}

/// Панель отсека поверх корабля: польза сейчас и на следующем уровне, цена, кнопки.
#[allow(clippy::too_many_arguments)]
pub fn panel(
    root: &mut ChildSpawnerCommands,
    assets: &AssetServer,
    fonts: &Fonts,
    data: &ShipData,
    ship: &Ship,
    profile: &Profile,
    id: &str,
    extra: impl FnOnce(&mut ChildSpawnerCommands),
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
            // Внизу, поверх ряда отсеков и похода: корабль над панелью остаётся виден.
            bottom: px(90),
            width: px(520),
            padding: UiRect::axes(px(32), px(30)),
            flex_direction: FlexDirection::Column,
            row_gap: px(8),
            ..default()
        },
        Skin(Skins::Tile),
        GlobalZIndex(10),
    ))
    .with_children(|p| {
        p.spawn(Node {
            justify_content: JustifyContent::SpaceBetween,
            align_items: AlignItems::Center,
            ..default()
        })
        .with_children(|row| {
            row.spawn(heading(
                fonts,
                if level == 0 {
                    format!("{} · не построен", compartment.name)
                } else {
                    format!("{} · ур. {level}", compartment.name)
                },
                23.0,
                BRIGHT,
            ));
            close_button(row, fonts);
        });

        // Польза отсека сейчас и на следующем уровне — на пергаменте.
        if let Some(effect) = &compartment.effect {
            p.spawn((
                Node {
                    padding: UiRect::axes(px(16), px(12)),
                    flex_direction: FlexDirection::Column,
                    row_gap: px(4),
                    ..default()
                },
                Skin(Skins::Parchment),
            ))
            .with_children(|card| {
                if level > 0 {
                    card.spawn(ink(
                        fonts,
                        format!("Сейчас: {}", effect_line(effect, level)),
                        17.0,
                        INK,
                    ));
                }
                if level < data.max_level.min(ship.current(data).max_level) {
                    card.spawn(ink(
                        fonts,
                        format!("На ур. {}: {}", level + 1, effect_line(effect, level + 1)),
                        17.0,
                        INK_GOOD,
                    ));
                }
            });
        }

        if building_here {
            p.spawn((BuildTimer, text(fonts, "", true, 15.0, GOLD)));
        }
        match &step {
            Ok(cost) => {
                // Цена иконками; чего не хватает — красным.
                let lack = |have: u32, need: u32| if have >= need { BRIGHT } else { LACKING };
                p.spawn(Node {
                    column_gap: px(16),
                    align_items: AlignItems::Center,
                    ..default()
                })
                .with_children(|row| {
                    let mut items = vec![
                        ("silver", cost.silver, lack(profile.silver, cost.silver)),
                        ("planks", cost.planks, lack(profile.planks, cost.planks)),
                    ];
                    if cost.ether > 0 {
                        items.push(("ether", cost.ether, lack(profile.ether, cost.ether)));
                    }
                    for (kind, n, color) in items {
                        row.spawn(Node {
                            column_gap: px(5),
                            align_items: AlignItems::Center,
                            ..default()
                        })
                        .with_children(|item| {
                            item.spawn(icon(assets, kind, 26.0));
                            item.spawn(heading(fonts, n.to_string(), 19.0, color));
                        });
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
        extra(p);
    });
}

/// Сколько глав пройдено целиком: глава N пройдена, когда пройдены все её этапы (`N-…`).
pub fn chapters_done(stages: &[game_data::StageDef], cleared: usize) -> u32 {
    let mut done = 0;
    for n in 1.. {
        let prefix = format!("{n}-");
        let Some(last) = stages.iter().rposition(|s| s.id.starts_with(&prefix)) else {
            break;
        };
        if last >= cleared {
            break;
        }
        done = n;
    }
    done
}

/// Можно ли купить корабль: цена, не купленная ли уже, пройдена ли глава.
fn ship_offer(def: &ShipDef, ship: &Ship, chapters: u32) -> Result<(), String> {
    if ship.owns(&def.id) {
        return Err("уже ваш".into());
    }
    if def.after_chapter > chapters {
        return Err(format!("после главы {}", def.after_chapter));
    }
    Ok(())
}

/// Верфь: все корабли, купленные и нет. Тап по чужому — купить, по своему — перейти на борт.
pub fn shipyard(
    root: &mut ChildSpawnerCommands,
    assets: &AssetServer,
    fonts: &Fonts,
    data: &ShipData,
    ship: &Ship,
    profile: &Profile,
    chapters: u32,
) {
    let current = ship.current(data);
    root.spawn((
        Node {
            position_type: PositionType::Absolute,
            left: px(10),
            bottom: px(90),
            width: px(520),
            padding: UiRect::axes(px(26), px(26)),
            flex_direction: FlexDirection::Column,
            row_gap: px(8),
            ..default()
        },
        Skin(Skins::Tile),
        GlobalZIndex(10),
    ))
    .with_children(|p| {
        p.spawn(Node {
            justify_content: JustifyContent::SpaceBetween,
            align_items: AlignItems::Center,
            ..default()
        })
        .with_children(|row| {
            row.spawn(heading(fonts, "Верфь", 25.0, BRIGHT));
            close_button(row, fonts);
        });
        p.spawn(text(
            fonts,
            format!(
                "Сейчас «{}»: отсеки до ур. {}. Отсеки и их уровни переезжают на новый корабль.",
                current.name, current.max_level
            ),
            false,
            13.0,
            MUTED,
        ));
        for def in &data.ships {
            let mine = ship.owns(&def.id);
            let here = def.id == current.id;
            let offer = ship_offer(def, ship, chapters);
            let affordable = match (&def.price, def.doubloons) {
                (Some(c), _) => {
                    profile.silver >= c.silver
                        && profile.planks >= c.planks
                        && profile.ether >= c.ether
                }
                (None, Some(d)) => profile.doubloons >= d,
                (None, None) => true,
            };
            let mut card = p.spawn((
                Node {
                    padding: UiRect::axes(px(10), px(6)),
                    column_gap: px(10),
                    align_items: AlignItems::Center,
                    border: UiRect::all(px(if here { 2.0 } else { 1.0 })),
                    border_radius: BorderRadius::all(px(10)),
                    ..default()
                },
                BackgroundColor(Color::srgba(0.16, 0.10, 0.06, 0.92)),
                BorderColor::all(if here {
                    GOLD
                } else {
                    Color::srgb(0.55, 0.42, 0.22)
                }),
            ));
            card.with_children(|c| {
                c.spawn((
                    Node {
                        width: px(92),
                        height: px(92),
                        ..default()
                    },
                    ImageNode {
                        color: if mine || offer.is_ok() {
                            Color::WHITE
                        } else {
                            Color::srgba(0.45, 0.45, 0.5, 0.9)
                        },
                        ..ImageNode::new(assets.load(def.exterior()))
                    },
                    Pickable::IGNORE,
                ));
                c.spawn((
                    Node {
                        flex_grow: 1.0,
                        flex_basis: px(0),
                        flex_direction: FlexDirection::Column,
                        row_gap: px(2),
                        ..default()
                    },
                    Pickable::IGNORE,
                ))
                .with_children(|col| {
                    col.spawn(heading(fonts, format!("«{}»", def.name), 19.0, BRIGHT));
                    col.spawn(text(fonts, def.about.clone(), false, 12.0, PARCHMENT));
                    let rooms = def
                        .rooms
                        .iter()
                        .filter(|r| data.compartment(&r.id).is_some_and(|c| c.rank <= def.rank))
                        .count();
                    col.spawn(text(
                        fonts,
                        format!("Отсеков: {rooms} · потолок ур. {}", def.max_level),
                        true,
                        12.0,
                        GOLD,
                    ));
                });
                // Справа — что можно сделать: на борту / перейти / цена / когда откроется.
                let (title, sub, ready) = if here {
                    ("На борту".to_string(), String::new(), false)
                } else if mine {
                    ("На борт".to_string(), "перейти".to_string(), true)
                } else {
                    match &offer {
                        Err(why) => ("Закрыт".to_string(), why.clone(), false),
                        Ok(()) => {
                            let sub = match (&def.price, def.doubloons) {
                                (Some(c), _) => format!(
                                    "{} серебра · {} досок{}",
                                    c.silver,
                                    c.planks,
                                    if c.ether > 0 {
                                        format!(" · {} эфира", c.ether)
                                    } else {
                                        String::new()
                                    }
                                ),
                                (None, Some(d)) => format!("{d} дублонов"),
                                (None, None) => String::new(),
                            };
                            ("Купить".to_string(), sub, affordable)
                        }
                    }
                };
                let mut button = c.spawn((
                    Node {
                        width: px(124),
                        min_height: px(56),
                        padding: UiRect::axes(px(6), px(4)),
                        flex_direction: FlexDirection::Column,
                        justify_content: JustifyContent::Center,
                        align_items: AlignItems::Center,
                        ..default()
                    },
                    Skin(if ready { Skins::Go } else { Skins::Off }),
                ));
                button.with_children(|b| {
                    b.spawn(heading(fonts, title, 17.0, Color::WHITE));
                    if !sub.is_empty() {
                        b.spawn((
                            text(fonts, sub, false, 11.0, PARCHMENT),
                            TextLayout::justify(Justify::Center),
                        ));
                    }
                });
                if !here {
                    let action = if mine {
                        Action::Board(def.id.clone())
                    } else {
                        Action::Buy(def.id.clone())
                    };
                    pressable(&mut button);
                    button.observe(move |_: On<PointerClick>, mut commands: Commands| {
                        commands.run_system_cached_with(act, action.clone());
                    });
                }
            });
        }
    });
}

/// Крестик панели: закрыть отсек или верфь.
fn close_button(row: &mut ChildSpawnerCommands, fonts: &Fonts) {
    let mut close = row.spawn((
        Node {
            width: px(46),
            height: px(40),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            ..default()
        },
        Skin(Skins::Dark),
    ));
    close.with_child(heading(fonts, "×", 22.0, PARCHMENT));
    pressable(&mut close);
    close.observe(
        |_: On<PointerClick>,
         mut open: ResMut<OpenCompartment>,
         mut rebuild: ResMut<MenuRebuild>| {
            open.0 = None;
            rebuild.0 = true;
        },
    );
}

/// Особый id в `OpenCompartment`: открыта верфь, а не отсек.
pub const SHIPYARD: &str = "shipyard";

pub fn speedup_price(data: &ShipData, left: f32) -> u32 {
    (left / 60.0).ceil().max(1.0) as u32 * data.speedup_doubloons_per_minute
}

#[derive(Clone)]
enum Action {
    Upgrade(String),
    Speedup,
    Open(Screen),
    Buy(String),
    Board(String),
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
            ..default()
        },
        Skin(if ready { Skins::Go } else { Skins::Off }),
    ));
    button.with_children(|b| {
        b.spawn(heading(fonts, title, 21.0, Color::WHITE));
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
    mut deeds: MessageWriter<Deed>,
    progress: Res<Progress>,
    screen: Res<crate::battle_screen::BattleScreen>,
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
                deeds.write(Deed("build"));
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
        Action::Board(id) => {
            ship.board(&id);
            sfx.write(Sfx("click"));
            rebuild.0 = true;
        }
        Action::Buy(id) => {
            let Some(def) = data.ship(&id) else {
                return;
            };
            let chapters = chapters_done(&screen.data.stages, progress.cleared);
            if let Err(why) = ship_offer(def, &ship, chapters) {
                toast.show(&format!("«{}»: {why}", def.name));
                return;
            }
            match (&def.price, def.doubloons) {
                (Some(c), _) => {
                    if profile.silver < c.silver
                        || profile.planks < c.planks
                        || profile.ether < c.ether
                    {
                        toast.show("Не хватает ресурсов — добудьте в боях и соберите эфир");
                        return;
                    }
                    profile.silver -= c.silver;
                    profile.planks -= c.planks;
                    profile.ether -= c.ether;
                }
                (None, Some(d)) => {
                    if profile.doubloons < d {
                        toast.show(&format!("Не хватает дублонов: нужно {d}"));
                        return;
                    }
                    profile.doubloons -= d;
                }
                (None, None) => {}
            }
            ship.buy(&id);
            toast.show(&format!("Новый корабль: «{}»! Отсеки переехали", def.name));
            sfx.write(Sfx("done"));
            rebuild.0 = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compartment_capped_by_ship() {
        let data = ShipData::bundled();
        let mut ship = Ship::new(&data);
        // На «Ласточке» потолок ур. 3: до 3 можно, дальше — нужен корабль крупнее.
        ship.levels.insert("cannon_deck".into(), 2);
        assert!(ship.next_step(&data, "cannon_deck").is_ok());
        ship.levels.insert("cannon_deck".into(), 3);
        assert!(ship.next_step(&data, "cannon_deck").is_err());
        ship.buy("leviathan");
        assert!(ship.next_step(&data, "cannon_deck").is_ok());
    }

    #[test]
    fn cramped_compartment_needs_bigger_ship() {
        let data = ShipData::bundled();
        let mut ship = Ship::new(&data);
        // Каюты — второго ранга: на «Ласточке» их нет.
        assert!(ship.next_step(&data, "cabins").is_err());
        ship.buy("petrel");
        assert!(ship.next_step(&data, "cabins").is_ok());
    }

    #[test]
    fn old_save_hull_becomes_ships() {
        let data = ShipData::bundled();
        let mut ship = Ship::new(&data);
        ship.owned.clear();
        ship.current.clear();
        ship.levels.insert(HULL.into(), 4);
        ship.levels.insert("hold".into(), 4);
        ship.migrate(&data);
        assert!(ship.owns("lastochka") && ship.owns("petrel") && !ship.owns("leviathan"));
        // Особый корабль за дублоны не выдаётся.
        assert!(!ship.owns("falcon"));
        assert_eq!(ship.current(&data).id, "petrel");
        assert_eq!(ship.level("hold"), 4);
        assert_eq!(ship.level(HULL), 0);
    }

    #[test]
    fn one_build_slot() {
        let data = ShipData::bundled();
        let mut ship = Ship::new(&data);
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
