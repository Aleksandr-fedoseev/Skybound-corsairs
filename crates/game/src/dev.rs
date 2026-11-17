//! Инструменты разработки.
//!
//! `SKYBOUND_SCREENSHOT=shot.png cargo run -p game` — игра сохранит кадр через несколько
//! секунд и закроется. Так можно проверять экран без ручного запуска (в том числе агенту).

use bevy::prelude::*;
use bevy::render::view::screenshot::{Screenshot, save_to_disk};
use bevy::window::WindowResizeConstraints;

const SCREENSHOT_VAR: &str = "SKYBOUND_SCREENSHOT";
const SHOT_AFTER_SECONDS: f32 = 4.0;
const EXIT_AFTER_SECONDS: f32 = 5.0;

#[derive(Resource)]
struct AutoScreenshot {
    path: String,
    elapsed: f32,
    taken: bool,
}

/// В режиме скриншота окно получает неизменяемый размер. Иначе тайловый оконный менеджер
/// (например, niri) растягивает его, и кадр выходит не в пропорциях телефона.
pub fn fix_window_size(window: &mut Window) {
    if std::env::var_os(SCREENSHOT_VAR).is_none() {
        return;
    }
    let (width, height) = (window.resolution.width(), window.resolution.height());
    window.resizable = false;
    window.resize_constraints = WindowResizeConstraints {
        min_width: width,
        min_height: height,
        max_width: width,
        max_height: height,
    };
}

pub fn plugin(app: &mut App) {
    if let Ok(path) = std::env::var(SCREENSHOT_VAR) {
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
