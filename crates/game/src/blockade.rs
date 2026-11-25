//! Ивент «Имперская блокада»: 15 волн подряд, здоровье не восстанавливается, павшие выбывают.
//!
//! После каждой второй волны — выбор одного усиления из трёх на всю оборону (здоровье, атака,
//! лечение, ульты, залп, подлечить, поднять павших, больше добычи). На 5-й, 10-й и 15-й волне —
//! флагман-босс, на некоторых — ветер боя. Уровень врагов растёт с прогрессом главы, как в Руинах.
//! Награда за волну — раз в неделю. Настройки — блок `blockade` в `data/events.json`.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::Screen;
use crate::battle_screen::{Boost, CustomFight, FightKind};
use crate::events::{Bundle, WeekEvents, days_left, week, week_left};
use crate::menu::{GOLD, PARCHMENT, Profile, Voyage, pressable, text};
use crate::story::VoyageRequest;
use crate::style::{self, Look, Skin};
use crate::summary::{Summary, Tone};
use crate::view::Fonts;

#[derive(Deserialize)]
pub struct BlockadeData {
    pub name: String,
    pub level_base: f32,
    /// На столько растёт уровень врагов за каждый пройденный этап главы.
    pub level_per_stage: f32,
    /// Усиление — после каждой такой по счёту волны.
    pub boon_every: usize,
    /// Сколько усилений на выбор.
    pub choices: usize,
    pub waves: Vec<Wave>,
    pub boons: Vec<Boon>,
}

#[derive(Deserialize)]
pub struct Wave {
    pub enemies: Vec<String>,
    /// Поправка уровня волны к общему.
    #[serde(default)]
    pub add: i32,
    pub reward: Bundle,
    #[serde(default)]
    pub wind: Option<battle_core::Element>,
    /// Имя флагмана: волна-босс.
    #[serde(default)]
    pub flagship: Option<String>,
}

/// Усиление на всю оборону. Доли — прибавки; `mend`/`revive` срабатывают сразу при выборе.
#[derive(Deserialize, Default)]
#[serde(default)]
pub struct Boon {
    pub id: String,
    pub name: String,
    pub about: String,
    pub hp: f32,
    pub atk: f32,
    pub spd: f32,
    pub heal: f32,
    pub skill: f32,
    pub broadside: f32,
    /// Множитель накопления ульт (0 — без изменений).
    pub energy: u32,
    /// Сразу подлечить выживших на эту долю.
    pub mend: f32,
    /// Сразу поднять павших с этой долей здоровья.
    pub revive: f32,
    /// Награда за волны больше на эту долю.
    pub plunder: f32,
}

/// Идущая оборона: следующая волна, здоровье героев, взятые усиления и предложенные на выбор.
#[derive(Clone, Default, Serialize, Deserialize)]
pub struct BlockadeRun {
    pub wave: usize,
    pub hp: Vec<(String, f32)>,
    #[serde(default)]
    pub boons: Vec<String>,
    /// Усиления на выбор: пока не выбрано, в бой нельзя.
    #[serde(default)]
    pub offer: Vec<String>,
}

impl BlockadeData {
    pub fn boon(&self, id: &str) -> Option<&Boon> {
        self.boons.iter().find(|b| b.id == id)
    }

    /// Уровень врагов волны при `cleared` пройденных этапах главы.
    pub fn level(&self, cleared: usize, wave: usize) -> u32 {
        let base = self.level_base + self.level_per_stage * cleared as f32;
        (base.round() as i32 + self.waves[wave].add).max(1) as u32
    }

    /// Усиления героев от взятого.
    fn boost(&self, taken: &[String]) -> (Boost, u32) {
        let mut b = Boost::default();
        let mut energy = 1;
        for boon in taken.iter().filter_map(|id| self.boon(id)) {
            b.hp += boon.hp;
            b.atk += boon.atk;
            b.spd += boon.spd;
            b.heal += boon.heal;
            b.skill += boon.skill;
            b.broadside += boon.broadside;
            energy = energy.max(boon.energy);
        }
        (b, energy)
    }

    fn plunder(&self, taken: &[String]) -> f32 {
        1.0 + taken
            .iter()
            .filter_map(|id| self.boon(id))
            .map(|b| b.plunder)
            .sum::<f32>()
    }

    pub fn fight(&self, run: &BlockadeRun, cleared: usize) -> CustomFight {
        let w = &self.waves[run.wave];
        let level = self.level(cleared, run.wave);
        let (boost, energy) = self.boost(&run.boons);
        let title = match &w.flagship {
            Some(name) => format!("{} · волна {} · {name}", self.name, run.wave + 1),
            None => format!(
                "{} · волна {} из {}",
                self.name,
                run.wave + 1,
                self.waves.len()
            ),
        };
        CustomFight {
            title,
            kind: FightKind::Blockade(run.wave),
            enemies: w.enemies.iter().map(|id| (id.clone(), level)).collect(),
            silver: 0,
            planks: 0,
            journals: 0,
            energy_rate: energy,
            ally_hp: run.hp.clone(),
            charged: true,
            boost,
            wind: w.wind,
        }
    }

    /// Три усиления на выбор после волны `wave`: из ещё не взятых, свои на каждой неделе.
    fn offer(&self, week: u64, wave: usize, taken: &[String]) -> Vec<String> {
        let mut pool: Vec<&Boon> = self
            .boons
            .iter()
            .filter(|b| !taken.contains(&b.id))
            .collect();
        let mut rng = battle_core::Rng::new(week * 101 + wave as u64 * 7 + 1);
        let mut out = Vec::new();
        while out.len() < self.choices && !pool.is_empty() {
            let i = (rng.next_u64() % pool.len() as u64) as usize;
            out.push(pool.remove(i).id.clone());
        }
        out
    }
}

impl WeekEvents {
    pub(crate) fn run(&self) -> Option<&BlockadeRun> {
        if self.week == week() {
            self.blockade_run.as_ref()
        } else {
            None
        }
    }

    /// Волна отбита: здоровье — дальше, награда — за новую волну недели, после каждой второй —
    /// выбор усиления.
    pub fn blockade_win(
        &mut self,
        wave: usize,
        hp: Vec<(String, f32)>,
        data: &BlockadeData,
        profile: &mut Profile,
    ) -> Summary {
        self.roll_week();
        let taken = self.run().map(|r| r.boons.clone()).unwrap_or_default();
        let mut sum = Summary::new(format!("Волна {} отбита!", wave + 1)).team(&hp);
        if wave + 1 > self.blockade_best {
            self.blockade_best = wave + 1;
            let reward = data.waves[wave].reward.scaled(data.plunder(&taken));
            reward.put(profile);
            sum = sum.bundle(&reward);
        }
        if wave + 1 >= data.waves.len() {
            self.blockade_run = None;
            return sum.chip("Блокада снята!", Tone::Gold);
        }
        let offer = if (wave + 1).is_multiple_of(data.boon_every) {
            data.offer(self.week, wave, &taken)
        } else {
            Vec::new()
        };
        self.blockade_run = Some(BlockadeRun {
            wave: wave + 1,
            hp,
            boons: taken,
            offer,
        });
        sum
    }

    pub fn blockade_lose(&mut self, wave: usize) -> Summary {
        self.roll_week();
        self.blockade_run = None;
        Summary::new(format!("Блокада прорвана на волне {}", wave + 1)).chip(
            format!("лучший за неделю: {} волн", self.blockade_best),
            Tone::Info,
        )
    }

    /// Взять усиление из предложенных: «сразу»-усиления меняют здоровье отряда.
    pub fn choose_boon(&mut self, id: &str, data: &BlockadeData) {
        let Some(run) = self.blockade_run.as_mut() else {
            return;
        };
        if !run.offer.iter().any(|o| o == id) {
            return;
        }
        if let Some(boon) = data.boon(id) {
            for (_, f) in &mut run.hp {
                if *f > 0.0 {
                    *f = (*f + boon.mend).min(1.0);
                } else if boon.revive > 0.0 {
                    *f = boon.revive;
                }
            }
        }
        run.boons.push(id.to_string());
        run.offer.clear();
    }
}

/// Карточка Блокады в Активностях.
pub fn card(
    col: &mut ChildSpawnerCommands,
    assets: &AssetServer,
    fonts: &Fonts,
    data: &BlockadeData,
    week_events: &WeekEvents,
    cleared: usize,
    team: &[String],
) {
    let best = if week_events.week == week() {
        week_events.blockade_best
    } else {
        0
    };
    let run = week_events.run().cloned();
    let wave = run.as_ref().map_or(0, |r| r.wave);
    let total = data.waves.len();
    let big = if best >= total && run.is_none() {
        "Блокада снята".to_string()
    } else {
        format!("Волна {} из {total}", wave + 1)
    };
    style::banner(
        col,
        assets,
        fonts,
        "event",
        big,
        &[
            (
                format!("осталось {}", days_left(week_left())),
                Color::srgb(0.45, 0.18, 0.14),
            ),
            (format!("лучшее: {best}"), Color::srgb(0.24, 0.2, 0.3)),
        ],
    );
    tile(col).with_children(|card| {
        let hp: Vec<(String, f32)> = match &run {
            Some(r) => r.hp.clone(),
            None => team.iter().map(|id| (id.clone(), 1.0)).collect(),
        };
        crate::ruins::team_hp(card, assets, fonts, &hp);
    });
    // Выбор усиления — вместо следующей волны, пока не выбрано.
    if let Some(r) = run.as_ref().filter(|r| !r.offer.is_empty()) {
        tile(col).with_children(|card| {
            card.spawn(style::heading(fonts, "Выберите усиление", 20.0, GOLD));
            for id in &r.offer {
                if let Some(boon) = data.boon(id) {
                    boon_card(card, fonts, boon);
                }
            }
        });
    } else if let Some(w) = data.waves.get(wave) {
        wave_card(
            col,
            assets,
            fonts,
            data,
            w,
            wave,
            best,
            run.as_ref(),
            cleared,
        );
    }
    // Взятые в этой обороне усиления.
    if let Some(r) = run.as_ref().filter(|r| !r.boons.is_empty()) {
        tile(col).with_children(|card| {
            card.spawn(style::heading(fonts, "Усиления", 18.0, PARCHMENT));
            for b in r.boons.iter().filter_map(|id| data.boon(id)) {
                card.spawn(Node {
                    column_gap: px(10),
                    align_items: AlignItems::Center,
                    ..default()
                })
                .with_children(|row| {
                    style::pill(row, fonts, b.name.clone(), Color::srgb(0.55, 0.4, 0.12));
                    row.spawn(text(
                        fonts,
                        b.about.clone(),
                        false,
                        13.0,
                        Color::srgb(0.8, 0.72, 0.6),
                    ));
                });
            }
        });
    }
}

/// Следующая волна: враги, ветер, флагман, награда и «В бой».
#[allow(clippy::too_many_arguments)]
fn wave_card(
    col: &mut ChildSpawnerCommands,
    assets: &AssetServer,
    fonts: &Fonts,
    data: &BlockadeData,
    w: &Wave,
    wave: usize,
    best: usize,
    run: Option<&BlockadeRun>,
    cleared: usize,
) {
    tile(col).with_children(|card| {
        card.spawn(Node {
            column_gap: px(8),
            align_items: AlignItems::Center,
            ..default()
        })
        .with_children(|row| {
            row.spawn(style::heading(
                fonts,
                format!("Волна {}", wave + 1),
                20.0,
                GOLD,
            ));
            if let Some(name) = &w.flagship {
                style::pill(row, fonts, name.clone(), Color::srgb(0.62, 0.2, 0.16));
            }
        });
        let ids = w.enemies.clone();
        crate::map::enemies_row(card, assets, fonts, &ids, w.wind, PARCHMENT);
        if wave >= best {
            let taken = run.map(|r| r.boons.clone()).unwrap_or_default();
            let reward = w.reward.scaled(data.plunder(&taken));
            style::reward_row(card, assets, fonts, "за волну", &reward.items());
        }
        let label = if run.is_some() {
            format!("В бой: волна {}", wave + 1)
        } else {
            "Начать оборону".to_string()
        };
        go_button(card, fonts, &label, cleared);
    });
}

fn tile<'a>(col: &'a mut ChildSpawnerCommands) -> EntityCommands<'a> {
    col.spawn((
        Node {
            padding: UiRect::axes(px(16), px(14)),
            flex_direction: FlexDirection::Column,
            row_gap: px(10),
            border: UiRect::all(px(2)),
            border_radius: BorderRadius::all(px(14)),
            ..default()
        },
        Skin(Look::Tile),
    ))
}

/// Усиление на выбор: пергамент с именем и описанием, «Взять».
fn boon_card(card: &mut ChildSpawnerCommands, fonts: &Fonts, boon: &Boon) {
    card.spawn((
        Node {
            padding: UiRect::axes(px(14), px(10)),
            column_gap: px(10),
            align_items: AlignItems::Center,
            border_radius: BorderRadius::all(px(12)),
            ..default()
        },
        Skin(Look::Parchment),
        Outline::new(px(2), px(0), Color::srgb(0.72, 0.55, 0.26)),
    ))
    .with_children(|row| {
        row.spawn(Node {
            flex_grow: 1.0,
            flex_shrink: 1.0,
            flex_direction: FlexDirection::Column,
            row_gap: px(2),
            ..default()
        })
        .with_children(|c| {
            c.spawn(style::ink(fonts, boon.name.clone(), 19.0, style::INK));
            c.spawn(text(
                fonts,
                boon.about.clone(),
                false,
                14.0,
                style::INK_SOFT,
            ));
        });
        let id = boon.id.clone();
        let mut take = row.spawn((
            Node {
                width: px(96),
                height: px(46),
                flex_shrink: 0.0,
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            Skin(Look::Go),
        ));
        take.with_child(text(fonts, "Взять", true, 17.0, Color::WHITE));
        pressable(&mut take);
        take.observe(
            move |_: On<PointerClick>,
                  data: Res<crate::events::EventData>,
                  mut week_events: ResMut<WeekEvents>,
                  mut dirty: ResMut<crate::abyss::ActivitiesDirty>| {
                week_events.choose_boon(&id, &data.blockade);
                dirty.0 = true;
            },
        );
    });
}

/// «Начать оборону» (весь отряд, полное здоровье) или «В бой: волна N».
fn go_button(card: &mut ChildSpawnerCommands, fonts: &Fonts, label: &str, cleared: usize) {
    let mut go = card.spawn((
        Node {
            height: px(54),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            ..default()
        },
        Skin(Look::Go),
    ));
    go.with_child(text(fonts, label, true, 19.0, Color::WHITE));
    pressable(&mut go);
    go.observe(
        move |_: On<PointerClick>,
              data: Res<crate::events::EventData>,
              roster: Res<crate::crew::Roster>,
              mut week_events: ResMut<WeekEvents>,
              mut voyage: ResMut<Voyage>,
              mut requests: MessageWriter<VoyageRequest>| {
            week_events.roll_week();
            let run = match week_events.run() {
                Some(r) => r.clone(),
                None => BlockadeRun {
                    wave: 0,
                    hp: roster.team.iter().map(|id| (id.clone(), 1.0)).collect(),
                    ..default()
                },
            };
            week_events.blockade_run = Some(run.clone());
            *voyage = Voyage {
                stage: None,
                custom: Some(data.blockade.fight(&run, cleared)),
                from: Screen::Activities,
            };
            requests.write(VoyageRequest);
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::EventData;
    use crate::menu::MenuData;

    #[test]
    fn hp_carries_boons_offered_and_rewards_once() {
        let data = EventData::bundled().blockade;
        let mut w = WeekEvents::default();
        let mut profile = Profile::new(&MenuData::bundled().start);
        let hp = vec![("ren".to_string(), 0.5), ("mei".to_string(), 0.0)];
        w.blockade_win(0, hp.clone(), &data, &mut profile);
        let run = w.blockade_run.clone().unwrap();
        assert_eq!((run.wave, run.hp.clone()), (1, hp.clone()));
        assert!(run.offer.is_empty(), "после 1-й волны выбора нет");
        // После 2-й — три разных усиления на выбор.
        w.blockade_win(1, hp.clone(), &data, &mut profile);
        let offer = w.blockade_run.clone().unwrap().offer;
        assert_eq!(offer.len(), data.choices);
        // «Второе дыхание» поднимает павших, остальное копится в списке.
        let revive = data
            .boons
            .iter()
            .find(|b| b.revive > 0.0)
            .unwrap()
            .id
            .clone();
        w.blockade_run.as_mut().unwrap().offer = vec![revive.clone()];
        w.choose_boon(&revive, &data);
        let run = w.blockade_run.clone().unwrap();
        assert!(run.offer.is_empty());
        assert!(run.hp[1].1 > 0.0);
        assert_eq!(run.boons, vec![revive]);
        // Повтор волны награды не даёт.
        let silver = profile.silver;
        w.blockade_win(0, hp, &data, &mut profile);
        assert_eq!(profile.silver, silver);
        w.blockade_lose(2);
        assert!(w.blockade_run.is_none());
    }
}
