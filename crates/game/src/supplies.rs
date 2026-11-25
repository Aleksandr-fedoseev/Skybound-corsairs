//! Припасы — ограничитель фарма этапов главы (решение #70).
//!
//! Победа на пройденном этапе тратит припасы; поражение и первая победа на новом этапе бесплатны —
//! стену можно штурмовать сколько угодно, а фармить пройденное — нет. Припасы восстанавливаются со временем до потолка, их варит
//! Камбуз (собираются тапом и могут превысить потолок), дают контракты и ивенты.
//! Повторные победы на одном этапе за день приносят всё меньше, а после лимита этап закрыт
//! до завтра. Настройки — `data/supplies.json`.

use bevy::prelude::*;
use serde::Deserialize;

use crate::audio::Sfx;
use crate::menu::{Profile, Progress, Toast, pressable, text};
use crate::ship::{Ship, ShipData};
use crate::view::Fonts;

const GALLEY: Color = Color::srgb(0.72, 0.42, 0.16);

/// Пузырь сваренного Камбузом: тап собирает (сверх потолка можно).
#[derive(Component)]
pub struct GalleyBubble;

#[derive(Resource, Deserialize)]
pub struct SupplyData {
    /// Потолок, до которого припасы восстанавливаются сами.
    pub cap: u32,
    /// Минут на одну единицу восстановления.
    pub regen_minutes: f32,
    /// Цена победы на этапе главы.
    pub cost: u32,
    /// Награда за n-ю победу на этапе за день, в процентах; длина списка — дневной лимит.
    pub replay_percent: Vec<u32>,
    /// Первая победа на новом этапе — без припасов: стену можно штурмовать и с пустым запасом.
    #[serde(default)]
    pub first_clear_free: bool,
    /// Сколько часов Камбуз варит, пока его не опустошат.
    pub galley_hours: f32,
}

impl SupplyData {
    pub fn bundled() -> Self {
        serde_json::from_str(include_str!("../../../data/supplies.json"))
            .expect("data/supplies.json корректен")
    }

    pub fn daily_limit(&self) -> u32 {
        self.replay_percent.len() as u32
    }

    /// Процент награды за следующую победу после `wins` побед сегодня; `None` — лимит.
    pub fn percent(&self, wins: u32) -> Option<u32> {
        self.replay_percent.get(wins as usize).copied()
    }

    /// Можно ли идти на этап: `Err` — почему нельзя.
    /// Сколько припасов стоит победа: новый этап (`first`) — бесплатно, если так в данных.
    pub fn cost_for(&self, first: bool) -> u32 {
        if first && self.first_clear_free {
            0
        } else {
            self.cost
        }
    }

    pub fn check(&self, profile: &Profile, wins: u32, first: bool) -> Result<(), String> {
        if self.percent(wins).is_none() {
            return Err(format!(
                "Этап взят {} раз за сегодня — команда вымоталась. Новый день — в полночь по UTC",
                self.daily_limit()
            ));
        }
        let cost = self.cost_for(first);
        if profile.supplies < cost {
            return Err(format!(
                "Не хватает припасов: нужно {cost}, есть {}. Камбуз, контракты и ивенты пополнят запас",
                profile.supplies
            ));
        }
        Ok(())
    }

    /// Восстановление и Камбуз за `seconds` (в игре и пока игрока не было).
    pub fn pass(&self, profile: &mut Profile, galley_per_hour: u32, seconds: f32) {
        if profile.supplies < self.cap {
            profile.supplies_regen += seconds / (self.regen_minutes * 60.0);
            let whole = profile.supplies_regen.floor();
            profile.supplies_regen -= whole;
            profile.supplies = (profile.supplies + whole as u32).min(self.cap);
        } else {
            profile.supplies_regen = 0.0;
        }
        let store = galley_per_hour as f32 * self.galley_hours;
        profile.supplies_pending =
            (profile.supplies_pending + galley_per_hour as f32 * seconds / 3600.0).min(store);
    }
}

impl Progress {
    /// Побед на этапе `stage` сегодня.
    pub fn wins_today(&self, stage: usize, today: u64) -> u32 {
        if self.day != today {
            return 0;
        }
        self.wins.get(stage).copied().unwrap_or(0)
    }

    pub fn add_win(&mut self, stage: usize, today: u64) {
        if self.day != today {
            self.day = today;
            self.wins.clear();
        }
        if self.wins.len() <= stage {
            self.wins.resize(stage + 1, 0);
        }
        self.wins[stage] += 1;
    }
}

pub fn plugin(app: &mut App) {
    app.insert_resource(SupplyData::bundled())
        .add_systems(Update, (tick, refresh));
}

pub fn galley_bubble(parent: &mut ChildSpawnerCommands, fonts: &Fonts, left: Val, top: Val) {
    let mut bubble = parent.spawn((
        GalleyBubble,
        Node {
            position_type: PositionType::Absolute,
            left,
            top,
            height: px(34),
            padding: UiRect::horizontal(px(14)),
            align_items: AlignItems::Center,
            border: UiRect::all(px(2)),
            border_radius: BorderRadius::all(px(17)),
            ..default()
        },
        BackgroundColor(GALLEY),
        BorderColor::all(Color::WHITE),
        Visibility::Hidden,
    ));
    bubble.with_child(text(fonts, "", true, 14.0, Color::WHITE));
    pressable(&mut bubble);
    bubble.observe(
        |_: On<PointerClick>,
         mut profile: ResMut<Profile>,
         mut toast: ResMut<Toast>,
         mut sfx: MessageWriter<Sfx>| {
            let got = profile.supplies_pending.floor() as u32;
            if got > 0 {
                profile.supplies += got;
                profile.supplies_pending -= got as f32;
                sfx.write(Sfx("coin"));
                toast.show(&format!("Камбуз: +{got} припасов"));
            }
        },
    );
}

fn refresh(
    profile: Res<Profile>,
    mut bubbles: Query<(&mut Visibility, &Children), With<GalleyBubble>>,
    mut texts: Query<&mut Text>,
) {
    let ready = profile.supplies_pending.floor() as u32;
    for (mut vis, children) in &mut bubbles {
        vis.set_if_neq(if ready > 0 {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        });
        for child in children {
            if let Ok(mut text) = texts.get_mut(*child) {
                let value = format!("+{ready} припасов");
                if text.0 != value {
                    text.0 = value;
                }
            }
        }
    }
}

fn tick(
    time: Res<Time>,
    data: Res<SupplyData>,
    ship: Res<Ship>,
    ship_data: Res<ShipData>,
    mut profile: ResMut<Profile>,
) {
    let galley = ship.supplies_per_hour(&ship_data);
    data.pass(&mut profile, galley, time.delta_secs());
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::menu::MenuData;

    fn profile() -> Profile {
        Profile::new(&MenuData::bundled().start)
    }

    #[test]
    fn regen_stops_at_cap() {
        let data = SupplyData::bundled();
        let mut p = profile();
        p.supplies = 0;
        data.pass(&mut p, 0, data.regen_minutes * 60.0 * 3.0 + 1.0);
        assert_eq!(p.supplies, 3);
        data.pass(&mut p, 0, 1.0e7);
        assert_eq!(p.supplies, data.cap);
    }

    #[test]
    fn galley_fills_up_to_its_store() {
        let data = SupplyData::bundled();
        let mut p = profile();
        data.pass(&mut p, 10, 3600.0);
        assert!((p.supplies_pending - 10.0).abs() < 1e-3);
        data.pass(&mut p, 10, 1.0e7);
        assert!((p.supplies_pending - 10.0 * data.galley_hours).abs() < 1e-3);
    }

    #[test]
    fn replays_pay_less_then_stop() {
        let data = SupplyData::bundled();
        let mut progress = Progress::default();
        let p = profile();
        let mut last = u32::MAX;
        for _ in 0..data.daily_limit() {
            let wins = progress.wins_today(0, 7);
            assert!(data.check(&p, wins, false).is_ok());
            let percent = data.percent(wins).unwrap();
            assert!(percent <= last);
            last = percent;
            progress.add_win(0, 7);
        }
        assert!(data.check(&p, progress.wins_today(0, 7), false).is_err());
        // Новый день — снова можно, другой этап не задет.
        assert_eq!(progress.wins_today(0, 8), 0);
        assert_eq!(progress.wins_today(1, 7), 0);
    }

    #[test]
    fn no_supplies_no_voyage() {
        let data = SupplyData::bundled();
        let mut p = profile();
        p.supplies = data.cost - 1;
        assert!(data.check(&p, 0, false).is_err());
        // Новый этап — бесплатно, даже с пустым запасом.
        assert!(data.check(&p, 0, true).is_ok());
    }
}
