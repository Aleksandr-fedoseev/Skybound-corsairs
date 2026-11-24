//! Верфь: корабли покупаются целиком (решения №57–58), а не строится корпус.
//!
//! Номер корабля — уровень `hull`: он открывает отсеки и задаёт их потолок. Новый корабль
//! открывается после этапа главы и стоит серебро, доски и эфир; отсеки с уровнями переезжают.
//! Пока у всех кораблей рисунок «Ласточки» — свои рисунки появятся позже.

use bevy::prelude::*;

use crate::audio::Sfx;
use crate::battle_screen::BattleScreen;
use crate::menu::{GOLD, MUTED, MenuRebuild, PARCHMENT, Profile, Progress, Toast, pressable, text};
use crate::ship::{OpenCompartment, Ship, ShipData};
use crate::style::{Look, Skin, heading, icon};
use crate::view::Fonts;

const BRIGHT: Color = Color::srgb(1.0, 0.93, 0.78);
const LACK: Color = Color::srgb(1.0, 0.55, 0.45);

#[allow(clippy::too_many_arguments)]
pub fn board(
    root: &mut ChildSpawnerCommands,
    assets: &AssetServer,
    fonts: &Fonts,
    data: &ShipData,
    ship: &Ship,
    profile: &Profile,
    cleared: usize,
    stages: &[String],
) {
    let current = ship.hull();
    let next = ship.next_ship(data, cleared, stages);
    root.spawn((
        Node {
            position_type: PositionType::Absolute,
            left: px(10),
            top: px(74),
            width: px(520),
            padding: UiRect::axes(px(16), px(14)),
            flex_direction: FlexDirection::Column,
            row_gap: px(8),
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
            row.spawn(heading(fonts, "Верфь", 24.0, BRIGHT));
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
            close.with_child(heading(fonts, "×", 20.0, PARCHMENT));
            pressable(&mut close);
            close.observe(
                |_: On<PointerClick>,
                 mut open: ResMut<OpenCompartment>,
                 mut rebuild: ResMut<MenuRebuild>| {
                    open.0 = None;
                    rebuild.0 = true;
                },
            );
        });
        p.spawn(text(
            fonts,
            "Корабль покупают целиком: отсеки с их уровнями переезжают. Крупнее корабль — выше потолок отсеков и новые отсеки.",
            false,
            13.0,
            MUTED,
        ));
        for class in &data.ships {
            let tier = class.tier;
            let owned = tier <= current;
            let is_next = tier == current + 1;
            // Какие отсеки впервые появляются на этом корабле.
            let opens: Vec<&str> = data
                .compartments
                .iter()
                .filter(|c| c.unlock_hull == tier && c.effect.is_some() && tier > 1)
                .map(|c| c.name.as_str())
                .collect();
            p.spawn((
                Node {
                    padding: UiRect::axes(px(12), px(8)),
                    column_gap: px(10),
                    align_items: AlignItems::Center,
                    border_radius: BorderRadius::all(px(10)),
                    ..default()
                },
                BackgroundColor(if tier == current {
                    Color::srgba(1.0, 0.85, 0.4, 0.16)
                } else {
                    Color::srgba(1.0, 1.0, 1.0, 0.05)
                }),
            ))
            .with_children(|row| {
                row.spawn(Node {
                    flex_grow: 1.0,
                    flex_direction: FlexDirection::Column,
                    row_gap: px(2),
                    ..default()
                })
                .with_children(|info| {
                    info.spawn(heading(
                        fonts,
                        format!("«{}» · {}", class.name, class.class),
                        17.0,
                        if owned { GOLD } else { BRIGHT },
                    ));
                    let mut line = format!("отсеки до ур. {tier}");
                    if !opens.is_empty() {
                        line.push_str(&format!(" · новые: {}", opens.join(", ")));
                    }
                    info.spawn(text(fonts, line, false, 12.0, PARCHMENT));
                });
                if tier == current {
                    row.spawn(heading(fonts, "ваш корабль", 14.0, GOLD));
                } else if owned {
                    row.spawn(text(fonts, "пройден", false, 12.0, MUTED));
                } else if is_next {
                    match &next {
                        Ok(_) => buy_button(row, assets, fonts, class, profile),
                        Err(reason) => {
                            row.spawn(text(fonts, reason.clone(), false, 12.0, MUTED));
                        }
                    }
                } else {
                    row.spawn(text(fonts, "впереди", false, 12.0, MUTED));
                }
            });
        }
    });
}

fn buy_button(
    row: &mut ChildSpawnerCommands,
    assets: &AssetServer,
    fonts: &Fonts,
    class: &crate::ship::ShipClass,
    profile: &Profile,
) {
    let mut button = row.spawn((
        Node {
            padding: UiRect::axes(px(12), px(6)),
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::Center,
            row_gap: px(2),
            ..default()
        },
        Skin(Look::Go),
    ));
    button.with_children(|b| {
        b.spawn(heading(fonts, "Купить", 16.0, Color::WHITE));
        b.spawn(Node {
            column_gap: px(6),
            align_items: AlignItems::Center,
            ..default()
        })
        .with_children(|costs| {
            for (kind, need, have) in [
                ("silver", class.silver, profile.silver),
                ("planks", class.planks, profile.planks),
                ("ether", class.ether, profile.ether),
            ] {
                if need == 0 {
                    continue;
                }
                costs.spawn(icon(assets, kind, 18.0));
                costs.spawn(heading(
                    fonts,
                    crate::style::compact(need),
                    13.0,
                    if have >= need { Color::WHITE } else { LACK },
                ));
            }
        });
    });
    pressable(&mut button);
    button.observe(buy);
}

#[allow(clippy::too_many_arguments)]
fn buy(
    _: On<PointerClick>,
    data: Res<ShipData>,
    screen: Res<BattleScreen>,
    progress: Res<Progress>,
    mut ship: ResMut<Ship>,
    mut profile: ResMut<Profile>,
    mut toast: ResMut<Toast>,
    mut rebuild: ResMut<MenuRebuild>,
    mut sfx: MessageWriter<Sfx>,
) {
    let stages: Vec<String> = screen.data.stages.iter().map(|s| s.id.clone()).collect();
    let next = match ship.next_ship(&data, progress.cleared, &stages) {
        Ok(next) => next,
        Err(reason) => {
            toast.show(&reason);
            return;
        }
    };
    if profile.silver < next.silver || profile.planks < next.planks || profile.ether < next.ether {
        toast.show("Не хватает ресурсов на корабль — добудьте в боях и на конвоях");
        return;
    }
    profile.silver -= next.silver;
    profile.planks -= next.planks;
    profile.ether -= next.ether;
    let (tier, name) = (next.tier, next.name.clone());
    ship.set_hull(tier);
    toast.show_long(&format!(
        "Новый корабль — «{name}»! Отсеки переехали, их потолок теперь ур. {tier}"
    ));
    sfx.write(Sfx("levelup"));
    rebuild.0 = true;
}
