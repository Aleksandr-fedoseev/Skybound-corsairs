//! Skybound Corsairs — прототип экрана боя «Живая палуба».
//!
//! Бой идёт сам (логика в battle-core), экран проигрывает его события с анимациями.
//! Игрок вмешивается ультами: тап по светящейся карточке героя. Арт пока — заглушки.

mod battle_screen;
mod dev;
mod fx;
mod text;
mod view;

use bevy::prelude::*;
use game_data::GameData;

use battle_screen::{BattleScreen, Presentation, Shown};
use bevy::window::WindowResizeConstraints;

const WIDTH: f32 = 540.0;
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
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(window),
            ..default()
        }))
        .add_plugins(dev::plugin)
        .add_message::<Shown>()
        .insert_resource(BattleScreen::new(data, Presentation::bundled()))
        .init_resource::<fx::BannerFx>()
        .init_resource::<fx::ScreenFx>()
        .add_systems(Startup, view::setup)
        .add_systems(
            Update,
            (
                battle_screen::keyboard,
                view::rebuild,
                battle_screen::play_events,
                fx::spawn_effects,
                (
                    view::refresh_bars,
                    view::refresh_labels,
                    view::refresh_controls,
                    view::refresh_cards,
                    view::refresh_result,
                ),
                (
                    fx::animate_cards,
                    fx::animate_floating,
                    fx::animate_banner,
                    fx::animate_screen,
                ),
            )
                .chain(),
        )
        .run();
}
