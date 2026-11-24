//! Интерфейс поверх поля боя, в духе Zero City: сверху — здоровье отрядов вокруг значка хода
//! и очередь ходов портретами; снизу — круглые портреты ульт с заливкой энергии, очки умений,
//! залп корабля и кнопки АВТО и скорости сбоку. Само поле с моделями — в `battlefield.rs`.

use battle_core::{BROADSIDE_MAX, ENERGY_MAX, Element, Outcome, Side, UnitId};
use bevy::prelude::*;

use crate::style::{Look, Skin};

use crate::Screen;
use crate::battle_screen::BattleScreen;
use crate::battlefield;

const SKY: Color = Color::srgb(0.53, 0.75, 0.92);
const PANEL: Color = Color::srgba(0.05, 0.10, 0.20, 0.80);
const HP_ALLY: Color = Color::srgb(0.38, 0.85, 0.45);
const HP_ENEMY: Color = Color::srgb(0.92, 0.32, 0.30);
const ENERGY: Color = Color::srgb(0.35, 0.65, 1.0);
const ACCENT: Color = Color::srgb(1.0, 0.85, 0.35);
const BROADSIDE: Color = Color::srgb(1.0, 0.55, 0.20);
const SKILL: Color = Color::srgb(0.55, 0.85, 1.0);
const BRASS_EDGE: Color = Color::srgb(0.79, 0.59, 0.25);
const WOOD_DARK: Color = Color::srgb(0.10, 0.07, 0.04);
const BRIGHT: Color = Color::srgb(1.0, 0.93, 0.78);
const TURN_CHIPS: usize = 8;
const CHIP: f32 = 50.0;
const ULT_SIZE: f32 = 92.0;
const SMALL_BUTTON: f32 = 76.0;
const SKILL_BADGE: f32 = 32.0;
/// Готовая ульта: портрет пульсирует.
const READY_PULSE: f32 = 0.06;

pub fn element_color(element: Element) -> Color {
    match element {
        Element::Wind => Color::srgb(0.45, 0.85, 0.62),
        Element::Lightning => Color::srgb(0.70, 0.55, 1.0),
        Element::Powder => Color::srgb(1.0, 0.60, 0.28),
        Element::Steel => Color::srgb(0.62, 0.68, 0.75),
        Element::Mist => Color::srgb(0.80, 0.88, 0.98),
    }
}

/// Отступ панели итога боя от верха экрана (под полосами здоровья и очередью ходов).
const RESULT_TOP: f32 = 140.0;

#[derive(Resource)]
pub struct Fonts {
    pub regular: Handle<Font>,
    pub bold: Handle<Font>,
    /// Rubik ExtraBold: заголовки, кнопки, крупные цифры.
    pub display: Handle<Font>,
    /// Rubik SemiBold: имена, подписи и числа на пергаменте.
    pub serif: Handle<Font>,
}

impl Fonts {
    pub fn display(&self, size: f32) -> TextFont {
        TextFont {
            font: self.display.clone().into(),
            font_size: FontSize::Px(size),
            ..default()
        }
    }

    pub fn serif(&self, size: f32) -> TextFont {
        TextFont {
            font: self.serif.clone().into(),
            font_size: FontSize::Px(size),
            ..default()
        }
    }

    pub fn text(&self, bold: bool, size: f32) -> TextFont {
        TextFont {
            font: if bold { &self.bold } else { &self.regular }.clone().into(),
            font_size: FontSize::Px(size),
            ..default()
        }
    }
}

fn shadow() -> TextShadow {
    TextShadow {
        offset: Vec2::new(1.0, 1.0),
        color: Color::srgba(0.0, 0.0, 0.0, 0.7),
    }
}

#[derive(Component)]
pub struct ScreenRoot;

#[derive(Component, Clone, Copy)]
pub struct SquadFill(pub Side);

#[derive(Component, Clone, Copy)]
pub struct SquadText(pub Side);

#[derive(Component)]
pub struct TurnBadge;

#[derive(Component, Clone, Copy)]
pub struct TurnChip(pub usize);

/// Полоса через экран для ульт и залпа.
#[derive(Component)]
pub struct UltBand;

#[derive(Component)]
pub struct UltBandText;

/// Портрет героя внизу: тап — ульта, даже вне его хода.
#[derive(Component, Clone, Copy)]
pub struct UltButton(pub UnitId);

/// Заливка энергии внутри портрета (снизу вверх).
#[derive(Component, Clone, Copy)]
pub struct UltFill(pub UnitId);

#[derive(Component, Clone, Copy)]
pub struct UltLabel(pub UnitId);

/// Значок на портрете: через сколько ходов спецатака.
#[derive(Component, Clone, Copy)]
pub struct SkillBadge(pub UnitId);

#[derive(Component)]
pub struct BroadsideFill;

#[derive(Component)]
pub struct BroadsideLabel;

#[derive(Component, Clone, Copy, PartialEq, Eq)]
pub enum ButtonKind {
    Auto,
    Speed,
    Pause,
}

#[derive(Component)]
pub struct ResultOverlay;

#[derive(Component)]
pub struct ResultText;

/// Строка под итогом боя: добыча и что открылось (заполняет `menu.rs`).
#[derive(Component)]
pub struct ResultNote;

/// Заголовок боя: этап и ветер (ветер меняется посреди боя).
#[derive(Component)]
pub struct BattleTitle;

pub fn refresh_title(screen: Res<BattleScreen>, mut titles: Query<&mut Text, With<BattleTitle>>) {
    let title = screen.title();
    for mut text in &mut titles {
        if text.0 != title {
            text.0 = title.clone();
        }
    }
}

/// Строка тестовой сцены (только с фичей dev).
#[cfg(feature = "dev")]
#[derive(Component)]
pub struct DevText;

impl FromWorld for Fonts {
    fn from_world(world: &mut World) -> Self {
        let assets = world.resource::<AssetServer>();
        Self {
            regular: assets.load("fonts/Rubik-Medium.ttf"),
            bold: assets.load("fonts/Rubik-SemiBold.ttf"),
            display: assets.load("fonts/Rubik-ExtraBold.ttf"),
            serif: assets.load("fonts/Rubik-SemiBold.ttf"),
        }
    }
}

pub fn setup(mut commands: Commands) {
    commands.insert_resource(ClearColor(SKY));
    commands.spawn(Camera2d);
}

/// Собрать экран заново: новый бой с другим составом или другая раскладка поля.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub fn rebuild(
    mut commands: Commands,
    mut screen: ResMut<BattleScreen>,
    fonts: Res<Fonts>,
    assets: Res<AssetServer>,
    voyage: Res<crate::menu::Voyage>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    roots: Query<Entity, Or<(With<ScreenRoot>, With<battlefield::FieldRoot>)>>,
) {
    if !screen.rebuild {
        return;
    }
    screen.rebuild = false;
    for root in &roots {
        commands.entity(root).despawn();
    }
    battlefield::spawn_field(
        &mut commands,
        &assets,
        &mut meshes,
        &mut materials,
        &screen,
        &fonts,
    );
    spawn_hud(&mut commands, &assets, &screen, &fonts, voyage.from);
}

fn spawn_hud(
    commands: &mut Commands,
    assets: &AssetServer,
    screen: &BattleScreen,
    fonts: &Fonts,
    back: Screen,
) {
    let heroes: Vec<UnitId> = screen
        .battle
        .units()
        .iter()
        .filter(|u| u.side == Side::Ally && u.def.ultimate.is_some())
        .map(|u| u.id)
        .collect();

    commands
        .spawn((
            ScreenRoot,
            Node {
                width: percent(100),
                height: percent(100),
                flex_direction: FlexDirection::Column,
                justify_content: JustifyContent::SpaceBetween,
                padding: UiRect::all(px(8)),
                ..default()
            },
            Pickable::IGNORE,
        ))
        .with_children(|root| {
            root.spawn(Node {
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                row_gap: px(5),
                ..default()
            })
            .with_children(|top| {
                spawn_squads(top, fonts);
                top.spawn((
                    BattleTitle,
                    crate::style::heading(fonts, screen.title(), 20.0, BRIGHT),
                ));
                spawn_turn_order(top);
            });
            spawn_bottom(root, assets, &heroes, screen, fonts);
        });

    // Полоса ульты через весь экран (видна только во время ульты и залпа).
    commands
        .spawn((
            ScreenRoot,
            UltBand,
            Node {
                position_type: PositionType::Absolute,
                width: percent(100),
                top: percent(40),
                padding: UiRect::axes(px(12), px(10)),
                justify_content: JustifyContent::Center,
                ..default()
            },
            BackgroundColor(Color::NONE),
            UiTransform::default(),
            Pickable::IGNORE,
        ))
        .with_child((
            UltBandText,
            Text::new(""),
            fonts.display(36.0),
            TextColor(Color::NONE),
            TextLayout::justify(Justify::Center),
            shadow(),
        ));

    // Итог боя поверх всего экрана.
    commands
        .spawn((
            ScreenRoot,
            ResultOverlay,
            Node {
                position_type: PositionType::Absolute,
                width: percent(100),
                height: percent(100),
                flex_direction: FlexDirection::Column,
                // Панель — наверху, где стояли враги: отряд в позах победы виден под ней.
                justify_content: JustifyContent::FlexStart,
                padding: UiRect::top(px(RESULT_TOP)),
                align_items: AlignItems::Center,
                row_gap: px(12),
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.25)),
            Visibility::Hidden,
            GlobalZIndex(10),
        ))
        .with_children(|overlay| {
            overlay
                .spawn((
                    Node {
                        width: px(470),
                        padding: UiRect::axes(px(22), px(24)),
                        flex_direction: FlexDirection::Column,
                        align_items: AlignItems::Center,
                        row_gap: px(10),
                        ..default()
                    },
                    Skin(Look::Tile),
                ))
                .with_children(|o| {
                    o.spawn((
                        ResultText,
                        Text::new(""),
                        fonts.display(50.0),
                        TextColor(ACCENT),
                        shadow(),
                    ));
                    o.spawn((
                        ResultNote,
                        Text::new(""),
                        fonts.text(false, 16.0),
                        TextColor(BRIGHT),
                        TextLayout::justify(Justify::Center),
                    ));
                    o.spawn(Node {
                        margin: UiRect::top(px(12)),
                        column_gap: px(14),
                        ..default()
                    })
                    .with_children(|row| {
                        let button = |look: Look| {
                            (
                                Node {
                                    width: px(190),
                                    height: px(58),
                                    justify_content: JustifyContent::Center,
                                    align_items: AlignItems::Center,
                                    ..default()
                                },
                                Skin(look),
                            )
                        };
                        row.spawn(button(Look::Dark))
                            .with_child(crate::style::heading(fonts, "Ещё раз", 21.0, BRIGHT))
                            .observe(
                                |_: On<PointerClick>,
                                 mut screen: ResMut<BattleScreen>,
                                 gate: (
                                    Res<crate::supplies::SupplyData>,
                                    Res<crate::menu::Profile>,
                                    Res<crate::menu::Progress>,
                                ),
                                 mut notes: Query<&mut Text, With<ResultNote>>| {
                                    if screen.shown_outcome.is_none() {
                                        return;
                                    }
                                    // Этап главы: повтор — за припасы и в пределах дневного лимита.
                                    if screen.custom.is_none() {
                                        let (supply, profile, progress) = gate;
                                        let today = crate::cabins::today();
                                        let wins = progress.wins_today(screen.stage, today);
                                        if let Err(why) = supply.check(&profile, wins) {
                                            for mut text in &mut notes {
                                                text.0 = why.clone();
                                            }
                                            return;
                                        }
                                    }
                                    screen.restart_with_new_seed();
                                },
                            );
                        row.spawn(button(Look::Go))
                            .with_child(crate::style::heading(
                                fonts,
                                match back {
                                    Screen::Map => "К карте",
                                    Screen::Activities => "Назад",
                                    _ => "На корабль",
                                },
                                21.0,
                                Color::WHITE,
                            ))
                            .observe(
                                move |_: On<PointerClick>, mut next: ResMut<NextState<Screen>>| {
                                    next.set(back);
                                },
                            );
                    });
                });
        });
}

/// Здоровье отрядов по бокам от значка с номером хода, как в Zero City.
fn spawn_squads(top: &mut ChildSpawnerCommands, fonts: &Fonts) {
    top.spawn(Node {
        width: percent(100),
        flex_direction: FlexDirection::Row,
        align_items: AlignItems::Center,
        column_gap: px(6),
        ..default()
    })
    .with_children(|row| {
        let bar = |row: &mut ChildSpawnerCommands, side: Side, color: Color| {
            row.spawn((
                Node {
                    flex_grow: 1.0,
                    flex_basis: px(0),
                    height: px(34),
                    border: UiRect::all(px(2)),
                    border_radius: BorderRadius::all(px(11)),
                    overflow: Overflow::clip(),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    ..default()
                },
                BackgroundColor(WOOD_DARK),
                BorderColor::all(BRASS_EDGE),
            ))
            .with_children(|b| {
                b.spawn((
                    SquadFill(side),
                    Node {
                        position_type: PositionType::Absolute,
                        // Враги убывают к центру, экипаж — тоже к центру: полосы «смотрят» друг на друга.
                        left: if side == Side::Ally { px(0) } else { auto() },
                        right: if side == Side::Enemy { px(0) } else { auto() },
                        width: percent(100),
                        height: percent(100),
                        ..default()
                    },
                    BackgroundColor(color),
                ));
                b.spawn((
                    SquadText(side),
                    Text::new(""),
                    fonts.display(19.0),
                    TextColor(Color::WHITE),
                    shadow(),
                ));
            });
        };
        bar(row, Side::Ally, HP_ALLY);
        row.spawn((
            Node {
                width: px(64),
                height: px(64),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                border: UiRect::all(px(3)),
                border_radius: BorderRadius::all(px(32)),
                ..default()
            },
            BackgroundColor(WOOD_DARK),
            BorderColor::all(BRASS_EDGE),
        ))
        .with_child((
            TurnBadge,
            Text::new(""),
            fonts.display(28.0),
            TextColor(BRIGHT),
        ));
        bar(row, Side::Enemy, HP_ENEMY);
    });
}

/// Очередь ходов: кружки с первой буквой, рамка — цвет стороны, первый — крупнее.
fn spawn_turn_order(top: &mut ChildSpawnerCommands) {
    top.spawn(Node {
        flex_direction: FlexDirection::Row,
        align_items: AlignItems::Center,
        column_gap: px(5),
        ..default()
    })
    .with_children(|row| {
        for i in 0..TURN_CHIPS {
            let size = if i == 0 { CHIP * 1.25 } else { CHIP };
            row.spawn((
                TurnChip(i),
                Node {
                    width: px(size),
                    height: px(size),
                    ..default()
                },
                ImageNode {
                    color: Color::NONE,
                    ..default()
                },
            ));
        }
    });
}

fn spawn_bottom(
    root: &mut ChildSpawnerCommands,
    assets: &AssetServer,
    heroes: &[UnitId],
    screen: &BattleScreen,
    fonts: &Fonts,
) {
    root.spawn(Node {
        flex_direction: FlexDirection::Column,
        row_gap: px(6),
        ..default()
    })
    .with_children(|bottom| {
        // Залп корабля.
        bottom
            .spawn(Node {
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                column_gap: px(5),
                ..default()
            })
            .with_children(|row| {
                row.spawn(Node {
                    flex_grow: 1.0,
                    ..default()
                });
                row.spawn((
                    BroadsideLabel,
                    Text::new(""),
                    fonts.display(19.0),
                    TextColor(BROADSIDE),
                    shadow(),
                ));
                row.spawn((
                    Node {
                        width: px(170),
                        height: px(18),
                        border: UiRect::all(px(2)),
                        border_radius: BorderRadius::all(px(9)),
                        overflow: Overflow::clip(),
                        ..default()
                    },
                    BackgroundColor(WOOD_DARK),
                    BorderColor::all(BRASS_EDGE),
                ))
                .with_child((
                    BroadsideFill,
                    Node {
                        width: percent(0),
                        height: percent(100),
                        ..default()
                    },
                    BackgroundColor(BROADSIDE),
                ));
            });

        // Портреты ульт и кнопки сбоку.
        bottom
            .spawn(Node {
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::FlexEnd,
                column_gap: px(8),
                ..default()
            })
            .with_children(|row| {
                for &id in heroes {
                    spawn_ult_button(row, assets, id, screen, fonts);
                }
                row.spawn(Node {
                    flex_grow: 1.0,
                    ..default()
                });
                row.spawn(Node {
                    flex_direction: FlexDirection::Column,
                    row_gap: px(6),
                    ..default()
                })
                .with_children(|col| {
                    for kind in [ButtonKind::Pause, ButtonKind::Speed, ButtonKind::Auto] {
                        col.spawn((
                            kind,
                            Node {
                                width: px(SMALL_BUTTON),
                                height: px(SMALL_BUTTON * 0.7),
                                justify_content: JustifyContent::Center,
                                align_items: AlignItems::Center,
                                ..default()
                            },
                            Skin(Look::Dark),
                        ))
                        .with_child((Text::new(""), fonts.display(20.0), TextColor(BRIGHT)))
                        .observe(
                            move |_: On<PointerClick>, mut screen: ResMut<BattleScreen>| {
                                press(&mut screen, kind);
                            },
                        );
                    }
                });
            });

        #[cfg(feature = "dev")]
        bottom.spawn((
            DevText,
            Text::new(""),
            fonts.text(false, 11.0),
            TextColor(Color::srgb(0.10, 0.18, 0.32)),
        ));
    });
}

fn spawn_ult_button(
    row: &mut ChildSpawnerCommands,
    assets: &AssetServer,
    id: UnitId,
    screen: &BattleScreen,
    fonts: &Fonts,
) {
    let unit = screen.battle.unit(id);
    let portrait = format!("ui/hero_{}.png", unit.def.id);
    row.spawn((
        UltButton(id),
        Node {
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::Center,
            row_gap: px(2),
            ..default()
        },
        UiTransform::default(),
    ))
    .with_children(|b| {
        // Медальон героя; энергия — полоской под ним (Bevy режет потомков по
        // прямоугольнику, а не по кругу, поэтому заливка внутри круга торчала бы углами).
        // Готовая ульта — золотое кольцо вокруг медальона.
        b.spawn((
            Node {
                width: px(ULT_SIZE),
                height: px(ULT_SIZE),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                border: UiRect::all(px(3)),
                border_radius: BorderRadius::all(px(ULT_SIZE / 2.0)),
                ..default()
            },
            ImageNode::new(assets.load(portrait)),
            BorderColor::all(Color::NONE),
        ))
        .with_children(|p| {
            // Через сколько ходов спецатака; «!» — готова.
            p.spawn((
                SkillBadge(id),
                Node {
                    position_type: PositionType::Absolute,
                    top: px(-10),
                    right: px(-10),
                    width: px(SKILL_BADGE),
                    height: px(SKILL_BADGE),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    border: UiRect::all(px(2)),
                    border_radius: BorderRadius::all(px(SKILL_BADGE / 2.0)),
                    ..default()
                },
                BackgroundColor(PANEL),
                BorderColor::all(SKILL),
            ))
            .with_child((Text::new(""), fonts.display(18.0), TextColor(Color::WHITE)));
        });
        b.spawn((
            Node {
                width: px(ULT_SIZE * 0.9),
                height: px(13),
                border: UiRect::all(px(1)),
                border_radius: BorderRadius::all(px(5)),
                overflow: Overflow::clip(),
                ..default()
            },
            BackgroundColor(WOOD_DARK),
            BorderColor::all(BRASS_EDGE),
        ))
        .with_child((
            UltFill(id),
            Node {
                width: percent(0),
                height: percent(100),
                ..default()
            },
            BackgroundColor(ENERGY),
        ));
        b.spawn((
            UltLabel(id),
            Text::new(""),
            fonts.display(18.0),
            TextColor(Color::WHITE),
            shadow(),
        ));
    })
    .observe(
        move |_: On<PointerClick>, mut screen: ResMut<BattleScreen>| {
            screen.try_ultimate(id);
        },
    );
}

pub fn press(screen: &mut BattleScreen, kind: ButtonKind) {
    match kind {
        ButtonKind::Auto => screen.auto = !screen.auto,
        ButtonKind::Speed => screen.speed = screen.speed % 3 + 1,
        ButtonKind::Pause => screen.paused = !screen.paused,
    }
}

fn button_label(screen: &BattleScreen, kind: ButtonKind) -> (String, bool) {
    match kind {
        ButtonKind::Auto => ("АВТО".into(), screen.auto),
        ButtonKind::Speed => (format!("x{}", screen.speed), screen.speed > 1),
        ButtonKind::Pause => (if screen.paused { ">" } else { "II" }.into(), screen.paused),
    }
}

/// Здоровье отрядов, номер хода, очки умений и залп.
#[allow(clippy::type_complexity)] // обычные для Bevy фильтры запросов
pub fn refresh_status(
    screen: Res<BattleScreen>,
    mut squad_fill: Query<(&SquadFill, &mut Node), Without<BroadsideFill>>,
    mut broadside: Query<&mut Node, With<BroadsideFill>>,
    mut texts: ParamSet<(
        Query<(&SquadText, &mut Text)>,
        Query<&mut Text, With<TurnBadge>>,
        Query<&mut Text, With<BroadsideLabel>>,
    )>,
) {
    let battle = &screen.battle;
    let total = |side: Side| {
        battle
            .units()
            .iter()
            .filter(|u| u.side == side)
            .fold((0u32, 0u32), |(hp, max), u| {
                (hp + screen.shown_hp[u.id.0], max + u.def.stats.max_hp)
            })
    };
    for (SquadFill(side), mut node) in &mut squad_fill {
        let (hp, max) = total(*side);
        node.width = percent(100.0 * hp as f32 / max.max(1) as f32);
    }
    for (SquadText(side), mut text) in &mut texts.p0() {
        let (hp, max) = total(*side);
        text.0 = format!("{hp}/{max}");
    }
    for mut text in &mut texts.p1() {
        text.0 = battle.turn().to_string();
    }
    for mut text in &mut texts.p2() {
        text.0 = format!("Залп {}%", battle.broadside() * 100 / BROADSIDE_MAX);
    }
    for mut node in &mut broadside {
        node.width = percent(100.0 * battle.broadside() as f32 / BROADSIDE_MAX as f32);
    }
}

/// Портреты ульт: заливка энергии, пульс и золотая рамка готовой ульты, блёклый — выбыл.
#[allow(clippy::type_complexity)]
pub fn refresh_ult_buttons(
    time: Res<Time>,
    screen: Res<BattleScreen>,
    mut buttons: Query<(&UltButton, &Children, &mut UiTransform)>,
    mut frames: Query<(&mut BorderColor, &mut ImageNode), Without<UltFill>>,
    mut fills: Query<(&UltFill, &mut Node, &mut BackgroundColor)>,
    mut labels: Query<(&UltLabel, &mut Text, &mut TextColor)>,
) {
    let battle = &screen.battle;
    let pulse = 0.5 + 0.5 * (time.elapsed_secs() * 7.0).sin();
    for (UltButton(id), children, mut transform) in &mut buttons {
        let unit = battle.unit(*id);
        let alive = screen.shown_hp[id.0] > 0;
        let ready = alive && unit.ultimate_ready() && screen.shown_outcome.is_none();
        transform.scale = Vec2::splat(if ready {
            1.0 + READY_PULSE * pulse
        } else {
            1.0
        });
        if let Some((mut border, mut image)) =
            children.first().and_then(|&c| frames.get_mut(c).ok())
        {
            *border = BorderColor::all(if ready { ACCENT } else { Color::NONE });
            image.color = if alive {
                Color::WHITE
            } else {
                Color::srgba(0.45, 0.45, 0.5, 0.6)
            };
        }
    }
    for (UltFill(id), mut node, mut bg) in &mut fills {
        let unit = battle.unit(*id);
        node.width = percent(100.0 * unit.energy as f32 / ENERGY_MAX as f32);
        bg.0 = if unit.ultimate_ready() {
            ACCENT.with_alpha(0.55)
        } else {
            ENERGY
        };
    }
    for (UltLabel(id), mut text, mut color) in &mut labels {
        let unit = battle.unit(*id);
        (text.0, color.0) = if screen.shown_hp[id.0] == 0 {
            ("выбыл".into(), Color::srgb(0.75, 0.75, 0.8))
        } else if unit.ultimate_ready() {
            ("УЛЬТА!".into(), ACCENT)
        } else {
            (unit.def.name.clone(), Color::WHITE)
        };
    }
}

/// Кнопки сбоку и кружки очереди ходов.
pub fn refresh_controls(
    screen: Res<BattleScreen>,
    assets: Res<AssetServer>,
    mut buttons: Query<(&ButtonKind, &Children, &mut Skin)>,
    mut chips: Query<(&TurnChip, &mut ImageNode)>,
    mut texts: Query<&mut Text>,
) {
    let battle = &screen.battle;
    for (kind, children, mut skin) in &mut buttons {
        let (label, on) = button_label(&screen, *kind);
        let look = if on { Look::Go } else { Look::Dark };
        if skin.0 != look {
            skin.0 = look;
        }
        if let Some(mut text) = children.first().and_then(|&c| texts.get_mut(c).ok()) {
            text.0 = label;
        }
    }
    let order = if screen.shown_outcome.is_some() {
        Vec::new()
    } else {
        battle.turn_order_preview(TURN_CHIPS)
    };
    // Очередь ходов — медальоны: у героев кольцо редкости, у врагов — красное.
    for (TurnChip(i), mut image) in &mut chips {
        let Some(unit) = order.get(*i).map(|&id| battle.unit(id)) else {
            image.color = Color::NONE;
            continue;
        };
        let path = match unit.side {
            Side::Ally => format!("ui/hero_{}.png", unit.def.id),
            Side::Enemy => format!("ui/unit_{}.png", unit.def.id),
        };
        let handle = assets.load(path);
        if image.image != handle {
            image.image = handle;
        }
        if image.color != Color::WHITE {
            image.color = Color::WHITE;
        }
    }
}

/// Итог боя поверх экрана.
pub fn refresh_result(
    screen: Res<BattleScreen>,
    mut overlay: Query<&mut Visibility, With<ResultOverlay>>,
    mut result: Query<(&mut Text, &mut TextColor), With<ResultText>>,
) {
    let finished = screen.shown_outcome;
    for mut vis in &mut overlay {
        *vis = if finished.is_some() {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }
    for (mut text, mut color) in &mut result {
        match finished {
            Some(Outcome::Victory) => {
                text.0 = "ПОБЕДА!".into();
                color.0 = ACCENT;
            }
            Some(Outcome::Defeat) => {
                text.0 = "ПОРАЖЕНИЕ".into();
                color.0 = Color::srgb(1.0, 0.45, 0.40);
            }
            None => {}
        }
    }
}

/// Значки спецатак на портретах: сколько ходов осталось, «!» — готова.
pub fn refresh_skill_badges(
    screen: Res<BattleScreen>,
    mut badges: Query<(
        &SkillBadge,
        &Children,
        &mut BackgroundColor,
        &mut Visibility,
    )>,
    mut texts: Query<&mut Text>,
) {
    for (SkillBadge(id), children, mut bg, mut vis) in &mut badges {
        let unit = screen.battle.unit(*id);
        let alive = screen.shown_hp[id.0] > 0;
        *vis = if alive && unit.def.skill.is_some() {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        bg.0 = if unit.skill_ready() { SKILL } else { PANEL };
        if let Some(mut text) = children.first().and_then(|&c| texts.get_mut(c).ok()) {
            text.0 = if unit.skill_ready() {
                "!".into()
            } else {
                unit.skill_cooldown.to_string()
            };
        }
    }
}
