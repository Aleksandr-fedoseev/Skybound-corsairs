//! Сюжет: короткие сцены-диалоги перед этапом или после первой победы.
//!
//! Сцены — в `data/story.json`. Каждая показывается один раз (просмотренные хранятся в
//! сохранении); сцена может привести героя в команду. Тап — дальше, «Пропустить» — к концу.

use bevy::prelude::*;
use serde::Deserialize;

use crate::Screen;
use crate::audio::Sfx;
use crate::battle_screen::BattleScreen;
use crate::crew::Roster;
use crate::menu::{BRASS, GOLD, MUTED, PARCHMENT, Progress, Voyage, pressable, text};
use crate::view::{Fonts, element_color};

const VALE: Color = Color::srgb(0.20, 0.26, 0.42);
const DARK_TEXT: Color = Color::srgb(0.08, 0.10, 0.16);
const PORTRAIT: f32 = 86.0;

#[derive(Deserialize)]
pub struct Scene {
    pub id: String,
    /// Перед первым заходом на этап.
    #[serde(default)]
    pub before: Option<String>,
    /// После первой победы на этапе.
    #[serde(default)]
    pub after: Option<String>,
    /// Герой, который присоединяется к команде в конце сцены.
    #[serde(default)]
    pub join: Option<String>,
    pub lines: Vec<Line>,
}

#[derive(Deserialize)]
pub struct Line {
    /// id героя, `vale` или `narrator`.
    pub who: String,
    pub text: String,
}

#[derive(Resource)]
pub struct Story(pub Vec<Scene>);

impl Story {
    fn bundled() -> Self {
        Self(
            serde_json::from_str(include_str!("../../../data/story.json"))
                .expect("data/story.json корректен"),
        )
    }

    fn find(&self, seen: &[String], pick: impl Fn(&Scene) -> bool) -> Option<usize> {
        self.0.iter().position(|s| pick(s) && !seen.contains(&s.id))
    }
}

/// Просьба выйти в поход (этап уже записан в `Voyage`): сначала сцена, если есть.
#[derive(Message, Clone, Copy)]
pub struct VoyageRequest;

/// Этапы, только что пройденные впервые: после них может быть сцена.
#[derive(Resource, Default)]
pub struct StoryQueue(pub Vec<String>);

#[derive(Clone, Copy)]
struct Playing {
    scene: usize,
    line: usize,
    battle_after: bool,
}

#[derive(Resource, Default)]
struct Active(Option<Playing>);

#[derive(Component)]
struct DialogueRoot;

pub fn plugin(app: &mut App) {
    app.insert_resource(Story::bundled())
        .init_resource::<StoryQueue>()
        .init_resource::<Active>()
        .add_message::<VoyageRequest>()
        .add_systems(
            Update,
            (
                start_voyage,
                after_scenes.run_if(not(in_state(Screen::Battle))),
                show_dialogue,
            )
                .chain(),
        );
}

fn start_voyage(
    mut requests: MessageReader<VoyageRequest>,
    voyage: Res<Voyage>,
    progress: Res<Progress>,
    screen: Res<BattleScreen>,
    story: Res<Story>,
    mut active: ResMut<Active>,
    mut next: ResMut<NextState<Screen>>,
) {
    if requests.read().count() == 0 {
        return;
    }
    let stages = &screen.data.stages;
    let stage = voyage.stage.unwrap_or(progress.next_stage(stages.len()));
    let id = &stages[stage].id;
    match story.find(&progress.seen, |s| s.before.as_ref() == Some(id)) {
        Some(scene) => {
            active.0 = Some(Playing {
                scene,
                line: 0,
                battle_after: true,
            })
        }
        None => next.set(Screen::Battle),
    }
}

fn after_scenes(
    mut queue: ResMut<StoryQueue>,
    progress: Res<Progress>,
    story: Res<Story>,
    mut active: ResMut<Active>,
) {
    if active.0.is_some() {
        return;
    }
    while let Some(id) = queue.0.pop() {
        if let Some(scene) = story.find(&progress.seen, |s| s.after.as_ref() == Some(&id)) {
            active.0 = Some(Playing {
                scene,
                line: 0,
                battle_after: false,
            });
            return;
        }
    }
}

/// Пересобрать окно диалога, когда сменилась реплика.
fn show_dialogue(
    mut commands: Commands,
    active: Res<Active>,
    story: Res<Story>,
    screen: Res<BattleScreen>,
    fonts: Res<Fonts>,
    roots: Query<Entity, With<DialogueRoot>>,
) {
    if !active.is_changed() {
        return;
    }
    for root in &roots {
        commands.entity(root).despawn();
    }
    let Some(playing) = active.0 else {
        return;
    };
    let Some(line) = story.0[playing.scene].lines.get(playing.line) else {
        return;
    };
    let hero = screen.data.hero(&line.who);
    let (name, color) = match (line.who.as_str(), hero) {
        ("narrator", _) => (None, MUTED),
        ("vale", _) => (Some("Адмирал Вейл".to_string()), VALE),
        (_, Some(h)) => (Some(h.name.clone()), element_color(h.element)),
        (who, None) => (Some(who.to_string()), MUTED),
    };

    let mut root = commands.spawn((
        DialogueRoot,
        Node {
            position_type: PositionType::Absolute,
            width: percent(100),
            height: percent(100),
            ..default()
        },
        BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.5)),
        GlobalZIndex(60),
    ));
    root.with_children(|r| {
        let mut skip = r.spawn((
            Node {
                position_type: PositionType::Absolute,
                top: px(72),
                right: px(12),
                padding: UiRect::axes(px(14), px(8)),
                border: UiRect::all(px(1)),
                border_radius: BorderRadius::all(px(10)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.11, 0.075, 0.05, 0.9)),
            BorderColor::all(BRASS),
        ));
        skip.with_child(text(&fonts, "Пропустить", true, 14.0, PARCHMENT));
        pressable(&mut skip);
        skip.observe(|mut ev: On<PointerClick>, mut commands: Commands| {
            ev.propagate(false);
            commands.run_system_cached_with(advance, true);
        });

        r.spawn((
            Node {
                position_type: PositionType::Absolute,
                left: px(10),
                bottom: px(28),
                width: px(520),
                min_height: px(170),
                padding: UiRect::all(px(16)),
                column_gap: px(14),
                border: UiRect::all(px(2)),
                border_radius: BorderRadius::all(px(16)),
                ..default()
            },
            BackgroundColor(Color::srgb(0.11, 0.075, 0.05)),
            BorderColor::all(BRASS),
            Pickable::IGNORE,
        ))
        .with_children(|panel| {
            if let Some(name) = &name {
                panel
                    .spawn((
                        Node {
                            width: px(PORTRAIT),
                            height: px(PORTRAIT),
                            flex_shrink: 0.0,
                            justify_content: JustifyContent::Center,
                            align_items: AlignItems::Center,
                            border: UiRect::all(px(3)),
                            border_radius: BorderRadius::MAX,
                            ..default()
                        },
                        BackgroundColor(color),
                        BorderColor::all(GOLD),
                        Pickable::IGNORE,
                    ))
                    .with_child((
                        Text::new(name.chars().next().map(String::from).unwrap_or_default()),
                        fonts.text(true, 40.0),
                        TextColor(if line.who == "vale" {
                            PARCHMENT
                        } else {
                            DARK_TEXT
                        }),
                        Pickable::IGNORE,
                    ));
            }
            panel
                .spawn((
                    Node {
                        flex_grow: 1.0,
                        flex_shrink: 1.0,
                        flex_direction: FlexDirection::Column,
                        row_gap: px(6),
                        ..default()
                    },
                    Pickable::IGNORE,
                ))
                .with_children(|col| {
                    if let Some(name) = &name {
                        col.spawn(text(&fonts, name.clone(), true, 18.0, GOLD));
                    }
                    col.spawn(text(
                        &fonts,
                        line.text.clone(),
                        false,
                        16.0,
                        if name.is_some() { PARCHMENT } else { MUTED },
                    ));
                    col.spawn((
                        Node {
                            align_self: AlignSelf::FlexEnd,
                            margin: UiRect::top(px(4)),
                            ..default()
                        },
                        text(&fonts, "нажмите, чтобы продолжить", false, 12.0, MUTED),
                    ));
                });
        });
    });
    root.observe(|_: On<PointerClick>, mut commands: Commands| {
        commands.run_system_cached_with(advance, false);
    });
}

/// Следующая реплика; в конце сцены (или по «Пропустить») — итог сцены.
fn advance(
    In(skip): In<bool>,
    story: Res<Story>,
    mut active: ResMut<Active>,
    mut progress: ResMut<Progress>,
    mut roster: ResMut<Roster>,
    mut next: ResMut<NextState<Screen>>,
    mut sfx: MessageWriter<Sfx>,
) {
    let Some(mut playing) = active.0 else {
        return;
    };
    let scene = &story.0[playing.scene];
    playing.line += 1;
    if !skip && playing.line < scene.lines.len() {
        sfx.write(Sfx("click"));
        active.0 = Some(playing);
        return;
    }
    active.0 = None;
    progress.seen.push(scene.id.clone());
    if let Some(hero) = &scene.join {
        roster.recruit(hero);
        sfx.write(Sfx("levelup"));
    }
    if playing.battle_after {
        next.set(Screen::Battle);
    }
}
