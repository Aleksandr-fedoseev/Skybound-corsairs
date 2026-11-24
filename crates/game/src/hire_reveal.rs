//! Момент найма в Таверне: карты рубашкой вверх → переворот по одной → витрина героя → итог.
//!
//! Таверна даёт копии героев команды. Рубашка светится золотом, если эта копия добирает героя до
//! звезды: перед ней карта дрожит, при открытии экран вспыхивает. Витрина — герой во весь рост в
//! позе победы на лучах цвета стихии (золотых, если звезда готова). В одиночном найме витрина есть
//! всегда, в ×10 — только для копий, которые добрали звезду. Тап по экрану — дальше, «Пропустить» — сразу к итогу.

use bevy::prelude::*;
use game_data::GameData;

use crate::Screen;
use crate::audio::Sfx;
use crate::battle_screen::BattleScreen;
use crate::battlefield::AnimatedArt;
use crate::menu::{GOLD, PARCHMENT, pressable, text};
use crate::style::{self, INK, INK_SOFT, Look, Skin};
use crate::view::Fonts;

const BRASS_EDGE: Color = Color::srgb(0.79, 0.59, 0.25);
const COPY_GLOW: Color = Color::srgb(0.70, 0.55, 1.0);
const MUTED: Color = Color::srgb(0.75, 0.70, 0.62);
/// Первая карта открывается через столько секунд, следующие — с шагом.
const FIRST_FLIP: f32 = 0.7;
const FLIP_STEP: f32 = 0.22;
/// Перед SSR карта дрожит столько секунд, после SR/SSR — пауза дольше.
const SSR_SUSPENSE: f32 = 0.7;
const RARE_AFTER: f32 = 0.45;
const FLIP_TIME: f32 = 0.3;
/// После последней карты — пауза перед витриной.
const CARDS_TAIL: f32 = 0.8;
const FLASH_TIME: f32 = 0.5;
/// Витрина: герой поднимается и проявляется.
const RISE_TIME: f32 = 0.55;
const RISE_PX: f32 = 50.0;
const FIGURE_H: f32 = 440.0;
/// Лучи: оборотов в секунду.
const RAYS_TURN: f32 = 0.03;

/// Что выпало: герой, редкость, новый ли.
#[derive(Clone)]
pub struct Pulled {
    /// Герой копии; у добычи из сундука пусто.
    pub id: String,
    /// Эта копия добрала героя до следующей звезды.
    pub star: bool,
    /// Ресурсы из сундука вместо копии.
    pub loot: Option<crate::events::Bundle>,
}

impl Pulled {
    fn copy(&self) -> bool {
        self.loot.is_none()
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Phase {
    Cards,
    /// Витрина номер n в списке `shows`.
    Show(usize),
    Summary,
}

#[derive(Resource)]
pub struct HireReveal {
    list: Vec<Pulled>,
    phase: Phase,
    t: f32,
    /// Когда открывается каждая карта.
    flips: Vec<f32>,
    /// Кого показать на витрине (индексы в `list`).
    shows: Vec<usize>,
    built: Option<Phase>,
    /// Звук и вспышка открытия уже были (по картам).
    opened: Vec<bool>,
}

impl Default for HireReveal {
    fn default() -> Self {
        Self {
            list: Vec::new(),
            phase: Phase::Cards,
            t: 0.0,
            flips: Vec::new(),
            shows: Vec::new(),
            built: None,
            opened: Vec::new(),
        }
    }
}

impl HireReveal {
    pub fn start(&mut self, list: Vec<Pulled>) {
        let mut flips = Vec::with_capacity(list.len());
        let mut t = FIRST_FLIP;
        for p in &list {
            if p.star {
                t += SSR_SUSPENSE;
            }
            flips.push(t);
            // После копии — пауза подольше: это и есть «выпало!».
            t += if p.copy() { RARE_AFTER } else { FLIP_STEP };
        }
        let shows = list
            .iter()
            .enumerate()
            .filter(|(_, p)| p.copy())
            .map(|(i, _)| i)
            .collect();
        *self = Self {
            opened: vec![false; list.len()],
            list,
            flips,
            shows,
            ..default()
        };
    }

    pub fn active(&self) -> bool {
        !self.list.is_empty()
    }

    fn cards_done_at(&self) -> f32 {
        self.flips.last().copied().unwrap_or(0.0) + FLIP_TIME + CARDS_TAIL
    }

    /// Следующий шаг: карты → витрины → итог (у одиночного найма итога нет — закрыть).
    fn advance(&mut self) {
        self.t = 0.0;
        self.phase = match self.phase {
            Phase::Cards if !self.shows.is_empty() => Phase::Show(0),
            Phase::Show(n) if n + 1 < self.shows.len() => Phase::Show(n + 1),
            Phase::Summary => {
                self.list.clear();
                Phase::Summary
            }
            _ if self.list.len() == 1 => {
                self.list.clear();
                Phase::Summary
            }
            _ => Phase::Summary,
        };
    }

    fn skip(&mut self) {
        self.t = 0.0;
        if self.list.len() == 1 {
            self.list.clear();
        }
        self.phase = Phase::Summary;
    }
}

#[derive(Component)]
struct RevealRoot;

/// Карта: номер в списке.
#[derive(Component)]
struct HireCard(usize);

#[derive(Component)]
struct CardBack;

#[derive(Component)]
struct CardFront;

#[derive(Component)]
struct Flash;

#[derive(Component)]
struct Rays;

#[derive(Component)]
struct Figure;

pub fn plugin(app: &mut App) {
    app.init_resource::<HireReveal>().add_systems(
        Update,
        (drive, build, animate_cards, animate_show)
            .chain()
            .run_if(in_state(Screen::Tavern)),
    );
}

fn drive(time: Res<Time>, mut reveal: ResMut<HireReveal>) {
    if !reveal.active() {
        return;
    }
    reveal.t += time.delta_secs();
    if reveal.phase == Phase::Cards && reveal.t >= reveal.cards_done_at() {
        reveal.advance();
    }
}

fn build(
    mut commands: Commands,
    mut reveal: ResMut<HireReveal>,
    roots: Query<Entity, With<RevealRoot>>,
    assets: Res<AssetServer>,
    fonts: Res<Fonts>,
    screen: Res<BattleScreen>,
) {
    let want = reveal.active().then_some(reveal.phase);
    if reveal.built == want {
        return;
    }
    for root in &roots {
        commands.entity(root).despawn();
    }
    reveal.built = want;
    let Some(phase) = want else {
        return;
    };
    let data = &screen.data;
    let mut root = commands.spawn((
        RevealRoot,
        DespawnOnExit(Screen::Tavern),
        Node {
            position_type: PositionType::Absolute,
            width: percent(100),
            height: percent(100),
            flex_direction: FlexDirection::Column,
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            row_gap: px(16),
            ..default()
        },
        BackgroundColor(Color::srgb(0.03, 0.02, 0.04)),
        GlobalZIndex(30),
    ));
    // Сцена — интерьер таверны в полутьме, экран под ней не просвечивает.
    root.with_child((
        Node {
            position_type: PositionType::Absolute,
            width: percent(100),
            height: percent(100),
            ..default()
        },
        ImageNode::new(assets.load("backgrounds/tavern.jpg"))
            .with_color(Color::srgb(0.32, 0.27, 0.30)),
        Pickable::IGNORE,
    ));
    // Тап по экрану: карты — открыть все сразу, витрина — дальше.
    if phase != Phase::Summary {
        root.observe(|_: On<PointerClick>, mut reveal: ResMut<HireReveal>| {
            if reveal.phase == Phase::Cards {
                let done = reveal.cards_done_at();
                if reveal.t < done - CARDS_TAIL {
                    reveal.t = done - CARDS_TAIL;
                } else {
                    reveal.advance();
                }
            } else {
                reveal.advance();
            }
        });
    }
    root.with_children(|root| match phase {
        Phase::Cards => cards(root, &fonts, &reveal.list),
        Phase::Show(n) => {
            let pulled = &reveal.list[reveal.shows[n]];
            showcase(
                root,
                &assets,
                &fonts,
                data,
                screen.field.animated.get(&pulled.id),
                pulled,
            );
        }
        Phase::Summary => summary(root, &assets, &fonts, data, &reveal.list),
    });
    if phase != Phase::Summary {
        root.with_children(|root| skip_button(root, &fonts));
    }
    // Вспышка поверх всего (гаснет сама).
    root.with_child((
        Flash,
        Node {
            position_type: PositionType::Absolute,
            width: percent(100),
            height: percent(100),
            ..default()
        },
        BackgroundColor(Color::NONE),
        Pickable::IGNORE,
    ));
}

fn skip_button(root: &mut ChildSpawnerCommands, fonts: &Fonts) {
    let mut skip = root.spawn((
        Node {
            position_type: PositionType::Absolute,
            top: px(16),
            right: px(16),
            height: px(44),
            padding: UiRect::horizontal(px(16)),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            ..default()
        },
        Skin(Look::Dark),
    ));
    skip.with_child(text(fonts, "Пропустить »", true, 15.0, PARCHMENT));
    pressable(&mut skip);
    skip.observe(
        |mut click: On<PointerClick>, mut reveal: ResMut<HireReveal>| {
            click.propagate(false);
            reveal.skip();
        },
    );
}

/// Рубашки: одна большая карта или две строки по пять.
fn cards(root: &mut ChildSpawnerCommands, fonts: &Fonts, list: &[Pulled]) {
    let single = list.len() == 1;
    let (w, h) = if single {
        (200.0, 300.0)
    } else {
        (92.0, 138.0)
    };
    root.spawn(text(
        fonts,
        if single {
            "Сундук вольного бойца…"
        } else {
            "Десять сундуков у стойки…"
        },
        true,
        20.0,
        PARCHMENT,
    ));
    root.spawn(Node {
        width: px(520),
        flex_wrap: FlexWrap::Wrap,
        justify_content: JustifyContent::Center,
        column_gap: px(8),
        row_gap: px(14),
        ..default()
    })
    .with_children(|grid| {
        for (i, p) in list.iter().enumerate() {
            // Свечение рубашки: золото — копия на звезду, фиолет — копия героя, без свечения — ресурсы.
            let glow = if p.star {
                GOLD.with_alpha(0.8)
            } else if p.copy() {
                COPY_GLOW.with_alpha(0.8)
            } else {
                Color::NONE
            };
            grid.spawn((
                HireCard(i),
                Node {
                    width: px(w),
                    height: px(h),
                    ..default()
                },
                UiTransform::default(),
                BoxShadow::new(glow, px(0), px(0), px(4), px(if single { 26 } else { 14 })),
            ))
            .with_children(|card| {
                card.spawn((
                    CardBack,
                    Node {
                        position_type: PositionType::Absolute,
                        width: percent(100),
                        height: percent(100),
                        justify_content: JustifyContent::Center,
                        align_items: AlignItems::Center,
                        border: UiRect::all(px(3)),
                        border_radius: BorderRadius::all(px(12)),
                        ..default()
                    },
                    BackgroundColor(Color::srgb(0.20, 0.12, 0.08)),
                    BorderColor::all(Color::srgb(0.79, 0.59, 0.25)),
                ))
                .with_children(|back| {
                    // Сургучная печать с вопросом.
                    let seal = if single { 96.0 } else { 46.0 };
                    back.spawn((
                        Node {
                            width: px(seal),
                            height: px(seal),
                            justify_content: JustifyContent::Center,
                            align_items: AlignItems::Center,
                            border: UiRect::all(px(2)),
                            border_radius: BorderRadius::all(px(seal / 2.0)),
                            ..default()
                        },
                        BackgroundColor(Color::srgb(0.55, 0.12, 0.10)),
                        BorderColor::all(Color::srgb(0.79, 0.59, 0.25)),
                    ))
                    .with_child((
                        Text::new("?"),
                        fonts.display(seal * 0.6),
                        TextColor(Color::srgb(1.0, 0.85, 0.6)),
                    ));
                });
                card.spawn((
                    CardFront,
                    Node {
                        position_type: PositionType::Absolute,
                        width: percent(100),
                        height: percent(100),
                        padding: UiRect::top(px(if single { 24.0 } else { 8.0 })),
                        flex_direction: FlexDirection::Column,
                        align_items: AlignItems::Center,
                        row_gap: px(2),
                        border_radius: BorderRadius::all(px(12)),
                        ..default()
                    },
                    Skin(Look::Parchment),
                    Outline::new(
                        px(3),
                        px(0),
                        if p.star {
                            GOLD
                        } else if p.copy() {
                            COPY_GLOW
                        } else {
                            BRASS_EDGE
                        },
                    ),
                    Visibility::Hidden,
                ));
            });
        }
    });
    root.spawn(text(
        fonts,
        if single {
            "нажмите, чтобы открыть"
        } else {
            "нажмите, чтобы открыть все"
        },
        false,
        13.0,
        MUTED,
    ));
}

/// Лицевая сторона: медальон, имя, редкость, новый ли.
fn card_face(
    card: &mut ChildSpawnerCommands,
    assets: &AssetServer,
    fonts: &Fonts,
    data: &GameData,
    p: &Pulled,
    big: bool,
) {
    let k = if big { 1.8 } else { 1.0 };
    if let Some((kind, n)) = p.loot.and_then(|l| l.main()) {
        card.spawn((
            Node {
                width: px(56.0 * k),
                height: px(56.0 * k),
                margin: UiRect::top(px(4.0 * k)),
                ..default()
            },
            ImageNode::new(assets.load(format!("ui/icon_{kind}.png"))),
        ));
        card.spawn(text(fonts, format!("+{n}"), true, 16.0 * k, INK));
        card.spawn(text(fonts, "из сундука", false, 12.0 * k, INK_SOFT));
        return;
    }
    let Some(hero) = data.hero(&p.id) else {
        return;
    };
    card.spawn(style::medallion(assets, &hero.id, 62.0 * k, false));
    card.spawn(text(fonts, hero.name.clone(), true, 14.0 * k, INK));
    card.spawn(text(
        fonts,
        if p.star {
            "на звезду!"
        } else {
            "копия"
        },
        p.star,
        12.0 * k,
        if p.star {
            Color::srgb(0.70, 0.36, 0.05)
        } else {
            INK_SOFT
        },
    ));
}

/// Витрина: лучи, свечение, герой в позе победы, имя и титул.
fn showcase(
    root: &mut ChildSpawnerCommands,
    assets: &AssetServer,
    fonts: &Fonts,
    data: &GameData,
    art: Option<&AnimatedArt>,
    p: &Pulled,
) {
    let Some(hero) = data.hero(&p.id) else {
        return;
    };
    let color = if p.star {
        GOLD
    } else {
        crate::view::element_color(hero.element)
    };
    let rays = if p.star { 760.0 } else { 620.0 };
    root.spawn((
        Rays,
        Node {
            position_type: PositionType::Absolute,
            width: px(rays),
            height: px(rays),
            left: px(270.0 - rays / 2.0),
            top: px(380.0 - rays / 2.0),
            ..default()
        },
        ImageNode::new(assets.load("ui/rays.png")).with_color(color.with_alpha(0.55)),
        UiTransform::default(),
        Pickable::IGNORE,
    ));
    root.spawn((
        Node {
            position_type: PositionType::Absolute,
            width: px(420),
            height: px(420),
            left: px(60),
            top: px(190),
            ..default()
        },
        ImageNode::new(assets.load("fx/glow.png")).with_color(color.with_alpha(0.7)),
        Pickable::IGNORE,
    ));
    // Герой: кадр победы из спрайт-листа боя, без листа — фигура из интерфейса.
    let (image, rect, aspect) = match art {
        Some(a) => {
            let row = a
                .clips
                .get("victory")
                .or(a.clips.get("idle"))
                .map_or(0, |c| c.row);
            let [w, h] = a.frame;
            (
                a.sheet.clone(),
                Some(Rect::new(0.0, row as f32 * h, w, (row + 1) as f32 * h)),
                w / h,
            )
        }
        None => (format!("ui/full_{}.png", hero.id), None, 0.75),
    };
    let mut figure = ImageNode::new(assets.load(image));
    figure.rect = rect;
    root.spawn((
        Figure,
        Node {
            position_type: PositionType::Absolute,
            height: px(FIGURE_H),
            width: px(FIGURE_H * aspect),
            left: px(270.0 - FIGURE_H * aspect / 2.0),
            top: px(150),
            ..default()
        },
        figure,
        UiTransform::default(),
        Pickable::IGNORE,
    ));
    // Подписи — внизу, поверх лучей.
    root.spawn((
        Node {
            position_type: PositionType::Absolute,
            top: px(600),
            width: percent(100),
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::Center,
            row_gap: px(4),
            ..default()
        },
        Pickable::IGNORE,
    ))
    .with_children(|info| {
        info.spawn((
            Text::new(hero.name.clone()),
            fonts.display(46.0),
            TextColor(color),
            TextShadow::default(),
        ));
        info.spawn(text(
            fonts,
            {
                let role = crate::crew::role_label(hero.role);
                let element = crate::crew::element_label(hero.element);
                if hero.title == role {
                    format!("{role} · {element}")
                } else {
                    format!("{} · {role} · {element}", hero.title)
                }
            },
            false,
            16.0,
            PARCHMENT,
        ));
        info.spawn((
            Node {
                margin: UiRect::top(px(10)),
                padding: UiRect::axes(px(18), px(6)),
                border_radius: BorderRadius::all(px(16)),
                ..default()
            },
            BackgroundColor(if p.star {
                Color::srgb(0.62, 0.36, 0.08)
            } else {
                Color::srgba(1.0, 1.0, 1.0, 0.12)
            }),
        ))
        .with_child(text(
            fonts,
            if p.star {
                "Копий хватает на звезду — слияние в Экипаже!"
            } else {
                "Копия — пойдёт на звезду героя"
            },
            true,
            16.0,
            Color::WHITE,
        ));
        info.spawn((
            Node {
                margin: UiRect::top(px(18)),
                ..default()
            },
            text(fonts, "нажмите, чтобы продолжить »", false, 13.0, MUTED),
        ));
    });
}

fn summary(
    root: &mut ChildSpawnerCommands,
    assets: &AssetServer,
    fonts: &Fonts,
    data: &GameData,
    list: &[Pulled],
) {
    let any_star = list.iter().any(|p| p.star);
    root.spawn((
        Text::new(if any_star {
            "Звезда ждёт слияния!"
        } else {
            "Итоги найма"
        }),
        fonts.display(30.0),
        TextColor(GOLD),
    ));
    root.spawn(Node {
        width: px(520),
        flex_wrap: FlexWrap::Wrap,
        justify_content: JustifyContent::Center,
        column_gap: px(8),
        row_gap: px(10),
        ..default()
    })
    .with_children(|grid| {
        for p in list {
            grid.spawn((
                Node {
                    width: px(96),
                    height: px(150),
                    padding: UiRect::top(px(10)),
                    flex_direction: FlexDirection::Column,
                    align_items: AlignItems::Center,
                    row_gap: px(2),
                    border_radius: BorderRadius::all(px(12)),
                    ..default()
                },
                Skin(Look::Parchment),
                Outline::new(
                    px(3),
                    px(0),
                    if p.star {
                        GOLD
                    } else if p.copy() {
                        COPY_GLOW
                    } else {
                        BRASS_EDGE
                    },
                ),
            ))
            .with_children(|card| card_face(card, assets, fonts, data, p, false));
        }
    });
    let mut done = root.spawn((
        Node {
            width: px(220),
            height: px(56),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            ..default()
        },
        Skin(Look::Go),
    ));
    done.with_child(text(fonts, "Забрать", true, 20.0, Color::WHITE));
    pressable(&mut done);
    done.observe(|_: On<PointerClick>, mut reveal: ResMut<HireReveal>| {
        reveal.advance();
    });
}

/// Рубашки дрожат, светятся и переворачиваются; открытие — звук и (у SSR) вспышка.
#[allow(clippy::too_many_arguments)]
fn animate_cards(
    time: Res<Time>,
    mut commands: Commands,
    mut reveal: ResMut<HireReveal>,
    assets: Res<AssetServer>,
    fonts: Res<Fonts>,
    screen: Res<BattleScreen>,
    mut cards: Query<(&HireCard, &mut UiTransform, &mut BoxShadow, &Children)>,
    mut backs: Query<&mut Visibility, (With<CardBack>, Without<CardFront>)>,
    mut fronts: Query<&mut Visibility, (With<CardFront>, Without<CardBack>)>,
    mut flash: Query<&mut BackgroundColor, With<Flash>>,
    mut sfx: MessageWriter<Sfx>,
) {
    if reveal.phase != Phase::Cards || !reveal.active() {
        return;
    }
    let t = reveal.t;
    let single = reveal.list.len() == 1;
    let mut ssr_opened = false;
    for (HireCard(i), mut transform, mut shadow, children) in &mut cards {
        let p = reveal.list[*i].clone();
        let at = reveal.flips[*i];
        let k = ((t - at) / FLIP_TIME).clamp(0.0, 1.0);
        // Ожидание: SSR дрожит перед открытием, рубашки редких пульсируют.
        let wait = at - t;
        transform.translation = if p.star && wait > 0.0 && wait < SSR_SUSPENSE {
            Val2::px((t * 70.0).sin() * 4.0 * (1.0 - wait / SSR_SUSPENSE), 0.0)
        } else {
            Val2::ZERO
        };
        if let Some(style) = shadow.0.first_mut()
            && p.copy()
        {
            let pulse = 0.55 + 0.45 * (t * 5.0 + *i as f32).sin().abs();
            style.color = if p.star { GOLD } else { COPY_GLOW }.with_alpha(0.9 * pulse);
        }
        // Переворот: первая половина — рубашка сжимается, вторая — лицо разворачивается.
        transform.scale.x = if k < 0.5 {
            1.0 - k * 2.0
        } else {
            k * 2.0 - 1.0
        }
        .max(0.02);
        if k >= 0.5 && !reveal.opened[*i] {
            reveal.opened[*i] = true;
            for child in children {
                if let Ok(mut vis) = backs.get_mut(*child) {
                    *vis = Visibility::Hidden;
                }
                if let Ok(mut vis) = fronts.get_mut(*child) {
                    *vis = Visibility::Inherited;
                    commands.entity(*child).with_children(|face| {
                        card_face(face, &assets, &fonts, &screen.data, &p, single);
                    });
                }
            }
            sfx.write(Sfx(if p.star {
                "ssr"
            } else if p.copy() {
                "skill"
            } else {
                "flip"
            }));
            if p.star {
                ssr_opened = true;
            }
        }
    }
    for mut bg in &mut flash {
        bg.0 = if ssr_opened {
            Color::srgba(1.0, 0.95, 0.8, 0.9)
        } else {
            bg.0.with_alpha((bg.0.alpha() - time.delta_secs() / FLASH_TIME).max(0.0))
        };
    }
}

/// Фигура героя на витрине (не лучи).
type FigureQuery<'w, 's> = Query<
    'w,
    's,
    (&'static mut UiTransform, &'static mut ImageNode),
    (With<Figure>, Without<Rays>),
>;

/// Витрина: лучи вращаются, герой поднимается и проявляется, при входе — вспышка и звук.
fn animate_show(
    time: Res<Time>,
    reveal: Res<HireReveal>,
    mut rays: Query<&mut UiTransform, (With<Rays>, Without<Figure>)>,
    mut figure: FigureQuery,
    mut flash: Query<&mut BackgroundColor, With<Flash>>,
    mut played: Local<Option<usize>>,
    mut sfx: MessageWriter<Sfx>,
) {
    let Phase::Show(n) = reveal.phase else {
        *played = None;
        return;
    };
    if !reveal.active() {
        return;
    }
    let p = &reveal.list[reveal.shows[n]];
    if *played != Some(n) {
        *played = Some(n);
        sfx.write(Sfx(if p.star { "ssr" } else { "levelup" }));
        for mut bg in &mut flash {
            bg.0 = Color::srgba(1.0, 1.0, 1.0, if p.star { 1.0 } else { 0.6 });
        }
    }
    let t = reveal.t;
    for mut transform in &mut rays {
        transform.rotation = Rot2::radians(time.elapsed_secs() * RAYS_TURN * std::f32::consts::TAU);
        let breathe = 1.0 + 0.04 * (time.elapsed_secs() * 2.0).sin();
        transform.scale = Vec2::splat(breathe);
    }
    let k = (t / RISE_TIME).min(1.0);
    let ease = 1.0 - (1.0 - k) * (1.0 - k);
    for (mut transform, mut image) in &mut figure {
        transform.translation = Val2::px(0.0, RISE_PX * (1.0 - ease));
        image.color = Color::WHITE.with_alpha(ease);
    }
    for mut bg in &mut flash {
        let a = bg.0.alpha();
        if a > 0.0 {
            bg.0 =
                bg.0.with_alpha((a - time.delta_secs() / FLASH_TIME).max(0.0));
        }
    }
}
