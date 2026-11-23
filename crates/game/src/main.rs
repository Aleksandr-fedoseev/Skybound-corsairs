//! Skybound Corsairs: главное меню на палубе «Ласточки» и бой «Живая палуба».
//!
//! Бой идёт сам (логика в battle-core), экран проигрывает его события анимациями моделей.
//! Игрок вмешивается ультами: кнопки героев внизу. Модели пока — силуэты-заглушки.

mod abyss;
mod audio;
mod battle_screen;
mod battlefield;
mod cabins;
mod contracts;
mod convoys;
mod crew;
mod dev;
mod events;
mod expeditions;
mod fx;
mod gear;
mod hire_reveal;
mod map;
mod menu;
mod save;
mod ship;
mod softbody;
mod spells;
mod story;
mod style;
mod supplies;
mod tavern;
mod view;

use bevy::prelude::*;
use game_data::GameData;

use battle_screen::{BattleScreen, Presentation, Shown};
use bevy::window::WindowResizeConstraints;

const WIDTH: f32 = 540.0;
/// `SKYBOUND_SCREEN=battle` — сразу в бой (для кадров боя и тестовой сцены).
const SCREEN_VAR: &str = "SKYBOUND_SCREEN";

/// Экран игры: меню корабля или бой.
#[derive(States, Default, Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Screen {
    #[default]
    Menu,
    Map,
    Crew,
    Tavern,
    Activities,
    Battle,
}
const HEIGHT: f32 = 960.0;

fn main() {
    // Окно в пропорциях телефона и неизменяемого размера: иначе тайловый оконный менеджер
    // (например, niri) растягивает его, и раскладка уже не та, что на телефоне.
    let window = Window {
        title: "Skybound Corsairs".into(),
        resolution: (WIDTH as u32, HEIGHT as u32).into(),
        resizable: false,
        resize_constraints: WindowResizeConstraints {
            min_width: WIDTH,
            min_height: HEIGHT,
            max_width: WIDTH,
            max_height: HEIGHT,
        },
        ..default()
    };
    let data = GameData::bundled().expect("данные из data/ корректны");
    let crew = crew::CrewData::bundled();
    let roster = crew::Roster::new(&crew);
    let team = roster.team_members();
    let rules = crew.clone();
    let mut app = App::new();
    app.add_plugins(DefaultPlugins.set(WindowPlugin {
        primary_window: Some(window),
        ..default()
    }))
    .add_plugins(dev::plugin)
    .insert_state(match std::env::var(SCREEN_VAR).as_deref() {
        Ok("battle") => Screen::Battle,
        _ => Screen::Menu,
    })
    .init_resource::<view::Fonts>()
    .add_plugins((
        menu::plugin,
        map::plugin,
        crew::plugin,
        tavern::plugin,
        ship::plugin,
        audio::plugin,
        save::plugin,
        story::plugin,
        convoys::plugin,
        contracts::plugin,
        abyss::plugin,
        expeditions::plugin,
        cabins::plugin,
        gear::plugin,
        events::plugin,
    ))
    .add_plugins(supplies::plugin)
    .add_plugins(hire_reveal::plugin)
    .add_plugins(style::plugin)
    .add_message::<Shown>()
    .insert_resource(crew)
    .insert_resource(roster)
    .insert_resource(BattleScreen::new(
        data,
        Presentation::bundled(),
        battlefield::FieldConfig::bundled(),
        team,
        rules,
    ))
    .init_resource::<fx::BandFx>()
    .init_resource::<fx::ScreenFx>()
    .add_systems(Startup, view::setup)
    .add_systems(OnEnter(Screen::Battle), battle_screen::enter)
    .add_systems(OnExit(Screen::Battle), battle_screen::leave)
    .add_systems(
        Update,
        (
            battle_screen::keyboard,
            view::rebuild,
            battle_screen::play_events,
            fx::spawn_effects,
            spells::spawn_spells,
            (
                view::refresh_status,
                view::refresh_ult_buttons,
                view::refresh_skill_badges,
                view::refresh_controls,
                view::refresh_result,
                battlefield::refresh_overhead,
            ),
            (
                battlefield::animate_models,
                softbody::animate_soft_bodies,
                fx::animate_floating,
                fx::animate_projectiles,
                spells::animate_spells,
                spells::animate_blades,
                spells::animate_whirls,
                fx::animate_band,
                fx::animate_screen,
            ),
        )
            .chain()
            .run_if(in_state(Screen::Battle)),
    );
    // Сохранение заменяет стартовые ресурсы — до первого кадра, пока экран не собран.
    if let Some(save) = save::load() {
        save::apply(&mut app, save);
    }
    app.run();
}
