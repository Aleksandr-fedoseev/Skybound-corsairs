//! Мощь отряда, героя и корабля в игре — формула в `battle_core::power`.
//!
//! Герой считается ровно таким, каким выходит в бой (уровень, звёзды, умения, снаряжение,
//! соседи по Каютам), корабль — по залпу Пушечной палубы. Рекомендуемая мощь боя — мощь
//! стартового отряда без звёзд и снаряжения на рекомендуемом уровне плюс начальный залп:
//! так она совпадает с порогом, по которому подобраны уровни этапов (`--example curve`).

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;

use crate::battle_screen::{BattleScreen, member_unit};
use crate::cabins::{CabinData, Cabins};
use crate::crew::{CrewData, Member, Roster};
use crate::gear::{Gear, GearData};
use crate::ship::{Ship, ShipData};

/// Залп корабля в начале игры — для рекомендуемой мощи.
const BASE_BROADSIDE: u32 = 120;

/// Зелёная плашка — мощи хватает, красная — рано.
pub const ENOUGH: Color = Color::srgb(0.22, 0.5, 0.26);
pub const SHORT: Color = Color::srgb(0.62, 0.2, 0.16);

#[derive(SystemParam)]
pub struct Power<'w> {
    roster: Res<'w, Roster>,
    crew: Res<'w, CrewData>,
    screen: Res<'w, BattleScreen>,
    gear: (Res<'w, Gear>, Res<'w, GearData>),
    cabins: (Res<'w, Cabins>, Res<'w, CabinData>),
    ship: (Res<'w, Ship>, Res<'w, ShipData>),
}

impl Power<'_> {
    fn member(&self, id: &str) -> Member {
        Member {
            id: id.to_string(),
            level: self.roster.level(id),
            stars: self.roster.stars(id),
            skills: self.roster.skills(id),
            atk_bonus: self
                .cabins
                .0
                .atk_bonus(id, &self.roster.team, &self.cabins.1),
            gear: self.gear.0.bonus(id, &self.gear.1),
        }
    }

    /// Мощь героя — таким, каким он выйдет в бой.
    pub fn hero(&self, id: &str) -> u32 {
        battle_core::unit_power(&member_unit(
            &self.screen.data,
            &self.crew,
            &self.member(id),
            0,
        ))
    }

    pub fn ship(&self) -> u32 {
        battle_core::ship_power(self.ship.0.broadside(&self.ship.1))
    }

    /// Мощь отряда в бой вместе с кораблём.
    pub fn team(&self) -> u32 {
        self.roster.team.iter().map(|id| self.hero(id)).sum::<u32>() + self.ship()
    }

    /// Рекомендуемая мощь боя с рекомендуемым уровнем `level`.
    pub fn recommended(&self, level: u32) -> u32 {
        recommended(&self.screen.data, &self.crew, level)
    }
}

/// Мощь стартового отряда (1 звезда, умения 1-го уровня) на уровне `level` плюс начальный залп.
pub fn recommended(data: &game_data::GameData, crew: &CrewData, level: u32) -> u32 {
    let heroes: u32 = crew
        .start_team
        .iter()
        .filter_map(|id| data.hero(id))
        .map(|h| battle_core::unit_power(&crew.hero_unit(h, level, 1, [1, 1, 1])))
        .sum();
    heroes + battle_core::ship_power(BASE_BROADSIDE)
}

/// «12 345» — мощь с пробелом между тысячами.
pub fn format(n: u32) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push(' ');
        }
        out.push(c);
    }
    out
}

/// Значок мощи: скрещённые сабли и число.
pub fn badge(
    parent: &mut ChildSpawnerCommands,
    assets: &AssetServer,
    fonts: &crate::view::Fonts,
    value: u32,
    size: f32,
    color: Color,
) {
    parent
        .spawn((
            Node {
                column_gap: px(3),
                align_items: AlignItems::Center,
                ..default()
            },
            Pickable::IGNORE,
        ))
        .with_children(|row| {
            row.spawn(crate::style::icon(assets, "power", size + 4.0));
            row.spawn(crate::style::heading(fonts, format(value), size, color));
        });
}

/// Плашка «мощь 7 000 · у вас 7 400»: зелёная, если хватает.
pub fn need_chip(
    parent: &mut ChildSpawnerCommands,
    fonts: &crate::view::Fonts,
    need: u32,
    have: u32,
) {
    crate::style::pill(
        parent,
        fonts,
        format!("мощь {} · у вас {}", format(need), format(have)),
        if have >= need { ENOUGH } else { SHORT },
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thousands_split() {
        assert_eq!(format(7), "7");
        assert_eq!(format(1234), "1 234");
        assert_eq!(format(1234567), "1 234 567");
    }

    #[test]
    fn recommended_grows_with_level() {
        let data = game_data::GameData::bundled().unwrap();
        let crew = CrewData::bundled();
        assert!(recommended(&data, &crew, 10) > recommended(&data, &crew, 5));
    }
}
