//! Ивенты по календарю: неделя за неделей по циклу из `data/events.json`.
//!
//! Пока готова «Буря»: буревая тварь с огромным запасом здоровья, счёт — урон за бой,
//! засчитывается лучший бой недели, за пороги урона — награды. По выходным —
//! «Попутный ветер»: добыча с конвоев больше.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::Screen;
use crate::battle_screen::{CustomFight, FightKind};
use crate::menu::{GOLD, MUTED, PARCHMENT, Profile, Voyage, pressable, text};
use crate::save::now;
use crate::ship::clock;
use crate::story::VoyageRequest;
use crate::style::{Look, Skin};
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
    pub blockade: BlockadeData,
    pub tailwind: Tailwind,
    /// Тестовая клавиша E: показать другой ивент цикла.
    #[serde(skip)]
    pub forced: Option<usize>,
}

#[derive(Deserialize)]
pub struct FairData {
    pub name: String,
    pub about: String,
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
    /// Копия героя на выбор.
    pub copies: u32,
}

impl Bundle {
    fn fields(&self) -> [(u32, &'static str); 9] {
        [
            (self.silver, "серебра"),
            (self.planks, "досок"),
            (self.ether, "эфира"),
            (self.journals, "журналов"),
            (self.details, "деталей"),
            (self.doubloons, "дублонов"),
            (self.charts, "лоции"),
            (self.storm_cores, "буревых ядер"),
            (self.copies, "копия героя"),
        ]
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
    }

    fn put(&self, p: &mut Profile) {
        p.silver += self.silver;
        p.planks += self.planks;
        p.ether += self.ether;
        p.journals += self.journals;
        p.details += self.details;
        p.doubloons += self.doubloons;
        p.charts += self.charts;
        p.storm_cores += self.storm_cores;
    }
}

#[derive(Deserialize)]
pub struct BlockadeData {
    pub name: String,
    pub about: String,
    pub waves: Vec<Leg>,
}

#[derive(Deserialize)]
pub struct RegattaData {
    pub name: String,
    pub about: String,
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

    pub fn blockade_fight(&self, wave: usize, hp: Vec<(String, f32)>) -> CustomFight {
        CustomFight {
            title: format!(
                "{} · волна {} из {}",
                self.blockade.name,
                wave + 1,
                self.blockade.waves.len()
            ),
            kind: FightKind::Blockade(wave),
            enemies: self.blockade.waves[wave].enemies.clone(),
            silver: 0,
            planks: 0,
            journals: 0,
            energy_rate: 1,
            ally_hp: hp,
            charged: true,
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
    pub blockade_run: Option<BlockadeRun>,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct BlockadeRun {
    pub wave: usize,
    pub hp: Vec<(String, f32)>,
}

impl WeekEvents {
    fn roll_week(&mut self) {
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

    fn run(&self) -> Option<&BlockadeRun> {
        if self.week == week() {
            self.blockade_run.as_ref()
        } else {
            None
        }
    }

    /// Блокада: волна отбита — дальше с тем здоровьем, что осталось; награда за новую волну.
    pub fn blockade_win(
        &mut self,
        wave: usize,
        hp: Vec<(String, f32)>,
        data: &BlockadeData,
        profile: &mut Profile,
    ) -> String {
        self.roll_week();
        let mut note = format!("Волна {} отбита!", wave + 1);
        if wave + 1 > self.blockade_best {
            self.blockade_best = wave + 1;
            let reward = data.waves[wave].reward;
            reward.put(profile);
            note.push_str(&format!(" +{}", reward.line()));
        }
        if wave + 1 >= data.waves.len() {
            self.blockade_run = None;
            note.push_str("\nБлокада снята — все волны отбиты!");
        } else {
            self.blockade_run = Some(BlockadeRun { wave: wave + 1, hp });
            note.push_str(&format!(
                "\nДальше — волна {}: здоровье не восстановится",
                wave + 2
            ));
        }
        note
    }

    /// Блокада: отряд пал — оборона окончена.
    pub fn blockade_lose(&mut self, wave: usize) -> String {
        self.roll_week();
        self.blockade_run = None;
        format!(
            "Блокада прорвана на волне {} · лучший результат недели: {} волн",
            wave + 1,
            self.blockade_best
        )
    }

    /// Регата: победа на текущем этапе — дальше по трассе и награда.
    pub fn finish_leg(&mut self, leg: usize, data: &RegattaData, profile: &mut Profile) -> String {
        self.roll_week();
        if leg != self.leg {
            return "Этот этап уже пройден".into();
        }
        self.leg += 1;
        let reward = data.legs[leg].reward;
        reward.put(profile);
        if self.leg >= data.legs.len() {
            format!("Финиш регаты! +{}", reward.line())
        } else {
            format!("Этап {} пройден: +{}", leg + 1, reward.line())
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
pub fn event_card(
    col: &mut ChildSpawnerCommands,
    fonts: &Fonts,
    data: &EventData,
    storm: &StormEvent,
    week_events: &WeekEvents,
    profile: &Profile,
    owned: &[(String, String)],
) {
    match data.current() {
        "storm" => storm_card(col, fonts, data, storm),
        "fair" => fair_card(col, fonts, data, week_events, profile, owned),
        "regatta" => regatta_card(col, fonts, data, week_events),
        "blockade" => blockade_card(col, fonts, data, week_events, owned),
        _ => {}
    }
}

fn event_frame<'a>(
    col: &'a mut ChildSpawnerCommands,
    fonts: &Fonts,
    title: String,
    about: &str,
) -> EntityCommands<'a> {
    let mut card = col.spawn((
        Node {
            padding: UiRect::axes(px(30), px(28)),
            flex_direction: FlexDirection::Column,
            row_gap: px(5),
            border: UiRect::all(px(2)),
            border_radius: BorderRadius::all(px(14)),
            ..default()
        },
        Skin(Look::Wood),
    ));
    card.with_children(|c| {
        c.spawn(text(fonts, title, true, 18.0, PARCHMENT));
        c.spawn(text(fonts, about.to_string(), false, 12.0, MUTED));
    });
    card
}

fn fair_card(
    col: &mut ChildSpawnerCommands,
    fonts: &Fonts,
    data: &EventData,
    week_events: &WeekEvents,
    profile: &Profile,
    owned: &[(String, String)],
) {
    let fair = &data.fair;
    let title = format!(
        "Ивент недели: {} · осталось {}",
        fair.name,
        days_left(week_left())
    );
    event_frame(col, fonts, title, &fair.about).with_children(|card| {
        for offer in &fair.offers {
            let left = offer.limit.saturating_sub(week_events.bought(&offer.id));
            let can = left > 0 && offer.give.affordable(profile);
            card.spawn((
                Node {
                    padding: UiRect::axes(px(10), px(5)),
                    column_gap: px(8),
                    row_gap: px(4),
                    flex_wrap: FlexWrap::Wrap,
                    align_items: AlignItems::Center,
                    border_radius: BorderRadius::all(px(8)),
                    ..default()
                },
                BackgroundColor(Color::srgba(1.0, 1.0, 1.0, 0.06)),
            ))
            .with_children(|row| {
                row.spawn(Node {
                    flex_grow: 1.0,
                    flex_direction: FlexDirection::Column,
                    ..default()
                })
                .with_children(|c| {
                    c.spawn(text(fonts, offer.text.clone(), true, 13.0, PARCHMENT));
                    c.spawn(text(
                        fonts,
                        format!(
                            "{} » {} · осталось {left}",
                            offer.give.line(),
                            offer.get.line()
                        ),
                        false,
                        11.0,
                        MUTED,
                    ));
                });
                if offer.get.copies > 0 {
                    // Копия на выбор: по кнопке на каждого героя команды.
                    for (id, name) in owned {
                        trade_button(row, fonts, name, can, (offer.id.clone(), Some(id.clone())));
                    }
                } else {
                    trade_button(row, fonts, "Обменять", can, (offer.id.clone(), None));
                }
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

fn blockade_card(
    col: &mut ChildSpawnerCommands,
    fonts: &Fonts,
    data: &EventData,
    week_events: &WeekEvents,
    owned: &[(String, String)],
) {
    let b = &data.blockade;
    let title = format!(
        "Ивент недели: {} · осталось {}",
        b.name,
        days_left(week_left())
    );
    let best = if week_events.week == week() {
        week_events.blockade_best
    } else {
        0
    };
    let run = week_events.run().cloned();
    let name = |id: &str| {
        owned
            .iter()
            .find(|(i, _)| i == id)
            .map_or(id.to_string(), |(_, n)| n.clone())
    };
    event_frame(col, fonts, title, &b.about).with_children(|card| {
        card.spawn(text(
            fonts,
            format!("Лучший результат недели: {best} из {} волн", b.waves.len()),
            true,
            13.0,
            GOLD,
        ));
        match &run {
            Some(r) => {
                let alive: Vec<String> =
                    r.hp.iter()
                        .filter(|(_, f)| *f > 0.0)
                        .map(|(id, f)| format!("{} {}%", name(id), (f * 100.0).round()))
                        .collect();
                card.spawn(text(
                    fonts,
                    format!("Волна {} · в строю: {}", r.wave + 1, alive.join(", ")),
                    false,
                    12.0,
                    PARCHMENT,
                ));
                card.spawn(Node {
                    column_gap: px(8),
                    ..default()
                })
                .with_children(|row| {
                    blockade_button(
                        row,
                        fonts,
                        &format!("В бой: волна {}", r.wave + 1),
                        Some(r.clone()),
                    );
                    blockade_button(row, fonts, "Сдаться", None);
                });
            }
            None => {
                card.spawn(text(
                    fonts,
                    format!(
                        "Награда за новую волну: {}",
                        b.waves.get(best).map_or("—".into(), |w| w.reward.line())
                    ),
                    false,
                    12.0,
                    PARCHMENT,
                ));
                card.spawn(Node::default()).with_children(|row| {
                    blockade_button(
                        row,
                        fonts,
                        "Начать оборону",
                        Some(BlockadeRun {
                            wave: 0,
                            hp: Vec::new(),
                        }),
                    );
                });
            }
        }
    });
}

/// Кнопка блокады: `Some` — в бой (пустое здоровье — новый заход всем отрядом), `None` — сдаться.
fn blockade_button(
    row: &mut ChildSpawnerCommands,
    fonts: &Fonts,
    label: &str,
    run: Option<BlockadeRun>,
) {
    let fight = run.is_some();
    let mut go = row.spawn((
        Node {
            flex_grow: 1.0,
            height: px(46),
            padding: UiRect::horizontal(px(12)),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            border: UiRect::all(px(2)),
            border_radius: BorderRadius::all(px(12)),
            ..default()
        },
        Skin(if fight { Look::Go } else { Look::Off }),
    ));
    go.with_child(text(fonts, label, true, 16.0, Color::WHITE));
    pressable(&mut go);
    go.observe(
        move |_: On<PointerClick>,
              data: Res<EventData>,
              roster: Res<crate::crew::Roster>,
              mut week_events: ResMut<WeekEvents>,
              mut voyage: ResMut<Voyage>,
              mut requests: MessageWriter<VoyageRequest>,
              mut dirty: ResMut<crate::abyss::ActivitiesDirty>| {
            week_events.roll_week();
            match &run {
                None => {
                    week_events.blockade_run = None;
                    dirty.0 = true;
                }
                Some(r) => {
                    // Новый заход: весь отряд с полным здоровьем.
                    let hp = if r.hp.is_empty() {
                        roster.team.iter().map(|id| (id.clone(), 1.0)).collect()
                    } else {
                        r.hp.clone()
                    };
                    week_events.blockade_run = Some(BlockadeRun {
                        wave: r.wave,
                        hp: hp.clone(),
                    });
                    *voyage = Voyage {
                        stage: None,
                        custom: Some(data.blockade_fight(r.wave, hp)),
                        from: Screen::Activities,
                    };
                    requests.write(VoyageRequest);
                }
            }
        },
    );
}

fn regatta_card(
    col: &mut ChildSpawnerCommands,
    fonts: &Fonts,
    data: &EventData,
    week_events: &WeekEvents,
) {
    let regatta = &data.regatta;
    let leg = week_events.next_leg();
    let title = format!(
        "Ивент недели: {} · осталось {}",
        regatta.name,
        days_left(week_left())
    );
    event_frame(col, fonts, title, &regatta.about).with_children(|card| {
        match regatta.legs.get(leg) {
            None => {
                card.spawn(text(
                    fonts,
                    "Трасса пройдена! Ждите следующую регату",
                    true,
                    14.0,
                    GOLD,
                ));
            }
            Some(l) => {
                card.spawn(text(
                    fonts,
                    format!(
                        "Этап {} из {} · награда: {}",
                        leg + 1,
                        regatta.legs.len(),
                        l.reward.line()
                    ),
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
                    Skin(Look::Go),
                ));
                go.with_child(text(fonts, "На старт", true, 18.0, Color::WHITE));
                pressable(&mut go);
                go.observe(
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
            padding: UiRect::axes(px(30), px(28)),
            flex_direction: FlexDirection::Column,
            row_gap: px(5),
            border: UiRect::all(px(2)),
            border_radius: BorderRadius::all(px(14)),
            ..default()
        },
        Skin(Look::Wood),
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
            Skin(Look::Go),
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
    fn blockade_carries_hp_and_rewards_new_waves_once() {
        let data = EventData::bundled();
        let mut w = WeekEvents::default();
        let mut profile = Profile::new(&MenuData::bundled().start);
        let hp = vec![("ren".to_string(), 0.5)];
        w.blockade_win(0, hp.clone(), &data.blockade, &mut profile);
        let run = w.blockade_run.clone().unwrap();
        assert_eq!(run.wave, 1);
        assert_eq!(run.hp, hp);
        let details = profile.details;
        // Повтор той же волны награды не даёт.
        w.blockade_win(0, hp, &data.blockade, &mut profile);
        assert_eq!(profile.details, details);
        w.blockade_lose(1);
        assert!(w.blockade_run.is_none());
        assert_eq!(w.blockade_best, 1);
    }

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
