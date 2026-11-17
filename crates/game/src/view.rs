//! Экран боя «Живая палуба»: враги сверху, экипаж на палубе снизу, кнопки внизу.
//! Пока всё на заглушках: карточки с первой буквой имени вместо портретов.

use battle_core::{BROADSIDE_MAX, ENERGY_MAX, Element, Outcome, SKILL_POINTS_MAX, Side, UnitId};
use bevy::prelude::*;

use crate::battle_screen::BattleScreen;
use crate::fx::CardMotion;
use crate::text;

const SKY: Color = Color::srgb(0.53, 0.75, 0.92);
const PANEL: Color = Color::srgba(0.05, 0.10, 0.20, 0.78);
const ALLY_CARD: Color = Color::srgb(0.13, 0.22, 0.34);
const ENEMY_CARD: Color = Color::srgb(0.32, 0.15, 0.19);
const DECK: Color = Color::srgb(0.45, 0.30, 0.18);
const HP_ALLY: Color = Color::srgb(0.38, 0.85, 0.45);
const HP_ENEMY: Color = Color::srgb(0.92, 0.32, 0.30);
const BAR_TRACK: Color = Color::srgba(0.0, 0.0, 0.0, 0.45);
const ENERGY: Color = Color::srgb(0.35, 0.65, 1.0);
const ACCENT: Color = Color::srgb(1.0, 0.85, 0.35);
const MUTED: Color = Color::srgb(0.70, 0.76, 0.84);
const BUTTON: Color = Color::srgb(0.16, 0.27, 0.42);
const BUTTON_ON: Color = Color::srgb(0.85, 0.62, 0.20);

const ALLY_CARD_WIDTH: f32 = 116.0;
const ENEMY_CARD_WIDTH: f32 = 104.0;
const TURN_CHIPS: usize = 7;

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

/// Карточка бойца. На ней же живут сдвиг и тряска.
#[derive(Component, Clone, Copy)]
pub struct UnitCard(pub UnitId);

#[derive(Component, Clone, Copy)]
pub struct CardBody(pub UnitId);

#[derive(Component, Clone, Copy)]
pub struct HpFill(pub UnitId);

#[derive(Component, Clone, Copy)]
pub struct HpText(pub UnitId);

#[derive(Component, Clone, Copy)]
pub struct EnergyFill(pub UnitId);

#[derive(Component, Clone, Copy)]
pub struct EnergyText(pub UnitId);

#[derive(Component, Clone, Copy)]
pub struct TurnChip(pub usize);

#[derive(Component)]
pub struct Banner;

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

/// Строка тестовой сцены (только со фичей dev).
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

/// Собрать экран заново, когда начался бой с другим составом.
pub fn rebuild(
    mut commands: Commands,
    mut screen: ResMut<BattleScreen>,
    fonts: Res<Fonts>,
    roots: Query<Entity, With<ScreenRoot>>,
) {
    if !screen.rebuild {
        return;
    }
    screen.rebuild = false;
    for root in &roots {
        commands.entity(root).despawn();
    }
    spawn_screen(&mut commands, &screen, &fonts);
}

fn spawn_screen(commands: &mut Commands, screen: &BattleScreen, fonts: &Fonts) {
    let battle = &screen.battle;
    let ids = |side: Side| -> Vec<UnitId> {
        battle
            .units()
            .iter()
            .filter(|u| u.side == side)
            .map(|u| u.id)
            .collect()
    };
    let (allies, enemies) = (ids(Side::Ally), ids(Side::Enemy));

    commands
        .spawn((
            ScreenRoot,
            Node {
                width: percent(100),
                height: percent(100),
                flex_direction: FlexDirection::Column,
                padding: UiRect::all(px(12)),
                row_gap: px(10),
                ..default()
            },
        ))
        .with_children(|root| {
            // Очередь ходов.
            root.spawn((
                Node {
                    flex_direction: FlexDirection::Row,
                    column_gap: px(4),
                    padding: UiRect::all(px(6)),
                    border_radius: BorderRadius::all(px(8)),
                    ..default()
                },
                BackgroundColor(PANEL),
            ))
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
                        fonts.text(i == 0, if i == 0 { 15.0 } else { 13.0 }),
                        TextColor(Color::WHITE),
                    ));
                }
            });

            root.spawn((
                Text::new(format!(
                    "Этап {} · {}",
                    screen.stage_id(),
                    screen.stage_name()
                )),
                fonts.text(true, 20.0),
                TextColor(Color::WHITE),
            ));

            spawn_row(root, &enemies, screen, fonts, ENEMY_CARD_WIDTH);

            // Небо между кораблями: здесь всплывают названия умений.
            root.spawn(Node {
                flex_grow: 1.0,
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            })
            .with_child((
                Banner,
                Text::new(""),
                fonts.text(true, 26.0),
                TextColor(ACCENT.with_alpha(0.0)),
                TextLayout::justify(Justify::Center),
            ));

            spawn_row(root, &allies, screen, fonts, ALLY_CARD_WIDTH);

            // Палуба «Ласточки».
            root.spawn((
                Node {
                    height: px(12),
                    border_radius: BorderRadius::all(px(4)),
                    ..default()
                },
                BackgroundColor(DECK),
            ));

            spawn_bottom_panel(root, fonts);

            #[cfg(feature = "dev")]
            root.spawn((
                DevText,
                Text::new(""),
                fonts.text(false, 12.0),
                TextColor(Color::srgb(0.08, 0.16, 0.30)),
            ));
        });

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

fn spawn_row(
    root: &mut ChildSpawnerCommands,
    ids: &[UnitId],
    screen: &BattleScreen,
    fonts: &Fonts,
    width: f32,
) {
    root.spawn(Node {
        flex_direction: FlexDirection::Row,
        justify_content: JustifyContent::Center,
        column_gap: px(8),
        ..default()
    })
    .with_children(|row| {
        for &id in ids {
            spawn_card(row, id, screen, fonts, width);
        }
    });
}

fn spawn_card(
    row: &mut ChildSpawnerCommands,
    id: UnitId,
    screen: &BattleScreen,
    fonts: &Fonts,
    width: f32,
) {
    let unit = screen.battle.unit(id);
    let ally = unit.side == Side::Ally;
    let element = element_color(unit.def.element);
    let initial: String = unit.def.name.chars().take(1).collect();

    let mut card = row.spawn((
        UnitCard(id),
        CardBody(id),
        Node {
            width: px(width),
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::Center,
            padding: UiRect::all(px(6)),
            row_gap: px(4),
            border: UiRect::all(px(3)),
            border_radius: BorderRadius::all(px(10)),
            ..default()
        },
        BackgroundColor(if ally { ALLY_CARD } else { ENEMY_CARD }),
        BorderColor::all(Color::NONE),
        UiTransform::default(),
        CardMotion::default(),
    ));
    card.with_children(|c| {
        // Заглушка портрета: круг цвета стихии с первой буквой имени.
        c.spawn((
            Node {
                width: px(46),
                height: px(46),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                border_radius: BorderRadius::all(px(23)),
                ..default()
            },
            BackgroundColor(element),
        ))
        .with_child((
            Text::new(initial),
            fonts.text(true, 24.0),
            TextColor(Color::srgb(0.08, 0.10, 0.16)),
        ));
        c.spawn((
            Text::new(unit.def.name.clone()),
            fonts.text(true, 13.0),
            TextColor(Color::WHITE),
            TextLayout::justify(Justify::Center),
        ));
        c.spawn((
            Text::new(text::element_name(unit.def.element)),
            fonts.text(false, 11.0),
            TextColor(element),
        ));
        spawn_bar(
            c,
            BAR_TRACK,
            if ally { HP_ALLY } else { HP_ENEMY },
            HpFill(id),
            8.0,
        );
        c.spawn((
            HpText(id),
            Text::new(""),
            fonts.text(false, 11.0),
            TextColor(MUTED),
        ));
        if unit.def.ultimate.is_some() {
            spawn_bar(c, BAR_TRACK, ENERGY, EnergyFill(id), 6.0);
            c.spawn((
                EnergyText(id),
                Text::new(""),
                fonts.text(true, 11.0),
                TextColor(ENERGY),
            ));
        }
    });
    if ally {
        card.observe(
            move |_: On<PointerClick>, mut screen: ResMut<BattleScreen>| {
                screen.try_ultimate(id);
            },
        );
    }
}

fn spawn_bar(
    parent: &mut ChildSpawnerCommands,
    track: Color,
    fill: Color,
    marker: impl Component,
    height: f32,
) {
    parent
        .spawn((
            Node {
                width: percent(100),
                height: px(height),
                border_radius: BorderRadius::all(px(height / 2.0)),
                overflow: Overflow::clip(),
                ..default()
            },
            BackgroundColor(track),
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

fn spawn_bottom_panel(root: &mut ChildSpawnerCommands, fonts: &Fonts) {
    root.spawn((
        Node {
            flex_direction: FlexDirection::Column,
            padding: UiRect::all(px(10)),
            row_gap: px(8),
            border_radius: BorderRadius::all(px(10)),
            ..default()
        },
        BackgroundColor(PANEL),
    ))
    .with_children(|panel| {
        panel.spawn((
            StatusText,
            Text::new(""),
            fonts.text(false, 14.0),
            TextColor(Color::WHITE),
        ));
        spawn_bar(panel, BAR_TRACK, ACCENT, BroadsideFill, 10.0);
        panel
            .spawn(Node {
                flex_direction: FlexDirection::Row,
                column_gap: px(8),
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
                                padding: UiRect::axes(px(6), px(10)),
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
    });
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

/// Полоски здоровья, энергии и залпа.
#[allow(clippy::type_complexity)] // обычные для Bevy фильтры запросов
pub fn refresh_bars(
    screen: Res<BattleScreen>,
    mut hp: Query<(&HpFill, &mut Node), (Without<EnergyFill>, Without<BroadsideFill>)>,
    mut energy: Query<(&EnergyFill, &mut Node, &mut BackgroundColor), Without<BroadsideFill>>,
    mut broadside: Query<&mut Node, With<BroadsideFill>>,
) {
    let battle = &screen.battle;
    for (HpFill(id), mut node) in &mut hp {
        let max = battle.unit(*id).def.stats.max_hp.max(1);
        node.width = percent(100.0 * screen.shown_hp[id.0] as f32 / max as f32);
    }
    for (EnergyFill(id), mut node, mut bg) in &mut energy {
        let unit = battle.unit(*id);
        node.width = percent(100.0 * unit.energy as f32 / ENERGY_MAX as f32);
        bg.0 = if unit.ultimate_ready() {
            ACCENT
        } else {
            ENERGY
        };
    }
    for mut node in &mut broadside {
        node.width = percent(100.0 * battle.broadside() as f32 / BROADSIDE_MAX as f32);
    }
}

/// Подписи на карточках и строка состояния.
#[allow(clippy::type_complexity)] // обычные для Bevy фильтры запросов
pub fn refresh_labels(
    screen: Res<BattleScreen>,
    mut hp: Query<(&HpText, &mut Text), (Without<EnergyText>, Without<StatusText>)>,
    mut energy: Query<(&EnergyText, &mut Text), Without<StatusText>>,
    mut status: Query<&mut Text, With<StatusText>>,
) {
    let battle = &screen.battle;
    for (HpText(id), mut text) in &mut hp {
        let unit = battle.unit(*id);
        text.0 = format!("{}/{}", screen.shown_hp[id.0], unit.def.stats.max_hp);
    }
    for (EnergyText(id), mut text) in &mut energy {
        let unit = battle.unit(*id);
        text.0 = if unit.ultimate_ready() {
            "УЛЬТА — ТАП".into()
        } else if screen.shown_hp[id.0] > 0 {
            format!("энергия {}%", unit.energy * 100 / ENERGY_MAX)
        } else {
            String::new()
        };
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

/// Кнопки внизу и фишки очереди ходов.
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
            Some(Side::Ally) => ALLY_CARD,
            Some(Side::Enemy) => ENEMY_CARD,
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

/// Рамка карточки: чей ход, готовая ульта; блёклая карточка — боец повержен.
pub fn refresh_cards(
    screen: Res<BattleScreen>,
    mut cards: Query<(&CardBody, &mut BorderColor, &mut BackgroundColor)>,
) {
    let battle = &screen.battle;
    for (CardBody(id), mut border, mut bg) in &mut cards {
        let unit = battle.unit(*id);
        let base = if unit.side == Side::Ally {
            ALLY_CARD
        } else {
            ENEMY_CARD
        };
        let alive = screen.shown_hp[id.0] > 0;
        *border = BorderColor::all(if !alive {
            Color::NONE
        } else if screen.shown_current == Some(*id) {
            Color::WHITE
        } else if unit.ultimate_ready() {
            ACCENT
        } else {
            Color::NONE
        });
        bg.0 = if alive { base } else { base.with_alpha(0.35) };
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
