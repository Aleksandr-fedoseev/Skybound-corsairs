//! Инструменты разработки.
//!
//! `SKYBOUND_SCREENSHOT=shot.png cargo run -p game` — игра сохранит кадр через несколько
//! секунд и закроется. Так можно проверять экран без ручного запуска (в том числе агенту).

use bevy::prelude::*;
use bevy::render::view::screenshot::{Screenshot, save_to_disk};

const SHOT_AFTER_SECONDS: f32 = 4.0;
const EXIT_AFTER_SECONDS: f32 = 5.0;

#[derive(Resource)]
struct AutoScreenshot {
    path: String,
    elapsed: f32,
    taken: bool,
}

pub fn plugin(app: &mut App) {
    if let Ok(path) = std::env::var("SKYBOUND_SCREENSHOT") {
        app.insert_resource(AutoScreenshot {
            path,
            elapsed: 0.0,
            taken: false,
        })
        .add_systems(Update, auto_screenshot);
    }
}

fn auto_screenshot(
    mut commands: Commands,
    time: Res<Time>,
    mut shot: ResMut<AutoScreenshot>,
    mut exit: MessageWriter<AppExit>,
) {
    shot.elapsed += time.delta_secs();
    if !shot.taken && shot.elapsed >= SHOT_AFTER_SECONDS {
        shot.taken = true;
        commands
            .spawn(Screenshot::primary_window())
            .observe(save_to_disk(shot.path.clone()));
    }
    if shot.elapsed >= EXIT_AFTER_SECONDS {
        exit.write(AppExit::Success);
    }
}
