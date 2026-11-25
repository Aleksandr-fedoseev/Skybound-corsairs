//! Настройки игрока: музыка и звуки. Кнопка — в полосе ресурсов главной, сохраняются вместе с игрой.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::menu::{MenuRebuild, PARCHMENT, pressable};
use crate::style::{self, Look, Skin};
use crate::view::Fonts;

#[derive(Resource, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub music: bool,
    pub sfx: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            music: true,
            sfx: true,
        }
    }
}

/// Открыта ли панель настроек на главной.
#[derive(Resource, Default)]
pub struct SettingsOpen(pub bool);

#[derive(Clone, Copy)]
enum Toggle {
    Music,
    Sfx,
}

pub fn plugin(app: &mut App) {
    app.init_resource::<Settings>()
        .init_resource::<SettingsOpen>();
}

/// Кнопка-шестерёнка для полосы ресурсов.
pub fn button(bar: &mut ChildSpawnerCommands, assets: &AssetServer) {
    let mut b = bar.spawn((
        Node {
            width: px(40),
            height: px(40),
            flex_shrink: 0.0,
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            ..default()
        },
        Skin(Look::Dark),
    ));
    b.with_child(style::icon(assets, "settings", 30.0));
    pressable(&mut b);
    b.observe(
        |_: On<PointerClick>, mut open: ResMut<SettingsOpen>, mut rebuild: ResMut<MenuRebuild>| {
            open.0 = !open.0;
            rebuild.0 = true;
        },
    );
}

/// Панель настроек поверх главной.
pub fn panel(root: &mut ChildSpawnerCommands, fonts: &Fonts, settings: &Settings) {
    root.spawn((
        Node {
            position_type: PositionType::Absolute,
            top: px(70),
            left: px(60),
            right: px(60),
            padding: UiRect::axes(px(22), px(18)),
            flex_direction: FlexDirection::Column,
            row_gap: px(14),
            border: UiRect::all(px(2)),
            border_radius: BorderRadius::all(px(14)),
            ..default()
        },
        Skin(Look::Tile),
        GlobalZIndex(12),
    ))
    .with_children(|p| {
        p.spawn(Node {
            justify_content: JustifyContent::SpaceBetween,
            align_items: AlignItems::Center,
            ..default()
        })
        .with_children(|row| {
            row.spawn(style::heading(fonts, "Настройки", 24.0, PARCHMENT));
            let mut close = row.spawn((
                Node {
                    width: px(40),
                    height: px(36),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    ..default()
                },
                Skin(Look::Dark),
            ));
            close.with_child(style::heading(fonts, "×", 22.0, PARCHMENT));
            pressable(&mut close);
            close.observe(
                |_: On<PointerClick>,
                 mut open: ResMut<SettingsOpen>,
                 mut rebuild: ResMut<MenuRebuild>| {
                    open.0 = false;
                    rebuild.0 = true;
                },
            );
        });
        switch(p, fonts, "Музыка", settings.music, Toggle::Music);
        switch(p, fonts, "Звуки", settings.sfx, Toggle::Sfx);
    });
}

/// Строка «Музыка [вкл]»: тап — переключить.
fn switch(parent: &mut ChildSpawnerCommands, fonts: &Fonts, label: &str, on: bool, which: Toggle) {
    parent
        .spawn(Node {
            justify_content: JustifyContent::SpaceBetween,
            align_items: AlignItems::Center,
            ..default()
        })
        .with_children(|row| {
            row.spawn(style::heading(fonts, label, 20.0, PARCHMENT));
            let mut b = row.spawn((
                Node {
                    width: px(110),
                    height: px(44),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    ..default()
                },
                Skin(if on { Look::Go } else { Look::Off }),
            ));
            b.with_child(style::heading(
                fonts,
                if on { "вкл" } else { "выкл" },
                18.0,
                Color::WHITE,
            ));
            pressable(&mut b);
            b.observe(
                move |_: On<PointerClick>,
                      mut settings: ResMut<Settings>,
                      mut rebuild: ResMut<MenuRebuild>| {
                    match which {
                        Toggle::Music => settings.music = !settings.music,
                        Toggle::Sfx => settings.sfx = !settings.sfx,
                    }
                    rebuild.0 = true;
                },
            );
        });
}
