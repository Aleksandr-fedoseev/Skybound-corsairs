//! Ивенты по календарю: неделя за неделей по циклу из `data/events.json`.
//!
//! Пока готова «Буря»: буревая тварь с огромным запасом здоровья, счёт — урон за бой,
//! засчитывается лучший бой недели, за пороги урона — награды. По выходным —
//! «Попутный ветер»: добыча с конвоев больше.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::Screen;
use crate::battle_screen::{CustomFight, FightKind};
use crate::menu::{GOLD, PARCHMENT, Profile, Voyage, pressable, text};
use crate::save::now;
use crate::ship::clock;
use crate::story::VoyageRequest;
use crate::style::{self, Look, Skin};
use crate::summary::{Summary, Tone};
use crate::view::Fonts;

const SECONDS_PER_DAY: u64 = 86_400;
/// 1 января 1970 года — четверг; сдвиг, чтобы неделя начиналась в понедельник.
const THURSDAY_SHIFT: u64 = 3;

#[derive(Resource, Deserialize)]
pub struct EventData {
    pub cycle: Vec<String>,
    pub storm: StormData,
    pub fair: FairData,
    pub regatta: RegattaData,
    pub blockade: crate::blockade::BlockadeData,
    pub tailwind: Tailwind,
    /// Тестовая клавиша E: показать другой ивент цикла.
    #[serde(skip)]
    pub forced: Option<usize>,
}

#[derive(Deserialize)]
pub struct FairData {
    pub name: String,
    pub offers: Vec<Offer>,
}

#[derive(Deserialize)]
pub struct Offer {
    pub id: String,
    pub text: String,
    pub give: Bundle,
    pub get: Bundle,
    /// Сколько раз за неделю.
    pub limit: u32,
}

/// Набор ресурсов для обмена и наград.
#[derive(Deserialize, Default, Clone, Copy)]
#[serde(default)]
pub struct Bundle {
    pub silver: u32,
    pub planks: u32,
    pub ether: u32,
    pub journals: u32,
    pub details: u32,
    pub doubloons: u32,
    pub charts: u32,
    pub storm_cores: u32,
    pub supplies: u32,
    pub figureheads: u32,
    /// Копия героя на выбор.
    pub copies: u32,
}

impl Bundle {
    fn fields(&self) -> [(u32, &'static str); 11] {
        [
            (self.silver, "серебра"),
            (self.planks, "досок"),
            (self.ether, "эфира"),
            (self.journals, "журналов"),
            (self.details, "деталей"),
            (self.doubloons, "дублонов"),
            (self.charts, "лоции"),
            (self.storm_cores, "буревых ядер"),
            (self.supplies, "припасов"),
            (self.figureheads, "носовые фигуры"),
            (self.copies, "копия героя"),
        ]
    }

    /// Первый ненулевой ресурс: имя иконки (`ui/icon_<имя>.png`) и сколько.
    pub fn main(&self) -> Option<(&'static str, u32)> {
        self.items().into_iter().next()
    }

    /// Все ненулевые ресурсы иконками: для строки наград.
    pub fn items(&self) -> Vec<(&'static str, u32)> {
        [
            (self.silver, "silver"),
            (self.planks, "planks"),
            (self.ether, "ether"),
            (self.journals, "journals"),
            (self.details, "details"),
            (self.doubloons, "doubloons"),
            (self.charts, "charts"),
            (self.storm_cores, "storm_cores"),
            (self.supplies, "supplies"),
            (self.figureheads, "figureheads"),
        ]
        .into_iter()
        .filter(|(n, _)| *n > 0)
        .map(|(n, kind)| (kind, n))
        .collect()
    }

    /// Награда, умноженная на дробное `k` (с округлением): «×1.5 до конца обороны».
    pub fn scaled(&self, k: f32) -> Self {
        let m = |n: u32| (n as f32 * k).round() as u32;
        Self {
            silver: m(self.silver),
            planks: m(self.planks),
            ether: m(self.ether),
            journals: m(self.journals),
            details: m(self.details),
            doubloons: m(self.doubloons),
            charts: m(self.charts),
            storm_cores: m(self.storm_cores),
            supplies: m(self.supplies),
            figureheads: m(self.figureheads),
            copies: self.copies,
        }
    }

    /// Та же награда, умноженная на `k` (двойная добыча).
    pub fn times(&self, k: u32) -> Self {
        Self {
            silver: self.silver * k,
            planks: self.planks * k,
            ether: self.ether * k,
            journals: self.journals * k,
            details: self.details * k,
            doubloons: self.doubloons * k,
            charts: self.charts * k,
            storm_cores: self.storm_cores * k,
            supplies: self.supplies * k,
            figureheads: self.figureheads * k,
            copies: self.copies * k,
        }
    }

    pub fn line(&self) -> String {
        self.fields()
            .iter()
            .filter(|(n, _)| *n > 0)
            .map(|(n, name)| format!("{n} {name}"))
            .collect::<Vec<_>>()
            .join(" · ")
    }

    fn affordable(&self, p: &Profile) -> bool {
        p.silver >= self.silver
            && p.planks >= self.planks
            && p.ether >= self.ether
            && p.journals >= self.journals
            && p.details >= self.details
            && p.doubloons >= self.doubloons
            && p.charts >= self.charts
            && p.storm_cores >= self.storm_cores
            && p.supplies >= self.supplies
            && p.figureheads >= self.figureheads
    }

    fn take(&self, p: &mut Profile) {
        p.silver -= self.silver;
        p.planks -= self.planks;
        p.ether -= self.ether;
        p.journals -= self.journals;
        p.details -= self.details;
        p.doubloons -= self.doubloons;
        p.charts -= self.charts;
        p.storm_cores -= self.storm_cores;
        p.supplies -= self.supplies;
        p.figureheads -= self.figureheads;
    }

    pub(crate) fn put(&self, p: &mut Profile) {
        p.silver += self.silver;
        p.planks += self.planks;
        p.ether += self.ether;
        p.journals += self.journals;
        p.details += self.details;
        p.doubloons += self.doubloons;
        p.charts += self.charts;
        p.storm_cores += self.storm_cores;
        p.supplies += self.supplies;
        p.figureheads += self.figureheads;
    }
}

#[derive(Deserialize)]
pub struct RegattaData {
    pub name: String,
    pub energy_rate: u32,
    pub legs: Vec<Leg>,
}

#[derive(Deserialize)]
pub struct Leg {
    pub enemies: Vec<(String, u32)>,
    pub reward: Bundle,
}

#[derive(Deserialize)]
pub struct StormData {
    pub name: String,
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
    pub supplies: u32,
}

impl Prize {
    pub fn items(&self) -> Vec<(&'static str, u32)> {
        [
            ("charts", self.charts),
            ("doubloons", self.doubloons),
            ("storm_cores", self.storm_cores),
            ("supplies", self.supplies),
        ]
        .into_iter()
        .filter(|(_, n)| *n > 0)
        .collect()
    }
}

#[derive(Deserialize)]
pub struct Tailwind {
    /// Дни недели (0 — понедельник), когда дует попутный ветер.
    pub days: Vec<u64>,
    pub convoy_multiplier: u32,
}

impl EventData {
    pub(crate) fn bundled() -> Self {
        serde_json::from_str(include_str!("../../../data/events.json"))
            .expect("data/events.json корректен")
    }

    /// Ивент текущей недели.
    pub fn current(&self) -> &str {
        let i = self.forced.unwrap_or(week() as usize);
        &self.cycle[i % self.cycle.len()]
    }

    /// Название ивента этой недели.
    pub fn current_name(&self) -> &str {
        match self.current() {
            "storm" => &self.storm.name,
            "fair" => &self.fair.name,
            "regatta" => &self.regatta.name,
            "blockade" => &self.blockade.name,
            _ => "",
        }
    }

    pub fn regatta_fight(&self, leg: usize) -> CustomFight {
        CustomFight {
            title: format!(
                "{} · этап {} из {}",
                self.regatta.name,
                leg + 1,
                self.regatta.legs.len()
            ),
            kind: FightKind::Regatta(leg),
            enemies: self.regatta.legs[leg].enemies.clone(),
            silver: 0,
            planks: 0,
            journals: 0,
            energy_rate: self.regatta.energy_rate,
            ally_hp: Vec::new(),
            charged: false,
            boost: Default::default(),
            wind: None,
        }
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
            energy_rate: 1,
            ally_hp: Vec::new(),
            charged: false,
            boost: Default::default(),
            wind: None,
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
pub fn days_left(seconds: u64) -> String {
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
    pub fn record(&mut self, damage: u32, data: &StormData, profile: &mut Profile) -> Summary {
        self.roll_week();
        self.best = self.best.max(damage);
        let mut sum = Summary::new(format!("Урон по буре: {damage}"))
            .chip(format!("лучший за неделю: {}", self.best), Tone::Info);
        while let Some(m) = data.milestones.get(self.reached) {
            if self.best < m.damage {
                break;
            }
            profile.charts += m.reward.charts;
            profile.doubloons += m.reward.doubloons;
            profile.storm_cores += m.reward.storm_cores;
            profile.supplies += m.reward.supplies;
            sum = sum
                .gain("charts", m.reward.charts as i64)
                .gain("doubloons", m.reward.doubloons as i64)
                .gain("storm_cores", m.reward.storm_cores as i64)
                .gain("supplies", m.reward.supplies as i64);
            self.reached += 1;
        }
        sum
    }
}

/// Ярмарка и регата за неделю (сохраняются).
#[derive(Resource, Clone, Default, Serialize, Deserialize)]
pub struct WeekEvents {
    pub week: u64,
    /// Ярмарка: сколько раз куплено каждое предложение.
    pub bought: std::collections::HashMap<String, u32>,
    /// Регата: следующий этап трассы.
    pub leg: usize,
    /// Блокада: сколько волн за неделю уже приносили награду.
    #[serde(default)]
    pub blockade_best: usize,
    /// Блокада: идущая оборона — волна и здоровье героев.
    #[serde(default)]
    pub blockade_run: Option<crate::blockade::BlockadeRun>,
}

impl WeekEvents {
    pub(crate) fn roll_week(&mut self) {
        let w = week();
        if self.week != w {
            *self = Self {
                week: w,
                ..default()
            };
        }
    }

    fn bought(&self, id: &str) -> u32 {
        if self.week == week() {
            self.bought.get(id).copied().unwrap_or(0)
        } else {
            0
        }
    }

    fn next_leg(&self) -> usize {
        if self.week == week() { self.leg } else { 0 }
    }

    /// Регата: победа на текущем этапе — дальше по трассе и награда.
    pub fn finish_leg(&mut self, leg: usize, data: &RegattaData, profile: &mut Profile) -> Summary {
        self.roll_week();
        if leg != self.leg {
            return Summary::new("Этот этап уже пройден");
        }
        self.leg += 1;
        let reward = data.legs[leg].reward;
        reward.put(profile);
        if self.leg >= data.legs.len() {
            Summary::new("Финиш регаты!").bundle(&reward)
        } else {
            Summary::new(format!("Этап {} пройден", leg + 1)).bundle(&reward)
        }
    }
}

pub fn plugin(app: &mut App) {
    app.insert_resource(EventData::bundled())
        .init_resource::<StormEvent>()
        .init_resource::<WeekEvents>();
    #[cfg(feature = "dev")]
    app.add_systems(Update, dev_event_key);
}

/// E — следующий ивент цикла (для проверки всех ивентов в любую неделю).
#[cfg(feature = "dev")]
fn dev_event_key(
    keys: Res<ButtonInput<KeyCode>>,
    mut data: ResMut<EventData>,
    mut dirty: ResMut<crate::abyss::ActivitiesDirty>,
) {
    if keys.just_pressed(KeyCode::KeyE) {
        let next = data.forced.unwrap_or(week() as usize) + 1;
        data.forced = Some(next);
        dirty.0 = true;
    }
}

/// Карточка ивента недели: «Буря», «Ярмарка» или «Регата».
#[allow(clippy::too_many_arguments)]
pub fn event_card(
    col: &mut ChildSpawnerCommands,
    assets: &AssetServer,
    fonts: &Fonts,
    data: &EventData,
    storm: &StormEvent,
    week_events: &WeekEvents,
    profile: &Profile,
    owned: &[(String, String)],
    progress: (usize, &[String]),
) {
    match data.current() {
        "storm" => storm_card(col, assets, fonts, data, storm),
        "fair" => fair_card(col, assets, fonts, data, week_events, profile, owned),
        "regatta" => regatta_card(col, assets, fonts, data, week_events),
        "blockade" => crate::blockade::card(
            col,
            assets,
            fonts,
            &data.blockade,
            week_events,
            progress.0,
            progress.1,
        ),
        _ => {}
    }
}
/// Ивент: баннер с главной цифрой и «осталось …», под ним — карточка с содержимым.
fn event_frame<'a>(
    col: &'a mut ChildSpawnerCommands,
    assets: &AssetServer,
    fonts: &Fonts,
    big: String,
) -> EntityCommands<'a> {
    style::banner(
        col,
        assets,
        fonts,
        "event",
        big,
        &[(
            format!("осталось {}", days_left(week_left())),
            Color::srgb(0.45, 0.18, 0.14),
        )],
    );
    col.spawn((
        Node {
            padding: UiRect::axes(px(16), px(14)),
            flex_direction: FlexDirection::Column,
            row_gap: px(9),
            border: UiRect::all(px(2)),
            border_radius: BorderRadius::all(px(14)),
            ..default()
        },
        Skin(Look::Tile),
    ))
}

/// Широкая кнопка ивента.
fn event_go<'a>(
    card: &'a mut ChildSpawnerCommands,
    fonts: &Fonts,
    label: &str,
) -> EntityCommands<'a> {
    let mut go = card.spawn((
        Node {
            margin: UiRect::top(px(2)),
            height: px(50),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            border: UiRect::all(px(2)),
            border_radius: BorderRadius::all(px(12)),
            ..default()
        },
        Skin(Look::Go),
    ));
    go.with_child(text(fonts, label, true, 18.0, Color::WHITE));
    pressable(&mut go);
    go
}

fn ids(enemies: &[(String, u32)]) -> Vec<String> {
    enemies.iter().map(|(id, _)| id.clone()).collect()
}

fn fair_card(
    col: &mut ChildSpawnerCommands,
    assets: &AssetServer,
    fonts: &Fonts,
    data: &EventData,
    week_events: &WeekEvents,
    profile: &Profile,
    owned: &[(String, String)],
) {
    let fair = &data.fair;
    event_frame(col, assets, fonts, "Обмены недели".to_string()).with_children(|card| {
        for offer in &fair.offers {
            let left = offer.limit.saturating_sub(week_events.bought(&offer.id));
            let can = left > 0 && offer.give.affordable(profile);
            card.spawn((
                Node {
                    padding: UiRect::axes(px(10), px(8)),
                    flex_direction: FlexDirection::Column,
                    row_gap: px(6),
                    border_radius: BorderRadius::all(px(10)),
                    ..default()
                },
                BackgroundColor(Color::srgba(1.0, 1.0, 1.0, 0.06)),
            ))
            .with_children(|c| {
                c.spawn(Node {
                    justify_content: JustifyContent::SpaceBetween,
                    align_items: AlignItems::Center,
                    ..default()
                })
                .with_children(|row| {
                    row.spawn(text(fonts, offer.text.clone(), true, 14.0, PARCHMENT));
                    style::pill(
                        row,
                        fonts,
                        format!("осталось {left}"),
                        if left > 0 {
                            Color::srgb(0.24, 0.2, 0.3)
                        } else {
                            Color::srgb(0.3, 0.28, 0.27)
                        },
                    );
                });
                c.spawn(Node {
                    column_gap: px(8),
                    row_gap: px(6),
                    flex_wrap: FlexWrap::Wrap,
                    align_items: AlignItems::Center,
                    ..default()
                })
                .with_children(|row| {
                    style::costs(row, assets, fonts, &offer.give.items(), 15.0, PARCHMENT);
                    row.spawn(style::heading(fonts, "»", 20.0, GOLD));
                    if offer.get.copies > 0 {
                        row.spawn(style::heading(fonts, "копия героя", 15.0, GOLD));
                    } else {
                        style::costs(row, assets, fonts, &offer.get.items(), 15.0, GOLD);
                    }
                    row.spawn(Node {
                        flex_grow: 1.0,
                        ..default()
                    });
                    if offer.get.copies > 0 {
                        // Копия на выбор: по кнопке на каждого героя команды.
                        for (id, name) in owned {
                            trade_button(
                                row,
                                fonts,
                                name,
                                can,
                                (offer.id.clone(), Some(id.clone())),
                            );
                        }
                    } else {
                        trade_button(row, fonts, "Обменять", can, (offer.id.clone(), None));
                    }
                });
            });
        }
    });
}

fn trade_button(
    row: &mut ChildSpawnerCommands,
    fonts: &Fonts,
    label: &str,
    active: bool,
    trade: (String, Option<String>),
) {
    let mut button = row.spawn((
        Node {
            padding: UiRect::axes(px(10), px(6)),
            border: UiRect::all(px(1)),
            border_radius: BorderRadius::all(px(8)),
            ..default()
        },
        Skin(if active { Look::Go } else { Look::Off }),
    ));
    button.with_child(text(fonts, label, true, 12.0, Color::WHITE));
    if active {
        pressable(&mut button);
        button.observe(move |_: On<PointerClick>, mut commands: Commands| {
            commands.run_system_cached_with(trade_act, trade.clone());
        });
    }
}

#[allow(clippy::too_many_arguments)]
fn trade_act(
    In((offer_id, hero)): In<(String, Option<String>)>,
    data: Res<EventData>,
    mut week_events: ResMut<WeekEvents>,
    mut profile: ResMut<Profile>,
    mut roster: ResMut<crate::crew::Roster>,
    mut toast: ResMut<crate::menu::Toast>,
    mut dirty: ResMut<crate::abyss::ActivitiesDirty>,
    mut sfx: MessageWriter<crate::audio::Sfx>,
) {
    week_events.roll_week();
    let Some(offer) = data.fair.offers.iter().find(|o| o.id == offer_id) else {
        return;
    };
    if week_events.bought(&offer.id) >= offer.limit || !offer.give.affordable(&profile) {
        return;
    }
    offer.give.take(&mut profile);
    offer.get.put(&mut profile);
    if let Some(hero) = &hero {
        roster.recruit(hero);
    }
    *week_events.bought.entry(offer.id.clone()).or_insert(0) += 1;
    toast.show(&format!("Сделка: +{}", offer.get.line()));
    sfx.write(crate::audio::Sfx("coin"));
    dirty.0 = true;
}

fn regatta_card(
    col: &mut ChildSpawnerCommands,
    assets: &AssetServer,
    fonts: &Fonts,
    data: &EventData,
    week_events: &WeekEvents,
) {
    let regatta = &data.regatta;
    let leg = week_events.next_leg();
    event_frame(
        col,
        assets,
        fonts,
        if leg < regatta.legs.len() {
            format!("Этап {} из {}", leg + 1, regatta.legs.len())
        } else {
            "Финиш!".to_string()
        },
    )
    .with_children(|card| {
        style::pips(card, fonts, regatta.legs.len(), leg, Some(leg));
        match regatta.legs.get(leg) {
            None => {
                style::pill(
                    card,
                    fonts,
                    "Трасса пройдена! Ждите следующую регату",
                    Color::srgb(0.7, 0.5, 0.12),
                );
            }
            Some(l) => {
                card.spawn(Node {
                    column_gap: px(10),
                    align_items: AlignItems::Center,
                    ..default()
                })
                .with_children(|row| {
                    row.spawn(style::heading(
                        fonts,
                        format!("Этап {}", leg + 1),
                        16.0,
                        GOLD,
                    ));
                    crate::map::enemies_row(row, assets, fonts, &ids(&l.enemies), None, PARCHMENT);
                });
                style::reward_row(card, assets, fonts, "за этап", &l.reward.items());
                event_go(card, fonts, "На старт").observe(
                    move |_: On<PointerClick>,
                          data: Res<EventData>,
                          mut voyage: ResMut<Voyage>,
                          mut requests: MessageWriter<VoyageRequest>| {
                        *voyage = Voyage {
                            stage: None,
                            custom: Some(data.regatta_fight(leg)),
                            from: Screen::Activities,
                        };
                        requests.write(VoyageRequest);
                    },
                );
            }
        }
    });
}

/// Карточка «Бури»: тварь, полоса лучшего урона до следующего порога, награда за порог.
pub fn storm_card(
    col: &mut ChildSpawnerCommands,
    assets: &AssetServer,
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
    let st = &data.storm;
    event_frame(col, assets, fonts, format!("Урон {best}")).with_children(|card| {
        crate::map::enemies_row(card, assets, fonts, &ids(&st.enemies), None, PARCHMENT);
        style::pips(card, fonts, st.milestones.len(), reached, Some(reached));
        match st.milestones.get(reached) {
            Some(m) => {
                style::meter(
                    card,
                    fonts,
                    best as f32 / m.damage.max(1) as f32,
                    format!("лучший урон {best} / {}", m.damage),
                    Color::srgb(0.45, 0.3, 0.75),
                );
                style::reward_row(card, assets, fonts, "за порог", &m.reward.items());
            }
            None => {
                style::pill(
                    card,
                    fonts,
                    format!("лучший урон {best} · все награды недели получены"),
                    Color::srgb(0.7, 0.5, 0.12),
                );
            }
        }
        event_go(card, fonts, "Атаковать бурю").observe(
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
