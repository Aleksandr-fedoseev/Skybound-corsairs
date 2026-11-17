//! Интерфейс поверх поля боя: сверху — здоровье отрядов и очередь ходов, снизу — кнопки ульт
//! героев, залп корабля и управление боем. Само поле с моделями — в `battlefield.rs`.

use battle_core::{BROADSIDE_MAX, ENERGY_MAX, Element, Outcome, SKILL_POINTS_MAX, Side, UnitId};
use bevy::prelude::*;

use crate::battle_screen::BattleScreen;
use crate::battlefield;

const SKY: Color = Color::srgb(0.53, 0.75, 0.92);
const PANEL: Color = Color::srgba(0.05, 0.10, 0.20, 0.80);
const ALLY: Color = Color::srgb(0.13, 0.22, 0.34);
const ENEMY: Color = Color::srgb(0.32, 0.15, 0.19);
const HP_ALLY: Color = Color::srgb(0.38, 0.85, 0.45);
const HP_ENEMY: Color = Color::srgb(0.92, 0.32, 0.30);
const BAR_TRACK: Color = Color::srgba(0.0, 0.0, 0.0, 0.45);
const ENERGY: Color = Color::srgb(0.35, 0.65, 1.0);
const ACCENT: Color = Color::srgb(1.0, 0.85, 0.35);
const BUTTON: Color = Color::srgb(0.16, 0.27, 0.42);
const BUTTON_ON: Color = Color::srgb(0.85, 0.62, 0.20);
const TURN_CHIPS: usize = 7;
/// Баннер действия: доля ширины экрана слева и отступ сверху, %.
const BANNER_WIDTH: f32 = 46.0;
const BANNER_TOP: f32 = 27.0;
const ULT_PORTRAIT: f32 = 46.0;
/// Готовая ульта: кнопка пульсирует.
const READY_PULSE: f32 = 0.05;

pub fn element_color(element: Element) -> Color {
    match element {
        Element::Wind => Color::srgb(0.45, 0.85, 0.62),
        Element::Lightning => Color::srgb(0.70, 0.55, 1.0),
        Element::Powder => Color::srgb(1.0, 0.60, 0.28),
        Element::Steel => Color::srgb(0.62, 0.68, 0.75),
        Element::Mist => Color::srgb(0.80, 0.88, 0.98),
    }
}

#[derive(Resource)]
pub struct Fonts {
    pub regular: Handle<Font>,
    pub bold: Handle<Font>,
}

impl Fonts {
    pub fn text(&self, bold: bool, size: f32) -> TextFont {
        TextFont {
            font: if bold { &self.bold } else { &self.regular }.clone().into(),
            font_size: FontSize::Px(size),
            ..default()
        }
    }
}

#[derive(Component)]
pub struct ScreenRoot;

#[derive(Component, Clone, Copy)]
pub struct SquadFill(pub Side);

#[derive(Component, Clone, Copy)]
pub struct SquadText(pub Side);

#[derive(Component, Clone, Copy)]
pub struct TurnChip(pub usize);

#[derive(Component)]
pub struct Banner;

/// Кнопка ульты героя: тап — ульта, даже вне его хода.
#[derive(Component, Clone, Copy)]
pub struct UltButton(pub UnitId);

#[derive(Component, Clone, Copy)]
pub struct UltFill(pub UnitId);

#[derive(Component, Clone, Copy)]
pub struct UltLabel(pub UnitId);

#[derive(Component)]
pub struct StatusText;

#[derive(Component)]
pub struct BroadsideFill;

#[derive(Component, Clone, Copy, PartialEq, Eq)]
pub enum ButtonKind {
    Auto,
    Speed,
    Pause,
    Restart,
}

#[derive(Component)]
pub struct ResultOverlay;

#[derive(Component)]
pub struct ResultText;

/// Строка тестовой сцены (только с фичей dev).
#[cfg(feature = "dev")]
#[derive(Component)]
pub struct DevText;

pub fn setup(mut commands: Commands, assets: Res<AssetServer>) {
    commands.insert_resource(ClearColor(SKY));
    commands.spawn(Camera2d);
    commands.insert_resource(Fonts {
        regular: assets.load("fonts/NotoSans-Regular.ttf"),
        bold: assets.load("fonts/NotoSans-Bold.ttf"),
    });
}

/// Собрать экран заново: новый бой с другим составом или другая раскладка поля.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub fn rebuild(
    mut commands: Commands,
    mut screen: ResMut<BattleScreen>,
    fonts: Res<Fonts>,
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
    battlefield::spawn_field(&mut commands, &mut meshes, &mut materials, &screen, &fonts);
    spawn_hud(&mut commands, &screen, &fonts);
}

fn spawn_hud(commands: &mut Commands, screen: &BattleScreen, fonts: &Fonts) {
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
                padding: UiRect::all(px(10)),
                ..default()
            },
            Pickable::IGNORE,
        ))
        .with_children(|root| {
            root.spawn((Node {
                flex_direction: FlexDirection::Column,
                row_gap: px(6),
                ..default()
            },))
                .with_children(|top| {
                    spawn_squads(top, fonts);
                    spawn_turn_order(top, fonts);
                    top.spawn((
                        Text::new(format!(
                            "Этап {} · {}",
                            screen.stage_id(),
                            screen.stage_name()
                        )),
                        fonts.text(true, 16.0),
                        TextColor(Color::WHITE),
                        TextShadow {
                            offset: Vec2::new(1.0, 1.0),
                            color: Color::srgba(0.0, 0.0, 0.0, 0.6),
                        },
                    ));
                });
            spawn_bottom(root, &heroes, screen, fonts);
        });

    // Баннер с названием действия — в пустом небе слева сверху, чтобы не закрывать модели.
    commands
        .spawn((
            ScreenRoot,
            Node {
                position_type: PositionType::Absolute,
                width: percent(BANNER_WIDTH),
                top: percent(BANNER_TOP),
                justify_content: JustifyContent::Center,
                ..default()
            },
            Pickable::IGNORE,
        ))
        .with_child((
            Banner,
            Text::new(""),
            fonts.text(true, 26.0),
            TextShadow {
                offset: Vec2::new(2.0, 2.0),
                color: Color::srgba(0.0, 0.0, 0.0, 0.7),
            },
            TextColor(ACCENT.with_alpha(0.0)),
            TextLayout::justify(Justify::Center),
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
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                row_gap: px(12),
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.55)),
            Visibility::Hidden,
            GlobalZIndex(10),
        ))
        .with_children(|o| {
            o.spawn((
                ResultText,
                Text::new(""),
                fonts.text(true, 44.0),
                TextColor(ACCENT),
            ));
            o.spawn((
                Text::new("Тап — новый бой"),
                fonts.text(false, 18.0),
                TextColor(Color::WHITE),
            ));
        })
        .observe(|_: On<PointerClick>, mut screen: ResMut<BattleScreen>| {
            if screen.shown_outcome.is_some() {
                screen.restart_with_new_seed();
            }
        });
}

/// Общее здоровье отрядов, как в Zero City: экипаж слева, враги справа.
fn spawn_squads(top: &mut ChildSpawnerCommands, fonts: &Fonts) {
    top.spawn((
        Node {
            flex_direction: FlexDirection::Row,
            column_gap: px(8),
            padding: UiRect::all(px(6)),
            border_radius: BorderRadius::all(px(8)),
            ..default()
        },
        BackgroundColor(PANEL),
    ))
    .with_children(|row| {
        for (side, color) in [(Side::Ally, HP_ALLY), (Side::Enemy, HP_ENEMY)] {
            row.spawn(Node {
                flex_grow: 1.0,
                flex_basis: px(0),
                flex_direction: FlexDirection::Column,
                row_gap: px(2),
                ..default()
            })
            .with_children(|col| {
                col.spawn((
                    SquadText(side),
                    Text::new(""),
                    fonts.text(true, 13.0),
                    TextColor(Color::WHITE),
                ));
                spawn_bar(col, color, SquadFill(side), 10.0);
            });
        }
    });
}

fn spawn_turn_order(top: &mut ChildSpawnerCommands, fonts: &Fonts) {
    top.spawn((Node {
        flex_direction: FlexDirection::Row,
        column_gap: px(4),
        ..default()
    },))
        .with_children(|row| {
            for i in 0..TURN_CHIPS {
                row.spawn((
                    TurnChip(i),
                    Node {
                        padding: UiRect::axes(px(6), px(3)),
                        border_radius: BorderRadius::all(px(6)),
                        ..default()
                    },
                    BackgroundColor(BUTTON),
                ))
                .with_child((
                    Text::new(""),
                    fonts.text(i == 0, if i == 0 { 16.0 } else { 13.0 }),
                    TextColor(Color::WHITE),
                ));
            }
        });
}

fn spawn_bottom(
    root: &mut ChildSpawnerCommands,
    heroes: &[UnitId],
    screen: &BattleScreen,
    fonts: &Fonts,
) {
    root.spawn((
        Node {
            flex_direction: FlexDirection::Column,
            padding: UiRect::all(px(8)),
            row_gap: px(7),
            border_radius: BorderRadius::all(px(10)),
            ..default()
        },
        BackgroundColor(PANEL),
    ))
    .with_children(|panel| {
        // Кнопки ульт героев.
        panel
            .spawn(Node {
                flex_direction: FlexDirection::Row,
                column_gap: px(6),
                ..default()
            })
            .with_children(|row| {
                for &id in heroes {
                    spawn_ult_button(row, id, screen, fonts);
                }
            });
        panel.spawn((
            StatusText,
            Text::new(""),
            fonts.text(false, 13.0),
            TextColor(Color::WHITE),
        ));
        spawn_bar(panel, ACCENT, BroadsideFill, 8.0);
        panel
            .spawn(Node {
                flex_direction: FlexDirection::Row,
                column_gap: px(6),
                ..default()
            })
            .with_children(|buttons| {
                for kind in [
                    ButtonKind::Auto,
                    ButtonKind::Speed,
                    ButtonKind::Pause,
                    ButtonKind::Restart,
                ] {
                    buttons
                        .spawn((
                            kind,
                            Node {
                                flex_grow: 1.0,
                                padding: UiRect::axes(px(6), px(8)),
                                justify_content: JustifyContent::Center,
                                border_radius: BorderRadius::all(px(8)),
                                ..default()
                            },
                            BackgroundColor(BUTTON),
                        ))
                        .with_child((
                            Text::new(""),
                            fonts.text(true, 15.0),
                            TextColor(Color::WHITE),
                        ))
                        .observe(
                            move |_: On<PointerClick>, mut screen: ResMut<BattleScreen>| {
                                press(&mut screen, kind);
                            },
                        );
                }
            });
        #[cfg(feature = "dev")]
        panel.spawn((
            DevText,
            Text::new(""),
            fonts.text(false, 11.0),
            TextColor(Color::srgb(0.75, 0.82, 0.92)),
        ));
    });
}

fn spawn_ult_button(
    row: &mut ChildSpawnerCommands,
    id: UnitId,
    screen: &BattleScreen,
    fonts: &Fonts,
) {
    let unit = screen.battle.unit(id);
    let initial: String = unit.def.name.chars().take(1).collect();
    row.spawn((
        UltButton(id),
        Node {
            flex_grow: 1.0,
            flex_basis: px(0),
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::Center,
            padding: UiRect::all(px(5)),
            row_gap: px(3),
            border: UiRect::all(px(3)),
            border_radius: BorderRadius::all(px(10)),
            ..default()
        },
        BackgroundColor(ALLY),
        BorderColor::all(Color::NONE),
        UiTransform::default(),
    ))
    .with_children(|b| {
        b.spawn((
            Node {
                width: px(ULT_PORTRAIT),
                height: px(ULT_PORTRAIT),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                border_radius: BorderRadius::all(px(ULT_PORTRAIT / 2.0)),
                ..default()
            },
            BackgroundColor(element_color(unit.def.element)),
        ))
        .with_child((
            Text::new(initial),
            fonts.text(true, 24.0),
            TextColor(Color::srgb(0.08, 0.10, 0.16)),
        ));
        b.spawn((
            Text::new(unit.def.name.clone()),
            fonts.text(true, 13.0),
            TextColor(Color::WHITE),
        ));
        spawn_bar(b, ENERGY, UltFill(id), 6.0);
        b.spawn((
            UltLabel(id),
            Text::new(""),
            fonts.text(true, 11.0),
            TextColor(ENERGY),
        ));
    })
    .observe(
        move |_: On<PointerClick>, mut screen: ResMut<BattleScreen>| {
            screen.try_ultimate(id);
        },
    );
}

fn spawn_bar(parent: &mut ChildSpawnerCommands, fill: Color, marker: impl Component, height: f32) {
    parent
        .spawn((
            Node {
                width: percent(100),
                height: px(height),
                border_radius: BorderRadius::all(px(height / 2.0)),
                overflow: Overflow::clip(),
                ..default()
            },
            BackgroundColor(BAR_TRACK),
        ))
        .with_child((
            marker,
            Node {
                width: percent(100),
                height: percent(100),
                ..default()
            },
            BackgroundColor(fill),
        ));
}

pub fn press(screen: &mut BattleScreen, kind: ButtonKind) {
    match kind {
        ButtonKind::Auto => screen.auto = !screen.auto,
        ButtonKind::Speed => screen.speed = screen.speed % 3 + 1,
        ButtonKind::Pause => screen.paused = !screen.paused,
        ButtonKind::Restart => screen.restart_with_new_seed(),
    }
}

fn button_label(screen: &BattleScreen, kind: ButtonKind) -> (String, bool) {
    match kind {
        ButtonKind::Auto => ("АВТО".into(), screen.auto),
        ButtonKind::Speed => (format!("x{}", screen.speed), screen.speed > 1),
        ButtonKind::Pause => (
            if screen.paused {
                "Дальше"
            } else {
                "Пауза"
            }
            .into(),
            screen.paused,
        ),
        ButtonKind::Restart => ("Заново".into(), false),
    }
}

/// Здоровье отрядов, залп и строка состояния.
#[allow(clippy::type_complexity)] // обычные для Bevy фильтры запросов
pub fn refresh_status(
    screen: Res<BattleScreen>,
    mut squad_fill: Query<(&SquadFill, &mut Node), Without<BroadsideFill>>,
    mut broadside: Query<&mut Node, With<BroadsideFill>>,
    mut squad_text: Query<(&SquadText, &mut Text), Without<StatusText>>,
    mut status: Query<&mut Text, With<StatusText>>,
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
    for (SquadText(side), mut text) in &mut squad_text {
        let (hp, max) = total(*side);
        let name = if *side == Side::Ally {
            "Экипаж"
        } else {
            "Враги"
        };
        text.0 = format!("{name}  {hp}/{max}");
    }
    for mut node in &mut broadside {
        node.width = percent(100.0 * battle.broadside() as f32 / BROADSIDE_MAX as f32);
    }
    for mut text in &mut status {
        text.0 = format!(
            "Ход {}   Очки умений {}/{}   Залп корабля {}%",
            battle.turn(),
            battle.skill_points(),
            SKILL_POINTS_MAX,
            battle.broadside() * 100 / BROADSIDE_MAX,
        );
    }
}

/// Кнопки ульт: энергия, подсветка и пульс готовой ульты, блёклая кнопка павшего героя.
#[allow(clippy::type_complexity)]
pub fn refresh_ult_buttons(
    time: Res<Time>,
    screen: Res<BattleScreen>,
    mut buttons: Query<(
        &UltButton,
        &mut BorderColor,
        &mut BackgroundColor,
        &mut UiTransform,
    )>,
    mut fills: Query<(&UltFill, &mut Node, &mut BackgroundColor), Without<UltButton>>,
    mut labels: Query<(&UltLabel, &mut Text, &mut TextColor)>,
) {
    let battle = &screen.battle;
    let pulse = 0.5 + 0.5 * (time.elapsed_secs() * 7.0).sin();
    for (UltButton(id), mut border, mut bg, mut transform) in &mut buttons {
        let unit = battle.unit(*id);
        let alive = screen.shown_hp[id.0] > 0;
        let ready = alive && unit.ultimate_ready() && screen.shown_outcome.is_none();
        *border = BorderColor::all(if ready { ACCENT } else { Color::NONE });
        bg.0 = if alive { ALLY } else { ALLY.with_alpha(0.35) };
        transform.scale = Vec2::splat(if ready {
            1.0 + READY_PULSE * pulse
        } else {
            1.0
        });
    }
    for (UltFill(id), mut node, mut bg) in &mut fills {
        let unit = battle.unit(*id);
        node.width = percent(100.0 * unit.energy as f32 / ENERGY_MAX as f32);
        bg.0 = if unit.ultimate_ready() {
            ACCENT
        } else {
            ENERGY
        };
    }
    for (UltLabel(id), mut text, mut color) in &mut labels {
        let unit = battle.unit(*id);
        (text.0, color.0) = if screen.shown_hp[id.0] == 0 {
            ("выбыл".into(), Color::srgb(0.6, 0.6, 0.65))
        } else if unit.ultimate_ready() {
            ("УЛЬТА!".into(), ACCENT)
        } else {
            (format!("{}%", unit.energy * 100 / ENERGY_MAX), ENERGY)
        };
    }
}

/// Кнопки управления и фишки очереди ходов.
pub fn refresh_controls(
    screen: Res<BattleScreen>,
    mut buttons: Query<(&ButtonKind, &Children, &mut BackgroundColor), Without<TurnChip>>,
    mut chips: Query<(&TurnChip, &Children, &mut BackgroundColor)>,
    mut texts: Query<&mut Text>,
) {
    let battle = &screen.battle;
    for (kind, children, mut bg) in &mut buttons {
        let (label, on) = button_label(&screen, *kind);
        bg.0 = if on { BUTTON_ON } else { BUTTON };
        if let Some(mut text) = children.first().and_then(|&c| texts.get_mut(c).ok()) {
            text.0 = label;
        }
    }
    let order = if screen.shown_outcome.is_some() {
        Vec::new()
    } else {
        battle.turn_order_preview(TURN_CHIPS)
    };
    for (TurnChip(i), children, mut bg) in &mut chips {
        let unit = order.get(*i).map(|&id| battle.unit(id));
        bg.0 = match unit.map(|u| u.side) {
            Some(Side::Ally) => ALLY,
            Some(Side::Enemy) => ENEMY,
            None => Color::NONE,
        };
        if let Some(mut text) = children.first().and_then(|&c| texts.get_mut(c).ok()) {
            text.0 = unit.map(|u| short_name(&u.def.name)).unwrap_or_default();
        }
    }
}

/// Короткое имя для очереди ходов: «Имперский морпех» → «Морпех».
fn short_name(name: &str) -> String {
    let word = name.split_whitespace().last().unwrap_or(name);
    let mut chars = word.chars();
    chars
        .next()
        .map(|first| first.to_uppercase().chain(chars).collect())
        .unwrap_or_default()
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
