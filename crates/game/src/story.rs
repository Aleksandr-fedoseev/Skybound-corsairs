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
use crate::menu::{MUTED, Profile, Progress, Toast, Voyage, pressable, text};
use crate::style::{Look, Skin, heading};
use crate::supplies::SupplyData;
use crate::view::Fonts;

/// Табличка с именем адмирала — имперская, тёмно-синяя.
const VALE: Look = Look::Dark;
const BRIGHT: Color = Color::srgb(1.0, 0.93, 0.78);
/// Высота фигуры говорящего над окном реплики.
const FIGURE: f32 = 470.0;

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

#[allow(clippy::too_many_arguments)]
fn start_voyage(
    mut requests: MessageReader<VoyageRequest>,
    voyage: Res<Voyage>,
    progress: Res<Progress>,
    screen: Res<BattleScreen>,
    story: Res<Story>,
    supplies: (Res<SupplyData>, Res<Profile>, ResMut<Toast>),
    mut active: ResMut<Active>,
    mut next: ResMut<NextState<Screen>>,
) {
    if requests.read().count() == 0 {
        return;
    }
    if voyage.custom.is_some() {
        next.set(Screen::Battle);
        return;
    }
    let stages = &screen.data.stages;
    let stage = voyage.stage.unwrap_or(progress.next_stage(stages.len()));
    let (supply, profile, mut toast) = supplies;
    let wins = progress.wins_today(stage, crate::cabins::today());
    if let Err(why) = supply.check(&profile, wins) {
        toast.show_long(&why);
        return;
    }
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
    assets: Res<AssetServer>,
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
    let name = match (line.who.as_str(), hero) {
        ("narrator", _) => None,
        ("vale", _) => Some("Адмирал Вейл".to_string()),
        (_, Some(h)) => Some(h.name.clone()),
        (who, None) => Some(who.to_string()),
    };

    let mut root = commands.spawn((
        DialogueRoot,
        Node {
            position_type: PositionType::Absolute,
            width: percent(100),
            height: percent(100),
            ..default()
        },
        BackgroundColor(Color::srgba(0.02, 0.02, 0.05, 0.6)),
        GlobalZIndex(60),
    ));
    root.with_children(|r| {
        // Говорящий герой во весь рост над окном реплики, как в визуальной новелле.
        if hero.is_some() {
            r.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    left: px(-6),
                    bottom: px(190),
                    height: px(FIGURE),
                    ..default()
                },
                ImageNode::new(assets.load(format!("ui/full_{}.png", line.who))),
                Pickable::IGNORE,
            ));
        }

        let mut skip = r.spawn((
            Node {
                position_type: PositionType::Absolute,
                top: px(16),
                right: px(12),
                height: px(42),
                padding: UiRect::horizontal(px(16)),
                align_items: AlignItems::Center,
                ..default()
            },
            Skin(Look::Dark),
        ));
        skip.with_child(heading(&fonts, "Пропустить »", 16.0, BRIGHT));
        pressable(&mut skip);
        skip.observe(|mut ev: On<PointerClick>, mut commands: Commands| {
            ev.propagate(false);
            commands.run_system_cached_with(advance, true);
        });

        r.spawn((
            Node {
                position_type: PositionType::Absolute,
                left: px(10),
                right: px(10),
                bottom: px(24),
                min_height: px(170),
                padding: UiRect::new(px(22), px(22), px(30), px(16)),
                flex_direction: FlexDirection::Column,
                row_gap: px(8),
                ..default()
            },
            Skin(Look::Tile),
            Pickable::IGNORE,
        ))
        .with_children(|panel| {
            // Табличка с именем на верхней кромке окна.
            if let Some(name) = &name {
                panel
                    .spawn((
                        Node {
                            position_type: PositionType::Absolute,
                            top: px(-22),
                            left: px(16),
                            height: px(42),
                            padding: UiRect::horizontal(px(18)),
                            align_items: AlignItems::Center,
                            ..default()
                        },
                        Skin(if line.who == "vale" { VALE } else { Look::Go }),
                        Pickable::IGNORE,
                    ))
                    .with_child(heading(&fonts, name.clone(), 20.0, Color::WHITE));
            }
            panel.spawn((
                Text::new(line.text.clone()),
                fonts.serif(19.0),
                TextColor(if name.is_some() { BRIGHT } else { MUTED }),
                Pickable::IGNORE,
            ));
            panel.spawn((
                Node {
                    align_self: AlignSelf::FlexEnd,
                    ..default()
                },
                text(&fonts, "нажмите, чтобы продолжить »", false, 12.0, MUTED),
            ));
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
