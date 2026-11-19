//! Доска контрактов: ежедневные поручения с наградой и бонус за все сразу.
//!
//! Прогресс идёт от дел в игре — сообщения `Deed`: победа, абордаж, уровень героя, сбор
//! эфира, стройка. Новый день (по UTC) — новые поручения. Поручения — `data/contracts.json`.

use std::collections::HashMap;

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::audio::Sfx;
use crate::menu::{
    BRASS, GO, GO_EDGE, GOLD, MUTED, MenuRebuild, PARCHMENT, Spoils, Toast, pressable, text,
};
use crate::save::now;
use crate::view::Fonts;

const SECONDS_PER_DAY: u64 = 86_400;
const DONE: Color = Color::srgb(0.30, 0.62, 0.38);
const DISABLED: Color = Color::srgb(0.36, 0.38, 0.43);

#[derive(Resource, Deserialize)]
pub struct ContractData {
    pub daily: Vec<Contract>,
    pub all_done: Reward,
}

#[derive(Deserialize)]
pub struct Contract {
    pub id: String,
    pub text: String,
    /// Какое дело засчитывается (`Deed`).
    pub deed: String,
    pub need: u32,
    pub reward: Reward,
}

#[derive(Deserialize, Default, Clone, Copy)]
#[serde(default)]
pub struct Reward {
    pub doubloons: u32,
    pub silver: u32,
    pub planks: u32,
    pub journals: u32,
    pub charts: u32,
}

impl Reward {
    fn line(&self) -> String {
        let mut parts = Vec::new();
        for (n, name) in [
            (self.doubloons, "дублонов"),
            (self.silver, "серебра"),
            (self.planks, "досок"),
            (self.journals, "журналов"),
            (self.charts, "лоций"),
        ] {
            if n > 0 {
                parts.push(format!("+{n} {name}"));
            }
        }
        parts.join(" · ")
    }
}

impl ContractData {
    fn bundled() -> Self {
        serde_json::from_str(include_str!("../../../data/contracts.json"))
            .expect("data/contracts.json корректен")
    }
}

/// Дело, которое может засчитаться в контракт.
#[derive(Message, Clone, Copy)]
pub struct Deed(pub &'static str);

/// Прогресс контрактов за день (сохраняется).
#[derive(Resource, Clone, Default, Serialize, Deserialize)]
pub struct Contracts {
    pub day: u64,
    pub counts: HashMap<String, u32>,
    pub claimed: Vec<String>,
    pub bonus_claimed: bool,
}

impl Contracts {
    fn today() -> u64 {
        now() / SECONDS_PER_DAY
    }

    /// Новый день — всё сначала.
    fn roll_day(&mut self) {
        let today = Self::today();
        if self.day != today {
            *self = Self {
                day: today,
                ..default()
            };
        }
    }

    fn done(&self, c: &Contract) -> bool {
        self.counts.get(&c.deed).copied().unwrap_or(0) >= c.need
    }

    /// Есть ли что забрать — для красной точки на кнопке.
    pub fn claimable(&self, data: &ContractData) -> bool {
        let today = self.day == Self::today();
        let any = data
            .daily
            .iter()
            .any(|c| today && self.done(c) && !self.claimed.contains(&c.id));
        let all = today && !self.bonus_claimed && self.claimed.len() >= data.daily.len();
        any || all
    }
}

/// Доска открыта поверх корабля.
#[derive(Resource, Default)]
pub struct BoardOpen(pub bool);

pub fn plugin(app: &mut App) {
    app.insert_resource(ContractData::bundled())
        .init_resource::<Contracts>()
        .init_resource::<BoardOpen>()
        .add_message::<Deed>()
        .add_systems(Update, count_deeds);
}

fn count_deeds(
    mut deeds: MessageReader<Deed>,
    mut contracts: ResMut<Contracts>,
    mut rebuild: ResMut<MenuRebuild>,
) {
    let mut any = false;
    for Deed(kind) in deeds.read() {
        if !any {
            contracts.roll_day();
            any = true;
        }
        *contracts.counts.entry(kind.to_string()).or_insert(0) += 1;
    }
    // Красная точка на «Контрактах» должна загореться сразу.
    if any {
        rebuild.0 = true;
    }
}

/// Доска поверх корабля: поручения, прогресс и «Забрать».
pub fn board(
    root: &mut ChildSpawnerCommands,
    fonts: &Fonts,
    data: &ContractData,
    contracts: &Contracts,
) {
    let today = contracts.day == Contracts::today();
    let count = |deed: &str| {
        if today {
            contracts.counts.get(deed).copied().unwrap_or(0)
        } else {
            0
        }
    };
    let claimed = |id: &str| today && contracts.claimed.iter().any(|c| c == id);

    root.spawn((
        Node {
            position_type: PositionType::Absolute,
            left: px(10),
            top: px(74),
            width: px(520),
            padding: UiRect::all(px(14)),
            flex_direction: FlexDirection::Column,
            row_gap: px(8),
            border: UiRect::all(px(2)),
            border_radius: BorderRadius::all(px(14)),
            ..default()
        },
        BackgroundColor(Color::srgb(0.11, 0.075, 0.05)),
        BorderColor::all(BRASS),
        GlobalZIndex(12),
    ))
    .with_children(|p| {
        p.spawn(Node {
            justify_content: JustifyContent::SpaceBetween,
            align_items: AlignItems::Center,
            ..default()
        })
        .with_children(|row| {
            row.spawn(text(
                fonts,
                "Доска контрактов · сегодня",
                true,
                20.0,
                PARCHMENT,
            ));
            let mut close = row.spawn((
                Node {
                    width: px(44),
                    height: px(36),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    border_radius: BorderRadius::all(px(8)),
                    ..default()
                },
                BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.35)),
            ));
            close.with_child(text(fonts, "x", true, 18.0, MUTED));
            pressable(&mut close);
            close.observe(
                |_: On<PointerClick>,
                 mut open: ResMut<BoardOpen>,
                 mut rebuild: ResMut<MenuRebuild>| {
                    open.0 = false;
                    rebuild.0 = true;
                },
            );
        });
        p.spawn(text(
            fonts,
            "Новые поручения — каждый день в полночь по UTC",
            false,
            12.0,
            MUTED,
        ));

        for c in &data.daily {
            let have = count(&c.deed).min(c.need);
            let state = if claimed(&c.id) {
                Row::Claimed
            } else if have >= c.need {
                Row::Ready
            } else {
                Row::Progress
            };
            row(
                p,
                fonts,
                &c.text,
                &format!("{have}/{} · {}", c.need, c.reward.line()),
                state,
                Claim::One(c.id.clone()),
            );
        }
        let all = data.daily.iter().all(|c| claimed(&c.id));
        let bonus_state = if today && contracts.bonus_claimed {
            Row::Claimed
        } else if all {
            Row::Ready
        } else {
            Row::Progress
        };
        row(
            p,
            fonts,
            "Все поручения дня",
            &data.all_done.line(),
            bonus_state,
            Claim::Bonus,
        );
    });
}

#[derive(Clone, Copy, PartialEq)]
enum Row {
    Progress,
    Ready,
    Claimed,
}

#[derive(Clone)]
enum Claim {
    One(String),
    Bonus,
}

fn row(
    parent: &mut ChildSpawnerCommands,
    fonts: &Fonts,
    title: &str,
    sub: &str,
    state: Row,
    claim: Claim,
) {
    parent
        .spawn((
            Node {
                padding: UiRect::axes(px(12), px(8)),
                column_gap: px(10),
                align_items: AlignItems::Center,
                border: UiRect::all(px(1)),
                border_radius: BorderRadius::all(px(10)),
                ..default()
            },
            BackgroundColor(Color::srgba(1.0, 1.0, 1.0, 0.04)),
            BorderColor::all(if state == Row::Ready {
                GOLD
            } else {
                Color::srgba(1.0, 1.0, 1.0, 0.1)
            }),
        ))
        .with_children(|r| {
            r.spawn(Node {
                flex_grow: 1.0,
                flex_direction: FlexDirection::Column,
                row_gap: px(2),
                ..default()
            })
            .with_children(|col| {
                col.spawn(text(fonts, title, true, 15.0, PARCHMENT));
                col.spawn(text(fonts, sub, false, 12.0, MUTED));
            });
            let (label, fill) = match state {
                Row::Progress => ("в работе", DISABLED),
                Row::Ready => ("Забрать", GO),
                Row::Claimed => ("готово", DONE),
            };
            let mut button = r.spawn((
                Node {
                    width: px(108),
                    height: px(42),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    border: UiRect::all(px(2)),
                    border_radius: BorderRadius::all(px(10)),
                    ..default()
                },
                BackgroundColor(fill),
                BorderColor::all(if state == Row::Ready { GO_EDGE } else { MUTED }),
            ));
            button.with_child(text(fonts, label, true, 14.0, Color::WHITE));
            if state == Row::Ready {
                pressable(&mut button);
                button.observe(move |_: On<PointerClick>, mut commands: Commands| {
                    commands.run_system_cached_with(claim_reward, claim.clone());
                });
            }
        });
}

fn claim_reward(
    In(claim): In<Claim>,
    data: Res<ContractData>,
    mut contracts: ResMut<Contracts>,
    mut spoils: Spoils,
    mut toast: ResMut<Toast>,
    mut rebuild: ResMut<MenuRebuild>,
    mut sfx: MessageWriter<Sfx>,
) {
    contracts.roll_day();
    let reward = match &claim {
        Claim::One(id) => {
            let Some(c) = data.daily.iter().find(|c| &c.id == id) else {
                return;
            };
            if !contracts.done(c) || contracts.claimed.contains(id) {
                return;
            }
            contracts.claimed.push(id.clone());
            c.reward
        }
        Claim::Bonus => {
            if contracts.bonus_claimed || contracts.claimed.len() < data.daily.len() {
                return;
            }
            contracts.bonus_claimed = true;
            data.all_done
        }
    };
    spoils.give(reward.silver, reward.planks, reward.journals);
    spoils.profile.doubloons += reward.doubloons;
    spoils.profile.charts += reward.charts;
    toast.show(&format!("Контракт выполнен: {}", reward.line()));
    sfx.write(Sfx("coin"));
    rebuild.0 = true;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claimable_after_deeds() {
        let data = ContractData::bundled();
        let mut c = Contracts::default();
        c.roll_day();
        assert!(!c.claimable(&data));
        let first = &data.daily[0];
        c.counts.insert(first.deed.clone(), first.need);
        assert!(c.claimable(&data));
        c.claimed.push(first.id.clone());
        assert!(!c.claimable(&data));
    }

    #[test]
    fn new_day_resets() {
        let mut c = Contracts {
            day: 1,
            ..default()
        };
        c.counts.insert("win".into(), 5);
        c.roll_day();
        assert!(c.counts.is_empty());
    }
}
