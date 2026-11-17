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
    #[cfg(feature = "dev")]
    app.add_plugins(bevy_brp_extras::BrpExtrasPlugin)
        .add_systems(Update, (gym_keys, gym_text));
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

/// Клавиши тестовой сцены боя: этап, уровень героев, перечитать данные с диска.
#[cfg(feature = "dev")]
fn gym_keys(
    keys: Res<ButtonInput<KeyCode>>,
    mut screen: ResMut<crate::battle_screen::BattleScreen>,
) {
    let stage_keys = [
        KeyCode::Digit1,
        KeyCode::Digit2,
        KeyCode::Digit3,
        KeyCode::Digit4,
        KeyCode::Digit5,
    ];
    for (i, key) in stage_keys.iter().enumerate() {
        if keys.just_pressed(*key) && i < screen.data.stages.len() {
            screen.stage = i;
            screen.restart();
        }
    }
    if keys.just_pressed(KeyCode::BracketRight) {
        screen.level += 1;
        screen.restart();
    }
    if keys.just_pressed(KeyCode::BracketLeft) && screen.level > 1 {
        screen.level -= 1;
        screen.restart();
    }
    if keys.just_pressed(KeyCode::KeyL) {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data");
        let look = std::fs::read_to_string(dir.join("presentation.json"))
            .map_err(|e| e.to_string())
            .and_then(|s| serde_json::from_str(&s).map_err(|e| e.to_string()));
        match (game_data::GameData::load_dir(&dir), look) {
            (Ok(data), Ok(look)) => {
                screen.data = data;
                screen.look = look;
                screen.stage = screen.stage.min(screen.data.stages.len() - 1);
                screen.notice = Some("данные перечитаны".into());
                screen.restart();
            }
            (Err(e), _) => screen.notice = Some(format!("ошибка данных: {e}")),
            (_, Err(e)) => screen.notice = Some(format!("ошибка presentation.json: {e}")),
        }
    }
}

#[cfg(feature = "dev")]
fn gym_text(
    screen: Res<crate::battle_screen::BattleScreen>,
    mut texts: Query<&mut Text, With<crate::view::DevText>>,
) {
    for mut text in &mut texts {
        text.0 = format!(
            "GYM  этап {} (1–{}) · ур. {} ([ ]) · seed {} · R повтор · N новый · L данные{}",
            screen.stage_id(),
            screen.data.stages.len(),
            screen.level,
            screen.seed % 100_000,
            screen
                .notice
                .as_deref()
                .map(|n| format!(" · {n}"))
                .unwrap_or_default(),
        );
    }
}
