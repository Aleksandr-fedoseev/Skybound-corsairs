//! Skybound Corsairs — прототип экрана боя.
//!
//! Пока это текстовый экран автобоя на заглушках: проверяем, что логика из battle-core
//! правильно отображается в Bevy. Арт, анимации и ручное управление — следующие шаги.

mod dev;
mod text;

use std::collections::VecDeque;
use std::time::{SystemTime, UNIX_EPOCH};

use battle_core::{
    BROADSIDE_MAX, Battle, ENERGY_MAX, Outcome, SKILL_POINTS_MAX, Ship, Side, UnitId,
};
use bevy::prelude::*;
use game_data::GameData;

const STARTER_TEAM: [&str; 4] = ["ren", "kaya", "borg", "mei"];
const STAGE: &str = "1-1";
const HERO_LEVEL: u32 = 1;
const SHIP: Ship = Ship {
    broadside_power: 120,
};
const STEP_SECONDS: f32 = 0.8;
const LOG_LINES: usize = 14;

const SKY: Color = Color::srgb(0.53, 0.75, 0.92);
const PANEL: Color = Color::srgba(0.05, 0.10, 0.20, 0.75);
const ALLY: Color = Color::srgb(0.85, 0.95, 1.0);
const ENEMY: Color = Color::srgb(1.0, 0.82, 0.78);
const DEFEATED: Color = Color::srgb(0.5, 0.5, 0.5);
const ACCENT: Color = Color::srgb(1.0, 0.85, 0.35);

fn main() {
    let mut window = Window {
        title: "Skybound Corsairs".into(),
        resolution: (540, 960).into(),
        ..default()
    };
    dev::fix_window_size(&mut window);
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(window),
            ..default()
        }))
        .insert_resource(ClearColor(SKY))
        .add_plugins(dev::plugin)
        .add_systems(Startup, setup)
        .add_systems(Update, (handle_input, tick_battle, refresh_ui).chain())
        .run();
}

#[derive(Resource)]
struct Fonts {
    regular: Handle<Font>,
    bold: Handle<Font>,
}

#[derive(Resource)]
struct BattleScreen {
    data: GameData,
    battle: Battle,
    timer: Timer,
    speed: u32,
    paused: bool,
    log: VecDeque<String>,
}

impl BattleScreen {
    fn new(data: GameData) -> Self {
        let battle = new_battle(&data);
        let mut screen = Self {
            data,
            battle,
            timer: Timer::from_seconds(STEP_SECONDS, TimerMode::Repeating),
            speed: 1,
            paused: false,
            log: VecDeque::new(),
        };
        screen.push_log(format!("Этап {STAGE}. В бой!"));
        screen
    }

    fn restart(&mut self) {
        self.battle = new_battle(&self.data);
        self.log.clear();
        self.push_log(format!("Этап {STAGE}. Новый бой!"));
    }

    fn push_log(&mut self, line: String) {
        self.log.push_back(line);
        while self.log.len() > LOG_LINES {
            self.log.pop_front();
        }
    }
}

fn new_battle(data: &GameData) -> Battle {
    let team = STARTER_TEAM
        .iter()
        .map(|id| {
            data.hero(id)
                .expect("герой есть в data/heroes.json")
                .to_unit(HERO_LEVEL)
        })
        .collect();
    let enemies = data
        .stage_enemies(STAGE)
        .expect("этап есть в data/stages.json");
    let seed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);
    Battle::new(team, enemies, SHIP, seed)
}

/// Какой текст показывает элемент интерфейса.
#[derive(Component, Clone, Copy)]
enum UiSlot {
    TurnOrder,
    Unit(UnitId),
    Status,
    Log,
}

fn setup(mut commands: Commands, assets: Res<AssetServer>) {
    let fonts = Fonts {
        regular: assets.load("fonts/NotoSans-Regular.ttf"),
        bold: assets.load("fonts/NotoSans-Bold.ttf"),
    };
    let data = GameData::bundled().expect("данные из data/ корректны");
    let screen = BattleScreen::new(data);

    commands.spawn(Camera2d);

    let font = |handle: &Handle<Font>, size: f32| TextFont {
        font: handle.clone().into(),
        font_size: FontSize::Px(size),
        ..default()
    };

    let enemies: Vec<UnitId> = screen
        .battle
        .units()
        .iter()
        .filter(|u| u.side == Side::Enemy)
        .map(|u| u.id)
        .collect();
    let allies: Vec<UnitId> = screen
        .battle
        .units()
        .iter()
        .filter(|u| u.side == Side::Ally)
        .map(|u| u.id)
        .collect();

    let panel = || {
        (
            Node {
                flex_direction: FlexDirection::Column,
                padding: UiRect::all(Val::Px(10.0)),
                row_gap: Val::Px(4.0),
                ..default()
            },
            BackgroundColor(PANEL),
        )
    };

    commands
        .spawn(Node {
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            flex_direction: FlexDirection::Column,
            padding: UiRect::all(Val::Px(16.0)),
            row_gap: Val::Px(12.0),
            ..default()
        })
        .with_children(|root| {
            root.spawn((
                Text::new("Skybound Corsairs"),
                font(&fonts.bold, 30.0),
                TextColor(Color::WHITE),
            ));
            root.spawn(panel()).with_children(|p| {
                p.spawn((
                    Text::new(""),
                    font(&fonts.regular, 15.0),
                    TextColor(ACCENT),
                    UiSlot::TurnOrder,
                ));
            });
            root.spawn(panel()).with_children(|p| {
                p.spawn((
                    Text::new("Враги"),
                    font(&fonts.bold, 18.0),
                    TextColor(ENEMY),
                ));
                for id in &enemies {
                    p.spawn((
                        Text::new(""),
                        font(&fonts.regular, 16.0),
                        TextColor(ENEMY),
                        UiSlot::Unit(*id),
                    ));
                }
            });
            root.spawn(panel()).with_children(|p| {
                p.spawn((
                    Text::new("Экипаж"),
                    font(&fonts.bold, 18.0),
                    TextColor(ALLY),
                ));
                for id in &allies {
                    p.spawn((
                        Text::new(""),
                        font(&fonts.regular, 16.0),
                        TextColor(ALLY),
                        UiSlot::Unit(*id),
                    ));
                }
            });
            root.spawn(panel()).with_children(|p| {
                p.spawn((
                    Text::new(""),
                    font(&fonts.regular, 15.0),
                    TextColor(Color::WHITE),
                    UiSlot::Status,
                ));
            });
            root.spawn((
                Node {
                    flex_grow: 1.0,
                    flex_direction: FlexDirection::Column,
                    padding: UiRect::all(Val::Px(10.0)),
                    ..default()
                },
                BackgroundColor(PANEL),
            ))
            .with_children(|p| {
                p.spawn((
                    Text::new(""),
                    font(&fonts.regular, 14.0),
                    TextColor(Color::WHITE),
                    UiSlot::Log,
                ));
            });
            root.spawn((
                Text::new("Пробел — пауза   X — скорость   R — новый бой"),
                font(&fonts.regular, 13.0),
                TextColor(Color::srgb(0.1, 0.2, 0.35)),
            ));
        });

    commands.insert_resource(fonts);
    commands.insert_resource(screen);
}

fn handle_input(keys: Res<ButtonInput<KeyCode>>, mut screen: ResMut<BattleScreen>) {
    if keys.just_pressed(KeyCode::Space) {
        screen.paused = !screen.paused;
    }
    if keys.just_pressed(KeyCode::KeyX) {
        screen.speed = screen.speed % 3 + 1;
    }
    if keys.just_pressed(KeyCode::KeyR) {
        screen.restart();
    }
}

fn tick_battle(time: Res<Time>, mut screen: ResMut<BattleScreen>) {
    if screen.paused || screen.battle.outcome().is_some() {
        return;
    }
    let delta = time.delta().mul_f32(screen.speed as f32);
    screen.timer.tick(delta);
    for _ in 0..screen.timer.times_finished_this_tick() {
        let events = screen.battle.auto_step();
        for event in &events {
            if let Some(line) = text::describe(&screen.battle, event) {
                screen.push_log(line);
            }
        }
        if screen.battle.outcome().is_some() {
            break;
        }
    }
}

fn refresh_ui(screen: Res<BattleScreen>, mut texts: Query<(&mut Text, &mut TextColor, &UiSlot)>) {
    let battle = &screen.battle;
    for (mut text, mut color, slot) in &mut texts {
        match *slot {
            UiSlot::TurnOrder => {
                let order: Vec<&str> = battle
                    .turn_order_preview(7)
                    .into_iter()
                    .map(|id| battle.unit(id).def.name.as_str())
                    .collect();
                text.0 = if order.is_empty() {
                    "Очередь ходов пуста".into()
                } else {
                    format!("Очередь: {}", order.join(" > "))
                };
            }
            UiSlot::Unit(id) => {
                let unit = battle.unit(id);
                let marker = if battle.current() == Some(id) {
                    "> "
                } else {
                    "   "
                };
                let ult = match unit.def.ultimate {
                    Some(_) if unit.energy >= ENERGY_MAX => "  УЛЬТА!".to_string(),
                    Some(_) => format!("  энергия {}%", unit.energy * 100 / ENERGY_MAX),
                    None => String::new(),
                };
                text.0 = format!(
                    "{marker}{} [{}]  {}/{}{ult}",
                    unit.def.name,
                    text::element_name(unit.def.element),
                    unit.hp,
                    unit.def.stats.max_hp,
                );
                if !unit.is_alive() {
                    *color = TextColor(DEFEATED);
                } else {
                    *color = TextColor(if unit.side == Side::Ally { ALLY } else { ENEMY });
                }
            }
            UiSlot::Status => {
                let state = match battle.outcome() {
                    Some(Outcome::Victory) => "ПОБЕДА! (R — новый бой)".to_string(),
                    Some(Outcome::Defeat) => "ПОРАЖЕНИЕ (R — новый бой)".to_string(),
                    None if screen.paused => "Пауза".to_string(),
                    None => format!("Автобой x{}", screen.speed),
                };
                text.0 = format!(
                    "Ход {}   Очки умений {}/{}   Залп {}%\n{state}",
                    battle.turn(),
                    battle.skill_points(),
                    SKILL_POINTS_MAX,
                    battle.broadside() * 100 / BROADSIDE_MAX,
                );
            }
            UiSlot::Log => {
                text.0 = screen.log.iter().cloned().collect::<Vec<_>>().join("\n");
            }
        }
    }
}
