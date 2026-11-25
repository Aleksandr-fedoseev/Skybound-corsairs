//! Бездна облаков и экран «Активности».
//!
//! Бездна — бесконечная башня вниз: каждый этаж сложнее, каждый 10-й — босс. Награда за
//! этаж выдаётся один раз, при первом прохождении. Этажи собираются из правил
//! `data/abyss.json`, а не перечисляются вручную.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::Screen;
use crate::battle_screen::{BattleScreen, CustomFight, FightKind};
use crate::expeditions::{ExpeditionData, Expeditions};
use crate::menu::{GOLD, PARCHMENT, Voyage, bottom_nav, pressable, spawn_toast, text};
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
    Ruins,
    Arena,
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
    /// На столько уровней слабее враги, когда их становится на одного больше — без скачка.
    #[serde(default)]
    pub level_drop_per_extra: u32,
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
        let count = (self.base_count + (floor - 1) / self.more_every).min(self.max_count);
        let extra = count.saturating_sub(self.base_count);
        let level = (self.base_level + ((floor - 1) as f32 * self.level_per_floor) as u32)
            .saturating_sub(extra * self.level_drop_per_extra)
            .max(1);
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
            boost: Default::default(),
            wind: None,
        }
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
    ruins: (
        Res<'w, crate::ruins::RuinsData>,
        Res<'w, crate::ruins::Ruins>,
    ),
    arena: (
        Res<'w, crate::arena::ArenaData>,
        Res<'w, crate::arena::Arena>,
    ),
    progress: Res<'w, crate::menu::Progress>,
    power: crate::power::Power<'w>,
    ladder: Res<'w, crate::ladder::LadderData>,
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
                Some(Page::Ruins) => view.ruins.0.name.clone(),
                Some(Page::Arena) => view.arena.0.name.clone(),
                Some(Page::Expeditions) => "Экспедиции".to_string(),
            };
            style::title(root, fonts, &title, 56.0);
            if let Some(page) = view.page.0 {
                let id = match page {
                    Page::Event => view.event_data.current(),
                    Page::Abyss => "abyss",
                    Page::Ruins => "ruins",
                    Page::Arena => "arena",
                    Page::Expeditions => "expeditions",
                };
                crate::help::corner(root, fonts, id, 60.0, 12.0);
            }
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
            Page::Event,
        ),
        (
            "abyss",
            "Бездна облаков".to_string(),
            "Башня вверх без конца, каждый 10-й этаж — босс".to_string(),
            format!("этаж {floor}"),
            Page::Abyss,
        ),
        (
            "ruins",
            view.ruins.0.name.clone(),
            "10 залов, здоровье не восстанавливается".to_string(),
            ruins_status(view),
            Page::Ruins,
        ),
        (
            "arena",
            view.arena.0.name.clone(),
            "Отряды других капитанов, лига недели".to_string(),
            arena_status(view),
            Page::Arena,
        ),
        (
            "expeditions",
            "Экспедиции".to_string(),
            "Свободные герои уходят на шлюпках за добычей".to_string(),
            if back > 0 {
                format!("вернулись: {back} — забрать добычу!")
            } else {
                let free = boats.saturating_sub(view.exps.running.len() as u32);
                format!("шлюпки: свободно {free} из {boats}")
            },
            Page::Expeditions,
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
                    height: px(108),
                    padding: UiRect::axes(px(10), px(6)),
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
                        width: px(100),
                        height: px(95),
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
                    c.spawn(style::ink(fonts, title, 18.0, INK));
                    c.spawn(text(fonts, sub, false, 12.0, INK_SOFT));
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
    });
}

/// Мощь для полного спуска в Руины: по `curve -- ruins` до конца доходят на 4–5 уровней выше
/// рекомендуемого для следующего этапа.
fn ruins_need(view: &View) -> u32 {
    let stages = &view.screen.data.stages;
    let rec = stages
        .get(view.progress.cleared)
        .or(stages.last())
        .map_or(1, |s| s.recommended_level);
    view.power.recommended(rec + 5)
}

/// Таблица Арены этой недели — для мини-таблицы на странице Арены.
fn arena_ladder(view: &View) -> Vec<crate::ladder::Row> {
    let mut a = (*view.arena.1).clone();
    a.roll(&view.arena.0, crate::cabins::today());
    let progress = 1.0 - crate::events::week_left() as f32 / (7.0 * 86_400.0);
    view.ladder.arena(a.week, progress, a.rating)
}

fn ruins_status(view: &View) -> String {
    let (data, ruins) = (&*view.ruins.0, &*view.ruins.1);
    if view.progress.cleared < data.unlock_cleared {
        return "откроются после главы 1".to_string();
    }
    let this_week = ruins.week == crate::events::week();
    match ruins.run.as_ref().filter(|_| this_week) {
        Some(r) => format!("спуск идёт: этаж {}", r.floor),
        None => format!(
            "глубина недели: {} из {}",
            if this_week { ruins.best } else { 0 },
            data.depth()
        ),
    }
}

fn arena_status(view: &View) -> String {
    let (data, arena) = (&*view.arena.0, &*view.arena.1);
    if view.progress.cleared < data.unlock_cleared {
        return "откроется после главы 1".to_string();
    }
    let mut a = arena.clone();
    a.roll(data, crate::cabins::today());
    if a.pending > 0 {
        return format!("награда лиги: {} дублонов!", a.pending);
    }
    format!(
        "лига {} · {} очков",
        data.leagues[data.league(a.rating)].name,
        a.rating
    )
}

fn go_button(row: &mut ChildSpawnerCommands, fonts: &Fonts, go: Page) {
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
              mut dirty: ResMut<ActivitiesDirty>| {
            page.0 = Some(go);
            dirty.0 = true;
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
                &view.assets,
                fonts,
                &view.event_data,
                &view.storm,
                &view.week_events,
                &view.profile,
                &owned,
                (view.progress.cleared, &view.roster.team),
            );
        }
        Page::Abyss => abyss_card(col, &view.assets, fonts, data, floor),
        Page::Ruins => crate::ruins::section(
            col,
            &view.assets,
            fonts,
            &view.ruins.0,
            &view.ruins.1,
            screen,
            view.progress.cleared,
            &view.roster.team,
            (ruins_need(view), view.power.team()),
        ),
        Page::Arena => {
            let heroes: Vec<String> = screen.data.heroes.iter().map(|h| h.id.clone()).collect();
            crate::arena::section(
                col,
                &view.assets,
                fonts,
                &view.arena.0,
                &view.arena.1,
                &heroes,
                crate::arena::team_level(&view.roster),
                view.progress.cleared,
                (&screen.data, view.power.team()),
                &arena_ladder(view),
            );
        }
        Page::Expeditions => crate::expeditions::section(
            col,
            &view.assets,
            fonts,
            &view.exp_data,
            &view.exps,
            boats,
            screen,
            now(),
            &view.roster,
        ),
    });
}

fn abyss_card(
    col: &mut ChildSpawnerCommands,
    assets: &AssetServer,
    fonts: &Fonts,
    data: &AbyssData,
    floor: u32,
) {
    let ids: Vec<String> = data.enemies(floor).into_iter().map(|(id, _)| id).collect();
    let mut chips = Vec::new();
    if data.is_boss(floor) {
        chips.push(("БОСС".to_string(), Color::srgb(0.62, 0.2, 0.16)));
    }
    style::banner(col, assets, fonts, "abyss", format!("Этаж {floor}"), &chips);
    tile(col).with_children(|card| {
        crate::map::enemies_row(card, assets, fonts, &ids, None, PARCHMENT);
        style::reward_row(card, assets, fonts, "за этаж", &floor_items(data, floor));
        let mut go = card.spawn((
            Node {
                height: px(54),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                border: UiRect::all(px(2)),
                border_radius: BorderRadius::all(px(12)),
                ..default()
            },
            Skin(Look::Go),
        ));
        go.with_child(text(fonts, "Подняться", true, 20.0, Color::WHITE));
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
    // Впереди: следующие этажи снизу вверх — номер, босс, награда.
    tile(col).with_children(|card| {
        card.spawn(style::heading(fonts, "Впереди", 18.0, PARCHMENT));
        for f in floor + 1..=floor + 7 {
            card.spawn((
                Node {
                    height: px(42),
                    padding: UiRect::horizontal(px(10)),
                    column_gap: px(10),
                    align_items: AlignItems::Center,
                    border_radius: BorderRadius::all(px(10)),
                    ..default()
                },
                BackgroundColor(if data.is_boss(f) {
                    Color::srgba(0.62, 0.2, 0.16, 0.35)
                } else {
                    Color::srgba(1.0, 1.0, 1.0, 0.05)
                }),
            ))
            .with_children(|row| {
                row.spawn((
                    Node {
                        width: px(96),
                        ..default()
                    },
                    style::heading(fonts, format!("Этаж {f}"), 17.0, GOLD),
                ));
                if data.is_boss(f) {
                    style::pill(row, fonts, "БОСС", Color::srgb(0.62, 0.2, 0.16));
                }
                row.spawn(Node {
                    flex_grow: 1.0,
                    ..default()
                });
                style::costs(row, assets, fonts, &floor_items(data, f), 14.0, PARCHMENT);
            });
        }
    });
}

/// Награда этажа иконками.
fn floor_items(data: &AbyssData, floor: u32) -> Vec<(&'static str, u32)> {
    let r = data.reward(floor);
    [
        ("silver", r.silver),
        ("journals", r.journals),
        ("details", r.details),
        ("charts", r.charts),
        ("doubloons", r.doubloons),
        ("figureheads", r.figureheads),
    ]
    .into_iter()
    .filter(|(_, n)| *n > 0)
    .collect()
}

fn tile<'a>(col: &'a mut ChildSpawnerCommands) -> EntityCommands<'a> {
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
