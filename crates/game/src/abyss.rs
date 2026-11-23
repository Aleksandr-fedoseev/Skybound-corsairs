//! Бездна облаков и экран «Активности».
//!
//! Бездна — бесконечная башня вниз: каждый этаж сложнее, каждый 10-й — босс. Награда за
//! этаж выдаётся один раз, при первом прохождении. Этажи собираются из правил
//! `data/abyss.json`, а не перечисляются вручную.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::Screen;
use crate::battle_screen::{BattleScreen, CustomFight, FightKind};
use crate::contracts::{BoardOpen, ContractData, Contracts};
use crate::expeditions::{ExpeditionData, Expeditions};
use crate::menu::{
    GOLD, MUTED, PARCHMENT, Voyage, bottom_nav, later, pressable, spawn_toast, text,
};
use crate::save::now;
use crate::ship::{Ship, ShipData};
use crate::story::VoyageRequest;
use crate::style::{self, INK, INK_SOFT, Look, Skin};
use crate::view::Fonts;

const BRIGHT: Color = Color::srgb(1.0, 0.93, 0.78);

/// Раздел Активностей, открытый из карточки; `None` — список карточек.
#[derive(Resource, Default, Clone, Copy, PartialEq, Eq)]
pub struct ActivitiesPage(pub Option<Page>);

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Page {
    Event,
    Abyss,
    Expeditions,
}

#[derive(Resource, Deserialize)]
pub struct AbyssData {
    pub enemy_pool: Vec<String>,
    pub boss: String,
    pub boss_every: u32,
    pub base_level: u32,
    pub level_per_floor: f32,
    pub base_count: u32,
    /// Каждые столько этажей — на одного врага больше (до `max_count`).
    pub more_every: u32,
    pub max_count: u32,
    pub silver: u32,
    pub silver_per_floor: u32,
    pub journals: u32,
    pub journals_per_floor: u32,
    /// Каждый такой этаж даёт лоции.
    pub charts_every: u32,
    pub charts: u32,
    pub boss_doubloons: u32,
    pub boss_figureheads: u32,
    pub details_per_floor: u32,
}

/// Что даёт этаж при первом прохождении.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct FloorReward {
    pub silver: u32,
    pub journals: u32,
    pub charts: u32,
    pub doubloons: u32,
    pub figureheads: u32,
    pub details: u32,
}

impl AbyssData {
    pub fn bundled() -> Self {
        serde_json::from_str(include_str!("../../../data/abyss.json"))
            .expect("data/abyss.json корректен")
    }

    pub fn is_boss(&self, floor: u32) -> bool {
        floor.is_multiple_of(self.boss_every)
    }

    /// Враги этажа: уровень растёт с глубиной, на каждом 10-м — босс.
    pub fn enemies(&self, floor: u32) -> Vec<(String, u32)> {
        let level = self.base_level + ((floor - 1) as f32 * self.level_per_floor) as u32;
        let count = (self.base_count + (floor - 1) / self.more_every).min(self.max_count);
        let pool = &self.enemy_pool;
        let mut out: Vec<(String, u32)> = (0..count)
            .map(|i| (pool[((floor + i) as usize) % pool.len()].clone(), level))
            .collect();
        if self.is_boss(floor) {
            // Босс в центре, по бокам — его свита.
            out.truncate(2);
            out.insert(1, (self.boss.clone(), level.saturating_sub(3).max(1)));
        }
        out
    }

    pub fn reward(&self, floor: u32) -> FloorReward {
        let boss = self.is_boss(floor);
        FloorReward {
            silver: self.silver + self.silver_per_floor * floor,
            journals: self.journals + self.journals_per_floor * floor,
            charts: if floor.is_multiple_of(self.charts_every) {
                self.charts
            } else {
                0
            },
            doubloons: if boss { self.boss_doubloons } else { 0 },
            figureheads: if boss { self.boss_figureheads } else { 0 },
            details: self.details_per_floor,
        }
    }

    pub fn fight(&self, floor: u32) -> CustomFight {
        let r = self.reward(floor);
        CustomFight {
            title: if self.is_boss(floor) {
                format!("Бездна · этаж {floor} · босс")
            } else {
                format!("Бездна · этаж {floor}")
            },
            kind: FightKind::Abyss(floor),
            enemies: self.enemies(floor),
            silver: r.silver,
            planks: 0,
            journals: r.journals,
            energy_rate: 1,
            ally_hp: Vec::new(),
            charged: false,
        }
    }
}

impl FloorReward {
    pub fn line(&self) -> String {
        let mut parts = vec![
            format!("+{} серебра", self.silver),
            format!("+{} журналов", self.journals),
        ];
        if self.details > 0 {
            parts.push(format!("+{} деталей", self.details));
        }
        if self.charts > 0 {
            parts.push(format!("+{} лоции", self.charts));
        }
        if self.doubloons > 0 {
            parts.push(format!("+{} дублонов", self.doubloons));
        }
        if self.figureheads > 0 {
            parts.push(format!("+{} носовая фигура", self.figureheads));
        }
        parts.join(" · ")
    }
}

/// Глубина, до которой игрок дошёл (сохраняется).
#[derive(Resource, Clone, Default, Serialize, Deserialize)]
pub struct Abyss {
    pub best: u32,
}

/// Экран «Активности» надо собрать заново (экспедиция ушла, вернулась, тикнул таймер).
#[derive(Resource, Default)]
pub struct ActivitiesDirty(pub bool);

#[derive(Component)]
struct ActivitiesRoot;

#[derive(Component)]
struct ActivitiesScroll;

/// Где прокручен список — чтобы пересборка экрана раз в секунду не сбрасывала его.
#[derive(Resource, Default)]
struct ScrollY(f32);

pub fn plugin(app: &mut App) {
    app.insert_resource(AbyssData::bundled())
        .init_resource::<Abyss>()
        .init_resource::<ActivitiesDirty>()
        .init_resource::<ScrollY>()
        .init_resource::<ActivitiesPage>()
        .add_systems(OnEnter(Screen::Activities), spawn_activities)
        .add_systems(
            Update,
            (
                wheel_scroll,
                remember_scroll,
                tick_timers,
                rebuild_activities,
            )
                .chain()
                .run_if(in_state(Screen::Activities)),
        );
}

/// Всё, из чего собирается экран.
#[derive(bevy::ecs::system::SystemParam)]
struct View<'w> {
    assets: Res<'w, AssetServer>,
    fonts: Res<'w, Fonts>,
    data: Res<'w, AbyssData>,
    abyss: Res<'w, Abyss>,
    screen: Res<'w, BattleScreen>,
    exp_data: Res<'w, ExpeditionData>,
    exps: Res<'w, Expeditions>,
    ship: Res<'w, Ship>,
    ship_data: Res<'w, ShipData>,
    event_data: Res<'w, crate::events::EventData>,
    storm: Res<'w, crate::events::StormEvent>,
    scroll: Res<'w, ScrollY>,
    week_events: Res<'w, crate::events::WeekEvents>,
    profile: Res<'w, crate::menu::Profile>,
    roster: Res<'w, crate::crew::Roster>,
    page: Res<'w, ActivitiesPage>,
    contract_data: Res<'w, ContractData>,
    contracts: Res<'w, Contracts>,
}

fn spawn_activities(mut commands: Commands, view: View) {
    build(&mut commands, &view);
}

fn wheel_scroll(
    mut wheel: MessageReader<bevy::input::mouse::MouseWheel>,
    mut lists: Query<&mut ScrollPosition, With<ActivitiesScroll>>,
) {
    for ev in wheel.read() {
        let step = match ev.unit {
            bevy::input::mouse::MouseScrollUnit::Line => ev.y * 40.0,
            bevy::input::mouse::MouseScrollUnit::Pixel => ev.y,
        };
        for mut pos in &mut lists {
            pos.0.y = (pos.0.y - step).max(0.0);
        }
    }
}

fn remember_scroll(lists: Query<&ScrollPosition, With<ActivitiesScroll>>, mut y: ResMut<ScrollY>) {
    for pos in &lists {
        y.0 = pos.0.y;
    }
}

/// Пока экспедиции в пути, раз в секунду обновляем время до возвращения.
fn tick_timers(
    time: Res<Time>,
    exps: Res<Expeditions>,
    mut acc: Local<f32>,
    mut dirty: ResMut<ActivitiesDirty>,
) {
    *acc += time.delta_secs();
    if *acc >= 1.0 {
        *acc = 0.0;
        if !exps.running.is_empty() {
            dirty.0 = true;
        }
    }
}

fn rebuild_activities(
    mut commands: Commands,
    mut dirty: ResMut<ActivitiesDirty>,
    roots: Query<Entity, With<ActivitiesRoot>>,
    view: View,
) {
    if !dirty.0 {
        return;
    }
    dirty.0 = false;
    for root in &roots {
        commands.entity(root).despawn();
    }
    build(&mut commands, &view);
}

fn build(commands: &mut Commands, view: &View) {
    let (assets, fonts, data, abyss, screen) = (
        &*view.assets,
        &*view.fonts,
        &*view.data,
        &*view.abyss,
        &*view.screen,
    );
    let floor = abyss.best + 1;
    let boats = crate::expeditions::boats(&view.exp_data, &view.ship, &view.ship_data);
    commands
        .spawn((
            ActivitiesRoot,
            DespawnOnExit(Screen::Activities),
            Node {
                width: percent(100),
                height: percent(100),
                ..default()
            },
        ))
        .with_children(|root| {
            style::top_bar(
                root,
                assets,
                fonts,
                &[
                    ("doubloons", view.profile.doubloons),
                    ("silver", view.profile.silver),
                    ("charts", view.profile.charts),
                ],
            );
            let title = match view.page.0 {
                None => "Активности".to_string(),
                Some(Page::Event) => view.event_data.current_name().to_string(),
                Some(Page::Abyss) => "Бездна облаков".to_string(),
                Some(Page::Expeditions) => "Экспедиции".to_string(),
            };
            style::title(root, fonts, &title, 56.0);
            match view.page.0 {
                None => hub(root, view, floor, boats),
                Some(page) => {
                    back_button(root, fonts);
                    section_page(root, view, page, floor, boats);
                }
            }
            bottom_nav(root, assets, fonts, Screen::Activities);
            spawn_toast(root, fonts);
        });
    let _ = (data, screen);
}

/// Карточки-сводки: картинка, название, состояние и «В путь».
fn hub(root: &mut ChildSpawnerCommands, view: &View, floor: u32, boats: u32) {
    let (assets, fonts) = (&*view.assets, &*view.fonts);
    let now = now();
    let back = view
        .exps
        .running
        .iter()
        .filter(|r| now >= r.ends_at)
        .count();
    let cards = [
        (
            "event",
            format!("Ивент недели: {}", view.event_data.current_name()),
            "Особые правила и награды — раз в неделю".to_string(),
            format!(
                "осталось {}",
                crate::events::days_left(crate::events::week_left())
            ),
            Go::Page(Page::Event),
        ),
        (
            "abyss",
            format!("Бездна облаков · этаж {floor}"),
            "Каждый этаж сложнее, каждый 10-й — босс".to_string(),
            format!("лучший этаж: {}", view.abyss.best),
            Go::Page(Page::Abyss),
        ),
        (
            "expeditions",
            format!("Экспедиции · {} из {boats}", view.exps.running.len()),
            "Свободные герои уходят на шлюпках за добычей".to_string(),
            if back > 0 {
                format!("вернулись: {back} — забрать добычу!")
            } else if view.exps.running.is_empty() {
                "шлюпки свободны".to_string()
            } else {
                "шлюпки в пути".to_string()
            },
            Go::Page(Page::Expeditions),
        ),
        (
            "contracts",
            format!(
                "Контракты дня · {} из {}",
                view.contracts.done_today(),
                view.contract_data.daily.len()
            ),
            "Поручения команды — награда каждый день".to_string(),
            "новые — в полночь по UTC".to_string(),
            Go::Contracts,
        ),
    ];
    root.spawn(Node {
        position_type: PositionType::Absolute,
        top: px(110),
        left: px(14),
        right: px(14),
        flex_direction: FlexDirection::Column,
        row_gap: px(10),
        ..default()
    })
    .with_children(|col| {
        for (art, title, sub, status, go) in cards {
            col.spawn((
                Node {
                    height: px(136),
                    padding: UiRect::all(px(10)),
                    column_gap: px(12),
                    align_items: AlignItems::Center,
                    border_radius: BorderRadius::all(px(12)),
                    ..default()
                },
                Skin(Look::Parchment),
                Outline::new(px(2), px(0), Color::srgb(0.72, 0.55, 0.26)),
            ))
            .with_children(|card| {
                card.spawn((
                    Node {
                        width: px(120),
                        height: px(114),
                        flex_shrink: 0.0,
                        border_radius: BorderRadius::all(px(8)),
                        ..default()
                    },
                    ImageNode::new(assets.load(format!("ui/act_{art}.png"))),
                ));
                card.spawn(Node {
                    flex_grow: 1.0,
                    flex_shrink: 1.0,
                    height: percent(100),
                    flex_direction: FlexDirection::Column,
                    justify_content: JustifyContent::SpaceBetween,
                    ..default()
                })
                .with_children(|c| {
                    c.spawn(style::ink(fonts, title, 19.0, INK));
                    c.spawn(text(fonts, sub, false, 13.0, INK_SOFT));
                    c.spawn(Node {
                        justify_content: JustifyContent::SpaceBetween,
                        align_items: AlignItems::Center,
                        column_gap: px(6),
                        ..default()
                    })
                    .with_children(|row| {
                        row.spawn((
                            Node {
                                flex_shrink: 1.0,
                                padding: UiRect::axes(px(10), px(3)),
                                border_radius: BorderRadius::all(px(10)),
                                ..default()
                            },
                            BackgroundColor(Color::srgb(0.24, 0.15, 0.09)),
                        ))
                        .with_child(style::heading(fonts, status, 12.0, BRIGHT));
                        go_button(row, fonts, go);
                    });
                });
            });
        }
        let mut soon = col.spawn((
            Node {
                padding: UiRect::axes(px(16), px(8)),
                flex_direction: FlexDirection::Column,
                border_radius: BorderRadius::all(px(10)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.06, 0.04, 0.03, 0.72)),
        ));
        soon.with_children(|c| {
            c.spawn(text(
                fonts,
                "Скоро: Руины старого мира и Арена капитанов",
                true,
                13.0,
                PARCHMENT,
            ));
        });
        later(&mut soon, "Руины и Арена откроются после запуска игры");
    });
}

#[derive(Clone, Copy)]
enum Go {
    Page(Page),
    Contracts,
}

fn go_button(row: &mut ChildSpawnerCommands, fonts: &Fonts, go: Go) {
    let mut button = row.spawn((
        Node {
            width: px(96),
            height: px(40),
            flex_shrink: 0.0,
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            ..default()
        },
        Skin(Look::Go),
    ));
    button.with_child(style::heading(fonts, "В путь »", 16.0, Color::WHITE));
    pressable(&mut button);
    button.observe(
        move |_: On<PointerClick>,
              mut page: ResMut<ActivitiesPage>,
              mut dirty: ResMut<ActivitiesDirty>,
              mut board: ResMut<BoardOpen>,
              mut next: ResMut<NextState<Screen>>| match go {
            Go::Page(p) => {
                page.0 = Some(p);
                dirty.0 = true;
            }
            // Доска контрактов — на главном экране.
            Go::Contracts => {
                board.0 = true;
                next.set(Screen::Menu);
            }
        },
    );
}

fn back_button(root: &mut ChildSpawnerCommands, fonts: &Fonts) {
    let mut button = root.spawn((
        Node {
            position_type: PositionType::Absolute,
            top: px(58),
            left: px(10),
            height: px(40),
            padding: UiRect::horizontal(px(14)),
            align_items: AlignItems::Center,
            ..default()
        },
        Skin(Look::Dark),
        GlobalZIndex(2),
    ));
    button.with_child(style::heading(fonts, "« Назад", 15.0, BRIGHT));
    pressable(&mut button);
    button.observe(
        |_: On<PointerClick>,
         mut page: ResMut<ActivitiesPage>,
         mut dirty: ResMut<ActivitiesDirty>,
         mut scroll: ResMut<ScrollY>| {
            page.0 = None;
            scroll.0 = 0.0;
            dirty.0 = true;
        },
    );
}

/// Открытый раздел: прокручиваемый список с одной секцией.
fn section_page(root: &mut ChildSpawnerCommands, view: &View, page: Page, floor: u32, boats: u32) {
    let (fonts, data, screen) = (&*view.fonts, &*view.data, &*view.screen);
    // Список прокручивается колесом и пальцем: разделы бывают длинными.
    let mut list = root.spawn((
        ActivitiesScroll,
        Node {
            position_type: PositionType::Absolute,
            top: px(108),
            left: px(10),
            right: px(10),
            bottom: px(88),
            padding: UiRect::vertical(px(4)),
            flex_direction: FlexDirection::Column,
            row_gap: px(10),
            overflow: Overflow::scroll_y(),
            ..default()
        },
        ScrollPosition(Vec2::new(0.0, view.scroll.0)),
    ));
    list.observe(
        |ev: On<PointerDrag>, mut lists: Query<&mut ScrollPosition, With<ActivitiesScroll>>| {
            for mut pos in &mut lists {
                pos.0.y = (pos.0.y - ev.delta.y).max(0.0);
            }
        },
    );
    list.with_children(|col| match page {
        Page::Event => {
            let owned: Vec<(String, String)> = screen
                .data
                .heroes
                .iter()
                .filter(|h| view.roster.owns(&h.id))
                .map(|h| (h.id.clone(), h.name.clone()))
                .collect();
            crate::events::event_card(
                col,
                fonts,
                &view.event_data,
                &view.storm,
                &view.week_events,
                &view.profile,
                &owned,
            );
        }
        Page::Abyss => abyss_card(col, fonts, data, screen, floor, view.abyss.best),
        Page::Expeditions => crate::expeditions::section(
            col,
            fonts,
            &view.exp_data,
            &view.exps,
            boats,
            screen,
            now(),
        ),
    });
}

fn abyss_card(
    col: &mut ChildSpawnerCommands,
    fonts: &Fonts,
    data: &AbyssData,
    screen: &BattleScreen,
    floor: u32,
    best: u32,
) {
    let enemies = data.enemies(floor);
    let names: Vec<String> = enemies
        .iter()
        .map(|(id, _)| screen.data.enemy(id).map_or(id.clone(), |e| e.name.clone()))
        .collect();
    let level = enemies.iter().map(|(_, l)| *l).max().unwrap_or(1);
    col.spawn((
        Node {
            padding: UiRect::axes(px(18), px(16)),
            flex_direction: FlexDirection::Column,
            row_gap: px(6),
            border: UiRect::all(px(2)),
            border_radius: BorderRadius::all(px(14)),
            ..default()
        },
        Skin(Look::Tile),
    ))
    .with_children(|card| {
        card.spawn(text(fonts, "Бездна облаков", true, 22.0, PARCHMENT));
        card.spawn(text(
            fonts,
            "Каждый этаж сложнее, каждый 10-й — босс. Награда за этаж — один раз.",
            false,
            13.0,
            MUTED,
        ));
        card.spawn(text(
            fonts,
            if data.is_boss(floor) {
                format!("Этаж {floor} — БОСС · лучший: {best}")
            } else {
                format!("Этаж {floor} · лучший: {best}")
            },
            true,
            17.0,
            GOLD,
        ));
        card.spawn(text(
            fonts,
            format!("{} · ур. {level}", names.join(" · ")),
            false,
            13.0,
            PARCHMENT,
        ));
        card.spawn(text(
            fonts,
            format!("Награда: {}", data.reward(floor).line()),
            false,
            13.0,
            PARCHMENT,
        ));
        let mut go = card.spawn((
            Node {
                margin: UiRect::top(px(6)),
                height: px(56),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                border: UiRect::all(px(2)),
                border_radius: BorderRadius::all(px(12)),
                ..default()
            },
            Skin(Look::Go),
        ));
        go.with_child(text(fonts, "Спуститься", true, 20.0, Color::WHITE));
        pressable(&mut go);
        go.observe(
            move |_: On<PointerClick>,
                  data: Res<AbyssData>,
                  mut voyage: ResMut<Voyage>,
                  mut requests: MessageWriter<VoyageRequest>| {
                *voyage = Voyage {
                    stage: None,
                    custom: Some(data.fight(floor)),
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

    #[test]
    fn deeper_is_harder() {
        let data = AbyssData::bundled();
        let lvl = |f: u32| data.enemies(f).iter().map(|(_, l)| *l).max().unwrap();
        assert!(lvl(20) > lvl(2));
        assert!(data.enemies(25).len() >= data.enemies(1).len());
    }

    #[test]
    fn boss_every_tenth_floor() {
        let data = AbyssData::bundled();
        assert!(data.enemies(10).iter().any(|(id, _)| *id == data.boss));
        assert!(!data.enemies(9).iter().any(|(id, _)| *id == data.boss));
        assert!(data.reward(10).doubloons > 0);
        assert_eq!(data.reward(9).doubloons, 0);
    }
}
