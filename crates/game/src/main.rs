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

fn main() {
    let mut window = Window {
        title: "Skybound Corsairs".into(),
        resolution: (540, 960).into(),
        ..default()
    };
    dev::fix_window_size(&mut window);
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
                (fx::animate_cards, fx::animate_floating, fx::animate_banner),
            )
                .chain(),
        )
        .run();
}
