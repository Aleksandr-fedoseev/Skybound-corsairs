//! Карта островов: этапы главы на островах вдоль пунктиров с картинки.
//!
//! Пройденные этапы можно перепроходить, следующий подсвечен, остальные закрыты. Тап по этапу
//! открывает карточку с врагами и кнопкой «В бой»; после боя игрок возвращается на карту.

use bevy::prelude::*;
use serde::Deserialize;

use crate::Screen;
use crate::battle_screen::BattleScreen;
use crate::convoys::{ConvoyData, Convoys};
use crate::menu::{
    BRASS, DARK_WOOD, GO, GO_EDGE, GOLD, MUTED, MenuData, PARCHMENT, Progress, Toast, Voyage,
    bottom_nav, pressable, spawn_toast, text,
};
use crate::ship::clock;
use crate::story::VoyageRequest;
use crate::style::{Look, Skin};
use crate::view::Fonts;

const CLEARED: Color = Color::srgb(0.30, 0.62, 0.38);
const CONVOY: Color = Color::srgb(0.20, 0.45, 0.62);
const LOCKED: Color = Color::srgb(0.36, 0.38, 0.43);
const NODE: f32 = 54.0;
const BOSS_NODE: f32 = 66.0;
const HOME_NODE: f32 = 64.0;
/// Следующий этап пульсирует: период (с) и размах (доля размера).
const PULSE_PERIOD: f32 = 1.4;
const PULSE: f32 = 0.08;

/// Карта: `data/map.json` — по карте на главу. Координаты — центр точки на экране 540×960.
#[derive(Resource, Deserialize)]
pub struct MapData {
    pub chapters: Vec<ChapterMap>,
}

#[derive(Deserialize)]
pub struct ChapterMap {
    pub name: String,
    pub background: String,
    pub home: Spot,
    /// Где на этой карте Торговый путь; без поля — как в `convoys.json`.
    #[serde(default)]
    pub convoy: Option<Pos>,
    pub stages: Vec<StageSpot>,
    pub locked: Vec<LockedSpot>,
}

#[derive(Deserialize, Clone, Copy)]
pub struct Pos {
    pub x: f32,
    pub y: f32,
}

#[derive(Deserialize)]
pub struct Spot {
    pub name: String,
    pub x: f32,
    pub y: f32,
}

#[derive(Deserialize)]
pub struct StageSpot {
    pub id: String,
    pub x: f32,
    pub y: f32,
}

/// Остров, который пока закрыт (следующая глава, ивент).
#[derive(Deserialize)]
pub struct LockedSpot {
    pub name: String,
    pub note: String,
    pub x: f32,
    pub y: f32,
    /// Остров ведёт в эту главу (номер в `chapters`), когда она открыта.
    #[serde(default)]
    pub chapter: Option<usize>,
}

impl MapData {
    pub fn bundled() -> Self {
        serde_json::from_str(include_str!("../../../data/map.json"))
            .expect("data/map.json корректен")
    }

    /// Глава, в которой этап `id`.
    fn chapter_of(&self, id: &str) -> usize {
        self.chapters
            .iter()
            .position(|c| c.stages.iter().any(|s| s.id == id))
            .unwrap_or(0)
    }

    /// Глава открыта, когда открыт её первый этап.
    fn open(&self, chapter: usize, stages: &[game_data::StageDef], cleared: usize) -> bool {
        let Some(first) = self.chapters.get(chapter).and_then(|c| c.stages.first()) else {
            return false;
        };
        stages
            .iter()
            .position(|s| s.id == first.id)
            .is_some_and(|i| i <= cleared)
    }
}

/// Какая глава открыта на карте.
#[derive(Resource, Default)]
pub struct MapChapter(pub usize);

/// Карту надо собрать заново (переключили главу).
#[derive(Resource, Default)]
struct MapDirty(bool);

#[derive(Component)]
struct MapRoot;

/// Этап, открытый в карточке (индекс в `stages.json`).
#[derive(Resource, Default)]
struct Selected(Option<usize>);

/// В карточке — конвой (а не этап).
#[derive(Resource, Default)]
struct ConvoyPick(bool);

/// Подпись конвоя на карте: сколько судов.
#[derive(Component)]
struct ConvoyLabel;

#[derive(Component)]
struct Pulse;

#[derive(Component)]
struct Card;

/// Кнопка «Рейд ×N» под «В бой»: видна только на пройденном этапе.
#[derive(Component)]
struct RaidButton;

/// Добыча последнего рейда — показывается в карточке этого этапа иконками.
#[derive(Resource, Default)]
struct LastRaid(Option<Raid>);

/// Рейд: этап, потраченные припасы и добыча иконками.
type Raid = (usize, u32, Vec<(&'static str, u32)>);

/// Точка на карте; число — нижний край кружка (точки под открытой карточкой прячутся).
#[derive(Component)]
struct MapSpot(f32);

/// Карточка этапа: постоянная высота, верхний край — чтобы знать, что она закрывает.
const CARD_H: f32 = 204.0;
const CARD_BOTTOM: f32 = 92.0;
const CARD_TOP: f32 = 960.0 - CARD_BOTTOM - CARD_H;

/// Содержимое карточки: пересобирается, когда меняется выбор или его состояние.
#[derive(Component)]
struct CardBody;

/// Таймер следующего судна в карточке конвоя.
#[derive(Component)]
struct CardTimer;

pub fn plugin(app: &mut App) {
    app.insert_resource(MapData::bundled())
        .init_resource::<Selected>()
        .init_resource::<ConvoyPick>()
        .init_resource::<MapChapter>()
        .init_resource::<MapDirty>()
        .init_resource::<LastRaid>()
        .add_systems(OnEnter(Screen::Map), enter_map)
        .add_systems(
            Update,
            (
                rebuild_map,
                refresh_card,
                refresh_raid,
                refresh_card_timer,
                refresh_convoy_label,
                pulse,
            )
                .chain()
                .run_if(in_state(Screen::Map)),
        );
}

/// На карту — к главе следующего этапа, с открытой карточкой этого этапа.
fn enter_map(
    map: Res<MapData>,
    progress: Res<Progress>,
    screen: Res<BattleScreen>,
    mut chapter: ResMut<MapChapter>,
    mut selected: ResMut<Selected>,
    mut pick: ResMut<ConvoyPick>,
    mut dirty: ResMut<MapDirty>,
) {
    let stages = &screen.data.stages;
    let next = progress.next_stage(stages.len());
    chapter.0 = map.chapter_of(&stages[next].id);
    selected.0 = Some(next);
    pick.0 = false;
    dirty.0 = true;
}

#[allow(clippy::too_many_arguments)]
fn rebuild_map(
    mut commands: Commands,
    mut dirty: ResMut<MapDirty>,
    roots: Query<Entity, With<MapRoot>>,
    assets: Res<AssetServer>,
    fonts: Res<Fonts>,
    map: Res<MapData>,
    chapter: Res<MapChapter>,
    progress: Res<Progress>,
    screen: Res<BattleScreen>,
    convoy: Res<ConvoyData>,
    profile: Res<crate::menu::Profile>,
) {
    if !dirty.0 {
        return;
    }
    dirty.0 = false;
    for root in &roots {
        commands.entity(root).despawn();
    }
    spawn_map(
        &mut commands,
        &assets,
        &fonts,
        &map,
        chapter.0,
        &progress,
        &screen,
        &convoy,
        &profile,
    );
}

#[allow(clippy::too_many_arguments)]
fn spawn_map(
    commands: &mut Commands,
    assets: &AssetServer,
    fonts: &Fonts,
    map: &MapData,
    chapter: usize,
    progress: &Progress,
    screen: &BattleScreen,
    convoy: &ConvoyData,
    profile: &crate::menu::Profile,
) {
    let stages = &screen.data.stages;
    let next = progress.next_stage(stages.len());
    let page = &map.chapters[chapter];
    let in_chapter: Vec<usize> = page
        .stages
        .iter()
        .filter_map(|spot| stages.iter().position(|s| s.id == spot.id))
        .collect();
    let done = in_chapter.iter().filter(|&&i| i < progress.cleared).count();

    commands
        .spawn((
            MapRoot,
            DespawnOnExit(Screen::Map),
            Node {
                width: percent(100),
                height: percent(100),
                ..default()
            },
            ImageNode {
                image_mode: NodeImageMode::Stretch,
                ..ImageNode::new(assets.load(page.background.clone()))
            },
        ))
        .with_children(|root| {
            crate::style::top_bar(
                root,
                assets,
                fonts,
                &[
                    ("doubloons", profile.doubloons),
                    ("silver", profile.silver),
                    ("planks", profile.planks),
                    ("supplies", profile.supplies),
                ],
            );
            crate::style::title(
                root,
                fonts,
                &format!("{} · {} из {}", page.name, done, in_chapter.len()),
                56.0,
            );
            chapter_arrows(root, fonts, map, chapter, stages, progress.cleared);
            // Справка — под стрелкой следующей главы.
            crate::help::corner(root, fonts, "map", 116.0, 13.0);
            home(root, assets, fonts, &page.home);
            for spot in &page.locked {
                let open = spot
                    .chapter
                    .is_some_and(|c| map.open(c, stages, progress.cleared));
                locked(root, fonts, spot, open);
            }
            let mut spot = convoy.spot.clone();
            if let Some(pos) = page.convoy {
                (spot.x, spot.y) = (pos.x, pos.y);
            }
            convoy_spot(root, assets, fonts, &spot);
            for spot in &page.stages {
                let Some(index) = stages.iter().position(|s| s.id == spot.id) else {
                    continue;
                };
                stage_node(
                    root,
                    fonts,
                    spot,
                    index,
                    &stages[index].name,
                    progress.cleared,
                    next,
                );
            }
            card(root, fonts);
            bottom_nav(root, assets, fonts, Screen::Map);
            spawn_toast(root, fonts);
        });
}

/// Стрелки «« »» по бокам заголовка: соседняя глава, если она открыта.
fn chapter_arrows(
    root: &mut ChildSpawnerCommands,
    fonts: &Fonts,
    map: &MapData,
    chapter: usize,
    stages: &[game_data::StageDef],
    cleared: usize,
) {
    for (left, to) in [(true, chapter.checked_sub(1)), (false, Some(chapter + 1))] {
        let Some(to) = to.filter(|&c| c < map.chapters.len()) else {
            continue;
        };
        let open = map.open(to, stages, cleared);
        let name = map.chapters[to].name.clone();
        let mut arrow = root.spawn((
            Node {
                position_type: PositionType::Absolute,
                top: px(60),
                left: if left { px(8) } else { auto() },
                right: if left { auto() } else { px(8) },
                width: px(48),
                height: px(44),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            Skin(if open { Look::Dark } else { Look::Off }),
        ));
        arrow.with_child(crate::style::heading(
            fonts,
            if left { "«" } else { "»" },
            24.0,
            if open { PARCHMENT } else { MUTED },
        ));
        pressable(&mut arrow);
        arrow.observe(
            move |_: On<PointerClick>,
                  mut chapter: ResMut<MapChapter>,
                  mut dirty: ResMut<MapDirty>,
                  mut selected: ResMut<Selected>,
                  mut toast: ResMut<Toast>| {
                if open {
                    chapter.0 = to;
                    selected.0 = None;
                    dirty.0 = true;
                } else {
                    toast.show(&format!("{name} откроется после босса предыдущей главы"));
                }
            },
        );
    }
}

/// Подпись под точкой на карте.
fn plate(parent: &mut ChildSpawnerCommands, fonts: &Fonts, title: &str, sub: &str, color: Color) {
    parent
        .spawn((
            Node {
                margin: UiRect::top(px(4)),
                padding: UiRect::axes(px(10), px(3)),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                border: UiRect::all(px(1)),
                border_radius: BorderRadius::all(px(8)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.07, 0.05, 0.03, 0.92)),
            BorderColor::all(BRASS),
            Pickable::IGNORE,
        ))
        .with_children(|p| {
            p.spawn(crate::style::heading(fonts, title, 14.0, color));
            if !sub.is_empty() {
                p.spawn(text(fonts, sub, false, 11.0, MUTED));
            }
        });
}

/// Колонка «круглая точка + подпись» с центром точки в (x, y).
fn spot_column(x: f32, y: f32, size: f32) -> impl Bundle {
    // Колонка шире точки, чтобы подпись не переносилась по словам.
    const LABEL_ROOM: f32 = 80.0;
    (
        MapSpot(y + size / 2.0),
        Node {
            position_type: PositionType::Absolute,
            left: px(x - (size + LABEL_ROOM) / 2.0),
            top: px(y - size / 2.0),
            width: px(size + LABEL_ROOM),
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::Center,
            ..default()
        },
    )
}

fn home(root: &mut ChildSpawnerCommands, assets: &AssetServer, fonts: &Fonts, spot: &Spot) {
    root.spawn((spot_column(spot.x, spot.y, HOME_NODE), Pickable::IGNORE))
        .with_children(|col| {
            let mut node = col.spawn((
                Node {
                    width: px(HOME_NODE),
                    height: px(HOME_NODE),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    border: UiRect::all(px(3)),
                    border_radius: BorderRadius::MAX,
                    ..default()
                },
                BackgroundColor(DARK_WOOD),
                BorderColor::all(GOLD),
            ));
            node.with_child((
                Node {
                    width: px(54),
                    height: px(54),
                    ..default()
                },
                ImageNode::new(assets.load("ui/nav_ship.png")),
                Pickable::IGNORE,
            ));
            pressable(&mut node);
            node.observe(|_: On<PointerClick>, mut next: ResMut<NextState<Screen>>| {
                next.set(Screen::Menu);
            });
            plate(col, fonts, &spot.name, "на корабль", GOLD);
        });
}

/// Закрытый остров. Если он ведёт в уже открытую главу — тап переключает карту на неё.
fn locked(root: &mut ChildSpawnerCommands, fonts: &Fonts, spot: &LockedSpot, open: bool) {
    let message = format!("{} — {}, пока закрыто", spot.name, spot.note);
    let go = spot.chapter.filter(|_| open);
    let mut column = root.spawn(spot_column(spot.x, spot.y, NODE));
    column.with_children(|col| {
        let note = if go.is_some() {
            "открыта »"
        } else {
            &spot.note
        };
        plate(
            col,
            fonts,
            &spot.name,
            note,
            if go.is_some() { GOLD } else { MUTED },
        );
    });
    column.observe(
        move |_: On<PointerClick>,
              mut toast: ResMut<Toast>,
              mut chapter: ResMut<MapChapter>,
              mut dirty: ResMut<MapDirty>,
              mut selected: ResMut<Selected>| {
            match go {
                Some(c) => {
                    chapter.0 = c;
                    selected.0 = None;
                    dirty.0 = true;
                }
                None => toast.show(&message),
            }
        },
    );
}

fn stage_node(
    root: &mut ChildSpawnerCommands,
    fonts: &Fonts,
    spot: &StageSpot,
    index: usize,
    name: &str,
    cleared: usize,
    next: usize,
) {
    let boss = spot.id.ends_with("boss");
    let size = if boss { BOSS_NODE } else { NODE };
    let open = index <= cleared;
    let (fill, edge) = if index < cleared {
        (CLEARED, BRASS)
    } else if open {
        (GO, GOLD)
    } else {
        (LOCKED, Color::srgb(0.6, 0.6, 0.62))
    };
    let label = if boss {
        "БОСС".to_string()
    } else {
        spot.id.clone()
    };
    let sub = if index < cleared {
        "пройден"
    } else if index == next && open {
        "новый"
    } else {
        ""
    };

    root.spawn((spot_column(spot.x, spot.y, size), Pickable::IGNORE))
        .with_children(|col| {
            let mut node = col.spawn((
                Node {
                    width: px(size),
                    height: px(size),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    border: UiRect::all(px(3)),
                    border_radius: BorderRadius::MAX,
                    ..default()
                },
                BackgroundColor(fill),
                BorderColor::all(edge),
                Outline::new(px(2), px(0), Color::srgb(0.30, 0.20, 0.08)),
            ));
            node.with_child(crate::style::heading(
                fonts,
                label,
                if boss { 16.0 } else { 18.0 },
                Color::WHITE,
            ));
            if index == next && open {
                node.insert(Pulse);
            }
            pressable(&mut node);
            node.observe(
                move |_: On<PointerClick>,
                      mut selected: ResMut<Selected>,
                      mut pick: ResMut<ConvoyPick>,
                      mut toast: ResMut<Toast>,
                      screen: Res<BattleScreen>| {
                    if open {
                        selected.0 = Some(index);
                        pick.0 = false;
                    } else {
                        let before = &screen.data.stages[cleared.min(index)].name;
                        toast.show(&format!("Сначала пройдите «{before}»"));
                    }
                },
            );
            plate(col, fonts, name, sub, if open { PARCHMENT } else { MUTED });
        });
}

fn card(root: &mut ChildSpawnerCommands, fonts: &Fonts) {
    root.spawn((
        Card,
        Node {
            position_type: PositionType::Absolute,
            left: px(10),
            bottom: px(CARD_BOTTOM),
            width: px(520),
            height: px(CARD_H),
            padding: UiRect::axes(px(18), px(10)),
            column_gap: px(12),
            align_items: AlignItems::Center,
            ..default()
        },
        Skin(Look::Tile),
        Visibility::Hidden,
    ))
    .with_children(|card| {
        // Закрыть карточку — открыть то, что под ней.
        let mut close = card.spawn((
            Node {
                position_type: PositionType::Absolute,
                top: px(-14),
                right: px(-6),
                width: px(36),
                height: px(36),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            Skin(Look::Dark),
        ));
        close.with_child(crate::style::heading(fonts, "×", 22.0, PARCHMENT));
        pressable(&mut close);
        close.observe(
            |_: On<PointerClick>, mut selected: ResMut<Selected>, mut pick: ResMut<ConvoyPick>| {
                selected.0 = None;
                pick.0 = false;
            },
        );
        card.spawn((
            CardBody,
            Node {
                flex_grow: 1.0,
                flex_shrink: 1.0,
                flex_direction: FlexDirection::Column,
                row_gap: px(7),
                ..default()
            },
        ));
        // Справа: «В бой» и под ним «Рейд ×N» — мгновенные победы на пройденном этапе.
        card.spawn(Node {
            flex_direction: FlexDirection::Column,
            row_gap: px(10),
            flex_shrink: 0.0,
            ..default()
        })
        .with_children(|col| {
            let mut go = col.spawn((
                Node {
                    width: px(130),
                    height: px(58),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    ..default()
                },
                Skin(Look::Go),
            ));
            go.with_child(crate::style::heading(fonts, "В бой", 24.0, Color::WHITE));
            pressable(&mut go);
            go.observe(
                |_: On<PointerClick>,
                 selected: Res<Selected>,
                 pick: Res<ConvoyPick>,
                 convoy: (Res<ConvoyData>, Res<Convoys>, Res<Progress>),
                 mut toast: ResMut<Toast>,
                 mut voyage: ResMut<Voyage>,
                 mut requests: MessageWriter<VoyageRequest>| {
                    let (data, convoys, progress) = convoy;
                    if pick.0 {
                        if convoys.ready == 0 {
                            toast.show(&format!(
                                "Конвоев нет — следующий через {}",
                                clock(convoys.next_in(&data))
                            ));
                            return;
                        }
                        *voyage = Voyage {
                            stage: None,
                            custom: Some(data.fight(&progress)),
                            from: Screen::Map,
                        };
                        requests.write(VoyageRequest);
                    } else if let Some(stage) = selected.0 {
                        *voyage = Voyage {
                            stage: Some(stage),
                            custom: None,
                            from: Screen::Map,
                        };
                        requests.write(VoyageRequest);
                    }
                },
            );
            let mut raid = col.spawn((
                RaidButton,
                Node {
                    width: px(130),
                    height: px(46),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    ..default()
                },
                Skin(Look::Dark),
                Visibility::Hidden,
            ));
            raid.with_child(crate::style::heading(fonts, "Рейд", 18.0, Color::WHITE));
            pressable(&mut raid);
            raid.observe(
                |_: On<PointerClick>, selected: Res<Selected>, mut commands: Commands| {
                    if let Some(stage) = selected.0 {
                        commands.run_system_cached_with(raid_stage, stage);
                    }
                },
            );
        });
    });
}

/// Что показывает карточка: если изменилось — собрать её заново.
#[derive(PartialEq, Clone)]
enum CardKey {
    Hidden,
    Stage {
        stage: usize,
        wins: u32,
        supplies: u32,
        team: u32,
    },
    Convoy {
        ready: u32,
        team: u32,
    },
}

#[allow(clippy::too_many_arguments)]
fn refresh_card(
    mut commands: Commands,
    selected: Res<Selected>,
    pick: Res<ConvoyPick>,
    convoy: (Res<ConvoyData>, Res<Convoys>),
    look: (Res<AssetServer>, Res<Fonts>, Res<crate::events::EventData>),
    screen: Res<BattleScreen>,
    progress: Res<Progress>,
    menu: Res<MenuData>,
    power: crate::power::Power,
    last_raid: Res<LastRaid>,
    supply: (Res<crate::supplies::SupplyData>, Res<crate::menu::Profile>),
    mut cards: Query<&mut Visibility, With<Card>>,
    bodies: Query<(Entity, Option<&Children>), With<CardBody>>,
    mut spots: Query<(&MapSpot, &mut Visibility), Without<Card>>,
    mut shown: Local<Option<CardKey>>,
) {
    // Точки, которые закрыла бы открытая карточка, прячем — не торчат из-под неё.
    let open = selected.0.is_some() || pick.0;
    for (MapSpot(bottom), mut vis) in &mut spots {
        vis.set_if_neq(if open && *bottom > CARD_TOP - 6.0 {
            Visibility::Hidden
        } else {
            Visibility::Inherited
        });
    }
    let (assets, fonts, events) = look;
    let (data, convoys) = convoy;
    let (supply, profile) = supply;
    // Мощь отряда против рекомендуемой: ниже — бой, скорее всего, не выиграть.
    let team_level = power.team();
    let key = if pick.0 {
        CardKey::Convoy {
            ready: convoys.ready,
            team: team_level,
        }
    } else if let Some(stage) = selected.0 {
        CardKey::Stage {
            stage,
            wins: progress.wins_today(stage, crate::cabins::today()),
            supplies: profile.supplies,
            team: team_level,
        }
    } else {
        CardKey::Hidden
    };
    let Some((body, children)) = bodies.iter().next() else {
        return;
    };
    // Пустое тело — карту только что собрали заново: заполнить, даже если выбор тот же.
    let empty = children.is_none_or(|c| c.is_empty());
    if shown.as_ref() == Some(&key) && (!empty || key == CardKey::Hidden) {
        return;
    }
    for mut vis in &mut cards {
        vis.set_if_neq(if key == CardKey::Hidden {
            Visibility::Hidden
        } else {
            Visibility::Inherited
        });
    }
    *shown = Some(key.clone());
    commands.entity(body).despawn_children();
    let ids_of = |list: &[(String, u32)]| list.iter().map(|(id, _)| id.clone()).collect::<Vec<_>>();
    commands.entity(body).with_children(|b| match key {
        CardKey::Hidden => {}
        CardKey::Convoy { ready, team } => {
            let tier = data.tier(&progress);
            b.spawn(crate::style::heading(
                &fonts,
                format!("Конвой · {}", tier.name),
                22.0,
                TITLE,
            ));
            enemies_row(b, &assets, &fonts, &ids_of(&tier.enemies), None, PARCHMENT);
            chips(b, |row| {
                crate::power::need_chip(
                    row,
                    &assets,
                    &fonts,
                    power.recommended(tier.recommended_level),
                    team,
                );
                chip(
                    row,
                    &fonts,
                    &format!("суда {ready}/{}", data.max_ready),
                    if ready > 0 { READY_BG } else { WEAK_BG },
                    None,
                );
                row.spawn((CardTimer, text(&fonts, "", false, 13.0, PARCHMENT)));
            });
            let k = events.convoy_multiplier();
            rewards(
                b,
                &assets,
                &fonts,
                &[("planks", tier.planks * k), ("silver", tier.silver * k)],
                if k > 1 {
                    "попутный ветер ×2"
                } else {
                    ""
                },
            );
        }
        CardKey::Stage {
            stage, wins, team, ..
        } => {
            let def = &screen.data.stages[stage];
            let title = if def.id.ends_with("boss") {
                format!("Босс · {}", def.name)
            } else {
                format!("{} · {}", def.id, def.name)
            };
            b.spawn(crate::style::heading(&fonts, title, 22.0, TITLE));
            let ids: Vec<String> = def.enemies.iter().map(|e| e.enemy.clone()).collect();
            enemies_row(b, &assets, &fonts, &ids, def.wind, PARCHMENT);
            chips(b, |row| {
                crate::power::need_chip(
                    row,
                    &assets,
                    &fonts,
                    power.recommended(def.recommended_level),
                    team,
                );
                // Счётчик побед за день — только на пройденном этапе: лимит касается повторов.
                if stage < progress.cleared {
                    chip(
                        row,
                        &fonts,
                        &format!("сегодня {wins}/{}", supply.daily_limit()),
                        if wins < supply.daily_limit() {
                            PLAIN_BG
                        } else {
                            WEAK_BG
                        },
                        None,
                    );
                }
            });
            // Только что был рейд — его добыча иконками.
            if let Some((_, spent, items)) = last_raid.0.as_ref().filter(|(s, ..)| *s == stage) {
                b.spawn(Node {
                    column_gap: px(8),
                    align_items: AlignItems::Center,
                    ..default()
                })
                .with_children(|row| {
                    row.spawn(text(&fonts, "рейд:", false, 13.0, MUTED));
                    crate::style::costs(row, &assets, &fonts, items, 15.0, GOLD);
                    row.spawn(crate::style::icon(&assets, "supplies", 19.0));
                    row.spawn(crate::style::heading(
                        &fonts,
                        format!("−{spent}"),
                        16.0,
                        Color::srgb(1.0, 0.55, 0.45),
                    ));
                });
            }
            let reward = &menu.victory_reward;
            match supply.percent(wins) {
                None => {
                    b.spawn(text(
                        &fonts,
                        "на сегодня всё — завтра снова",
                        false,
                        13.0,
                        MUTED,
                    ));
                }
                Some(p) => {
                    let mut items = vec![
                        ("silver", reward.silver * p / 100),
                        ("planks", reward.planks * p / 100),
                        ("journals", reward.journals * p / 100),
                    ];
                    if stage >= progress.cleared {
                        items.push(("doubloons", menu.first_clear_doubloons));
                    }
                    let note = if p < 100 {
                        format!("повтор {p}%")
                    } else {
                        String::new()
                    };
                    rewards(b, &assets, &fonts, &items, &note);
                }
            }
        }
    });
}

/// Таймер следующего судна — тикает каждую секунду.
fn refresh_card_timer(
    convoy: (Res<ConvoyData>, Res<Convoys>),
    mut timers: Query<&mut Text, With<CardTimer>>,
) {
    let (data, convoys) = convoy;
    let value = if convoys.ready >= data.max_ready {
        "очередь полна".to_string()
    } else {
        format!("следующее через {}", clock(convoys.next_in(&data)))
    };
    for mut text in &mut timers {
        if text.0 != value {
            text.0 = value.clone();
        }
    }
}

const TITLE: Color = Color::srgb(1.0, 0.93, 0.78);
const READY_BG: Color = Color::srgb(0.18, 0.42, 0.22);
const WEAK_BG: Color = Color::srgb(0.55, 0.16, 0.12);
const PLAIN_BG: Color = Color::srgba(1.0, 1.0, 1.0, 0.10);

/// Враги медальонами с числом, рядом — плашка ветра боя.
pub(crate) fn enemies_row(
    parent: &mut ChildSpawnerCommands,
    assets: &AssetServer,
    fonts: &Fonts,
    ids: &[String],
    wind: Option<battle_core::Element>,
    color: Color,
) {
    let mut groups: Vec<(&str, usize)> = Vec::new();
    for id in ids {
        match groups.iter_mut().find(|(g, _)| *g == id.as_str()) {
            Some((_, n)) => *n += 1,
            None => groups.push((id.as_str(), 1)),
        }
    }
    parent
        .spawn(Node {
            column_gap: px(10),
            align_items: AlignItems::Center,
            flex_wrap: FlexWrap::Wrap,
            row_gap: px(4),
            ..default()
        })
        .with_children(|row| {
            for (id, n) in groups {
                row.spawn(Node {
                    align_items: AlignItems::Center,
                    column_gap: px(2),
                    ..default()
                })
                .with_children(|g| {
                    g.spawn((
                        Node {
                            width: px(40),
                            height: px(40),
                            ..default()
                        },
                        ImageNode::new(assets.load(format!("ui/unit_{id}.png"))),
                    ));
                    g.spawn(crate::style::heading(fonts, format!("×{n}"), 17.0, color));
                });
            }
            if let Some(w) = wind {
                let label = format!(
                    "ветер боя: {} +{}%",
                    crate::crew::element_label(w),
                    (battle_core::WIND_BONUS * 100.0).round()
                );
                row.spawn((
                    Node {
                        padding: UiRect::axes(px(10), px(4)),
                        border_radius: BorderRadius::all(px(12)),
                        ..default()
                    },
                    BackgroundColor(crate::view::element_color(w).with_alpha(0.85)),
                ))
                .with_child(crate::style::heading(
                    fonts,
                    label,
                    14.0,
                    Color::srgb(0.08, 0.06, 0.05),
                ));
            }
        });
}

fn chips(parent: &mut ChildSpawnerCommands, fill: impl FnOnce(&mut ChildSpawnerCommands)) {
    parent
        .spawn(Node {
            column_gap: px(6),
            align_items: AlignItems::Center,
            flex_wrap: FlexWrap::Wrap,
            row_gap: px(4),
            ..default()
        })
        .with_children(fill);
}

/// Плашка: цветной фон, при желании — иконка ресурса слева.
fn chip(
    parent: &mut ChildSpawnerCommands,
    fonts: &Fonts,
    label: &str,
    bg: Color,
    icon: Option<(&str, &AssetServer)>,
) {
    parent
        .spawn((
            Node {
                padding: UiRect::axes(px(9), px(4)),
                column_gap: px(4),
                align_items: AlignItems::Center,
                border_radius: BorderRadius::all(px(12)),
                ..default()
            },
            BackgroundColor(bg),
        ))
        .with_children(|c| {
            if let Some((kind, assets)) = icon {
                c.spawn(crate::style::icon(assets, kind, 20.0));
            }
            c.spawn(crate::style::heading(
                fonts,
                label.to_string(),
                15.0,
                Color::WHITE,
            ));
        });
}

/// Награда иконками: «серебро 120 · доски 40 · журналы 100».
fn rewards(
    parent: &mut ChildSpawnerCommands,
    assets: &AssetServer,
    fonts: &Fonts,
    items: &[(&str, u32)],
    note: &str,
) {
    parent
        .spawn(Node {
            column_gap: px(8),
            align_items: AlignItems::Center,
            ..default()
        })
        .with_children(|row| {
            row.spawn(text(fonts, "награда", false, 13.0, MUTED));
            crate::style::costs(row, assets, fonts, items, 15.0, PARCHMENT);
            if !note.is_empty() {
                row.spawn(text(fonts, note.to_string(), false, 13.0, MUTED));
            }
        });
}

/// Точка конвоя на карте: шлюпка и подпись с числом судов.
fn convoy_spot(
    root: &mut ChildSpawnerCommands,
    assets: &AssetServer,
    fonts: &Fonts,
    spot: &crate::convoys::Spot,
) {
    root.spawn((spot_column(spot.x, spot.y, NODE), Pickable::IGNORE))
        .with_children(|col| {
            let mut node = col.spawn((
                Node {
                    width: px(NODE),
                    height: px(NODE),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    border: UiRect::all(px(3)),
                    border_radius: BorderRadius::MAX,
                    ..default()
                },
                BackgroundColor(CONVOY),
                BorderColor::all(GO_EDGE),
            ));
            node.with_child((
                Node {
                    width: px(44),
                    height: px(36),
                    ..default()
                },
                ImageNode::new(assets.load("menu/module_boats.png")),
                Pickable::IGNORE,
            ));
            pressable(&mut node);
            node.observe(
                |_: On<PointerClick>,
                 mut selected: ResMut<Selected>,
                 mut pick: ResMut<ConvoyPick>| {
                    selected.0 = None;
                    pick.0 = true;
                },
            );
            col.spawn((
                Node {
                    margin: UiRect::top(px(4)),
                    padding: UiRect::axes(px(10), px(3)),
                    flex_direction: FlexDirection::Column,
                    align_items: AlignItems::Center,
                    border: UiRect::all(px(1)),
                    border_radius: BorderRadius::all(px(8)),
                    ..default()
                },
                BackgroundColor(Color::srgba(0.07, 0.05, 0.03, 0.92)),
                BorderColor::all(BRASS),
                Pickable::IGNORE,
            ))
            .with_children(|p| {
                p.spawn(crate::style::heading(
                    fonts,
                    spot.name.clone(),
                    14.0,
                    PARCHMENT,
                ));
                p.spawn((ConvoyLabel, text(fonts, "", false, 11.0, GOLD)));
            });
        });
}

fn refresh_convoy_label(convoys: Res<Convoys>, mut labels: Query<&mut Text, With<ConvoyLabel>>) {
    for mut text in &mut labels {
        let value = format!("судов: {}", convoys.ready);
        if text.0 != value {
            text.0 = value;
        }
    }
}

fn pulse(time: Res<Time>, mut nodes: Query<&mut UiTransform, With<Pulse>>) {
    let t = time.elapsed_secs() / PULSE_PERIOD * std::f32::consts::TAU;
    for mut transform in &mut nodes {
        // Пока палец на кнопке, она сжата — пульс её не разжимает.
        if transform.scale.x > 0.95 {
            transform.scale = Vec2::splat(1.0 + PULSE * (0.5 + 0.5 * t.sin()));
        }
    }
}

/// Сколько рейдов можно сделать на этапе прямо сейчас: дневной лимит и припасы.
fn raids_left(
    stage: usize,
    progress: &Progress,
    supplies: u32,
    supply: &crate::supplies::SupplyData,
) -> u32 {
    let today = crate::cabins::today();
    let wins = progress.wins_today(stage, today);
    let by_limit = supply.daily_limit().saturating_sub(wins);
    let by_supplies = supplies / supply.cost.max(1);
    by_limit.min(by_supplies)
}

/// «Рейд ×N»: столько побед на пройденном этапе разом, без боя — те же припасы и добыча.
#[allow(clippy::too_many_arguments)]
fn raid_stage(
    In(stage): In<usize>,
    data: Res<MenuData>,
    mut progress: ResMut<Progress>,
    mut spoils: crate::menu::Spoils,
    supply: Res<crate::supplies::SupplyData>,
    mut deeds: MessageWriter<crate::contracts::Deed>,
    mut toast: ResMut<Toast>,
    mut last: ResMut<LastRaid>,
    mut sfx: MessageWriter<crate::audio::Sfx>,
    mut dirty: ResMut<MapDirty>,
) {
    if stage >= progress.cleared {
        return;
    }
    let n = raids_left(stage, &progress, spoils.profile.supplies, &supply);
    if n == 0 {
        let wins = progress.wins_today(stage, crate::cabins::today());
        if let Err(why) = supply.check(&spoils.profile, wins, false) {
            toast.show(&why);
        }
        return;
    }
    let (mut silver, mut planks, mut journals, mut spent) = (0, 0, 0, 0);
    for _ in 0..n {
        let win = crate::menu::stage_win(stage, false, &data, &mut progress, &mut spoils, &supply);
        silver += win.reward.silver;
        planks += win.reward.planks;
        journals += win.reward.journals;
        spent += win.cost;
        deeds.write(crate::contracts::Deed("win"));
    }
    last.0 = Some((
        stage,
        spent,
        vec![
            ("silver", silver),
            ("planks", planks),
            ("journals", journals),
        ],
    ));
    sfx.write(crate::audio::Sfx("coin"));
    // Карта целиком — чтобы полоса ресурсов вверху показала добычу.
    dirty.0 = true;
}

/// Кнопка рейда видна на пройденном этапе; подпись — сколько рейдов доступно.
fn refresh_raid(
    selected: Res<Selected>,
    pick: Res<ConvoyPick>,
    progress: Res<Progress>,
    profile: Res<crate::menu::Profile>,
    supply: Res<crate::supplies::SupplyData>,
    mut buttons: Query<(&mut Visibility, &mut Skin, &Children), With<RaidButton>>,
    mut texts: Query<&mut Text>,
) {
    let stage = selected.0.filter(|s| !pick.0 && *s < progress.cleared);
    for (mut vis, mut skin, children) in &mut buttons {
        let Some(stage) = stage else {
            vis.set_if_neq(Visibility::Hidden);
            continue;
        };
        vis.set_if_neq(Visibility::Inherited);
        let n = raids_left(stage, &progress, profile.supplies, &supply);
        let look = if n > 0 { Look::Go } else { Look::Off };
        if skin.0 != look {
            skin.0 = look;
        }
        let label = if n > 1 {
            format!("Рейд ×{n}")
        } else {
            "Рейд".to_string()
        };
        for child in children {
            if let Ok(mut text) = texts.get_mut(*child)
                && text.0 != label
            {
                text.0 = label.clone();
            }
        }
    }
}
