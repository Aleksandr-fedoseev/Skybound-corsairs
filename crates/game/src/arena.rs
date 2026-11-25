//! Арена капитанов — бои с отрядами других капитанов под управлением ИИ.
//!
//! Сервера пока нет, поэтому соперников собираем сами: имя капитана, корабль и четыре героя
//! из тех же, что у игрока, на уровне отряда игрока (лёгкий −3, равный, сильный +3). Список
//! свой на каждой неделе и после каждого боя. Победа — очки рейтинга и (первые 10 за день) серебро
//! с журналами; лига в конце недели — дублоны, половина очков переходит на новую неделю.
//! Когда появится сервер, соперниками станут защиты настоящих игроков. Настройки — `data/arena.json`.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::Screen;
use crate::battle_screen::{CustomFight, FightKind};
use crate::events::{Bundle, days_left, week, week_left};
use crate::menu::{PARCHMENT, Profile, Toast, Voyage, pressable, text};
use crate::story::VoyageRequest;
use crate::style::{self, Look, Skin};
use crate::summary::{Summary, Tone};
use crate::view::Fonts;

#[derive(Resource, Deserialize)]
pub struct ArenaData {
    pub name: String,
    pub unlock_cleared: usize,
    pub captains: Vec<String>,
    pub ships: Vec<String>,
    /// Соперники по силе: поправка к уровню отряда игрока и очки за победу.
    pub rivals: Vec<RivalTier>,
    pub team_size: usize,
    pub loss_points: u32,
    pub daily_rewarded_wins: u32,
    pub win_reward: Bundle,
    pub refresh_silver: u32,
    /// Какая доля очков переходит на новую неделю.
    pub carry_over: f32,
    pub leagues: Vec<League>,
}

#[derive(Deserialize)]
pub struct RivalTier {
    pub name: String,
    pub level: i32,
    pub points: u32,
}

#[derive(Deserialize)]
pub struct League {
    pub name: String,
    pub from: u32,
    pub doubloons: u32,
}

/// Соперник в списке.
pub struct Rival {
    pub captain: String,
    pub ship: String,
    pub team: Vec<(String, u32)>,
}

impl ArenaData {
    pub fn bundled() -> Self {
        serde_json::from_str(include_str!("../../../data/arena.json"))
            .expect("data/arena.json корректен")
    }

    pub fn league(&self, rating: u32) -> usize {
        self.leagues
            .iter()
            .rposition(|l| rating >= l.from)
            .unwrap_or(0)
    }

    /// Соперники недели `week` после `refresh` смен списка; `level` — средний уровень отряда игрока.
    /// Один на каждую ступень силы, по порядку: лёгкий, равный, сильный.
    pub fn rivals(&self, week: u64, refresh: u32, heroes: &[String], level: u32) -> Vec<Rival> {
        let mut rng = battle_core::Rng::new(week * 1_000_003 + refresh as u64 * 7919 + 17);
        let mut pick = |n: usize| (rng.next_u64() % n as u64) as usize;
        // Капитаны и корабли в одном списке не повторяются.
        let mut captains = self.captains.clone();
        let mut ships = self.ships.clone();
        self.rivals
            .iter()
            .map(|tier| {
                let mut pool = heroes.to_vec();
                let mut team = Vec::new();
                while team.len() < self.team_size && !pool.is_empty() {
                    let id = pool.remove(pick(pool.len()));
                    team.push((id, (level as i32 + tier.level).max(1) as u32));
                }
                Rival {
                    captain: captains.remove(pick(captains.len())),
                    ship: ships.remove(pick(ships.len())),
                    team,
                }
            })
            .collect()
    }

    pub fn fight(&self, tier: usize, rival: &Rival) -> CustomFight {
        CustomFight {
            title: format!("Арена · {} · {}", rival.captain, self.rivals[tier].name),
            kind: FightKind::Arena(tier),
            enemies: rival.team.clone(),
            silver: 0,
            planks: 0,
            journals: 0,
            energy_rate: 1,
            ally_hp: Vec::new(),
            charged: false,
            boost: Default::default(),
            wind: None,
        }
    }
}

/// Рейтинг Арены (сохраняется).
#[derive(Resource, Clone, Default, Serialize, Deserialize)]
pub struct Arena {
    pub week: u64,
    pub rating: u32,
    /// Сколько раз за неделю менялся список соперников (номер списка).
    pub refresh: u32,
    pub day: u64,
    pub wins_today: u32,
    /// Дублоны за лигу прошлой недели — ждут, когда их заберут.
    pub pending: u32,
    pub pending_league: String,
}

impl Arena {
    /// Новая неделя: награда за лигу, часть очков переходит. Новый день: счётчик побед с наградой.
    pub fn roll(&mut self, data: &ArenaData, today: u64) {
        let w = week();
        if self.week != w {
            if self.week != 0 && self.rating > 0 {
                let league = &data.leagues[data.league(self.rating)];
                self.pending += league.doubloons;
                self.pending_league = league.name.clone();
            }
            self.rating = (self.rating as f32 * data.carry_over).round() as u32;
            self.week = w;
            self.refresh = 0;
        }
        if self.day != today {
            self.day = today;
            self.wins_today = 0;
        }
    }

    /// Итог боя: очки, награда за победу (если дневной лимит не выбран), новый список соперников.
    pub fn finish(
        &mut self,
        win: bool,
        tier: usize,
        data: &ArenaData,
        today: u64,
        profile: &mut Profile,
    ) -> Summary {
        self.roll(data, today);
        self.refresh += 1;
        let before = data.league(self.rating);
        let mut sum = if win {
            let points = data.rivals[tier].points;
            self.rating += points;
            let mut sum =
                Summary::new("Победа на Арене!").chip(format!("+{points} очков"), Tone::Good);
            if self.wins_today < data.daily_rewarded_wins {
                self.wins_today += 1;
                data.win_reward.put(profile);
                sum = sum.bundle(&data.win_reward);
            }
            sum
        } else {
            self.rating = self.rating.saturating_sub(data.loss_points);
            Summary::new("Поражение на Арене")
                .chip(format!("−{} очков", data.loss_points), Tone::Bad)
        };
        let after = data.league(self.rating);
        sum = sum.chip(format!("рейтинг {}", self.rating), Tone::Info);
        if after > before {
            sum = sum.chip(
                format!("Новая лига: {}!", data.leagues[after].name),
                Tone::Gold,
            );
        } else {
            sum = sum.chip(format!("лига {}", data.leagues[after].name), Tone::Info);
        }
        if win && self.wins_today >= data.daily_rewarded_wins {
            sum = sum.chip(
                format!("побед с наградой: {0}/{0}", data.daily_rewarded_wins),
                Tone::Info,
            );
        }
        sum
    }
}

pub fn plugin(app: &mut App) {
    app.insert_resource(ArenaData::bundled())
        .init_resource::<Arena>();
}

/// Средний уровень отряда — по нему подбираются соперники.
pub fn team_level(roster: &crate::crew::Roster) -> u32 {
    let members = roster.team_members();
    if members.is_empty() {
        return 1;
    }
    (members.iter().map(|m| m.level).sum::<u32>() as f32 / members.len() as f32).round() as u32
}

/// Раздел Арены в Активностях.
#[allow(clippy::too_many_arguments)]
pub fn section(
    col: &mut ChildSpawnerCommands,
    assets: &AssetServer,
    fonts: &Fonts,
    data: &ArenaData,
    arena: &Arena,
    heroes: &[String],
    level: u32,
    cleared: usize,
    power: (&game_data::GameData, u32),
) {
    let (game, mine) = power;
    // Показываем так, будто неделя и день уже перевёрнуты: сохраняемое меняют только кнопки.
    let mut view = arena.clone();
    view.roll(data, crate::cabins::today());
    let league = data.league(view.rating);
    tile(col).with_children(|card| {
        // Лига и шкала до следующей (какие лиги бывают — в справке «?»).
        card.spawn(Node {
            column_gap: px(10),
            align_items: AlignItems::Center,
            ..default()
        })
        .with_children(|row| {
            let colors = [
                Color::srgb(0.55, 0.36, 0.2),
                Color::srgb(0.5, 0.52, 0.56),
                Color::srgb(0.78, 0.58, 0.18),
                Color::srgb(0.42, 0.3, 0.75),
            ];
            style::pill(
                row,
                fonts,
                data.leagues[league].name.clone(),
                colors[league.min(colors.len() - 1)],
            );
            let frac = match data.leagues.get(league + 1) {
                Some(next) => {
                    let from = data.leagues[league].from;
                    (view.rating - from) as f32 / (next.from - from).max(1) as f32
                }
                None => 1.0,
            };
            row.spawn(Node {
                flex_grow: 1.0,
                flex_direction: FlexDirection::Column,
                ..default()
            })
            .with_children(|bar| {
                style::meter(bar, fonts, frac, "", Color::srgb(0.55, 0.42, 0.18));
            });
        });
        card.spawn(Node {
            column_gap: px(8),
            row_gap: px(6),
            flex_wrap: FlexWrap::Wrap,
            align_items: AlignItems::Center,
            ..default()
        })
        .with_children(|row| {
            let league_prize = [("doubloons", data.leagues[league].doubloons)];
            style::reward_row(row, assets, fonts, "лига", &league_prize);
            style::reward_row(
                row,
                assets,
                fonts,
                &format!("победа {}/{}", view.wins_today, data.daily_rewarded_wins),
                &data.win_reward.items(),
            );
            style::pill(
                row,
                fonts,
                format!("осталось {}", days_left(week_left())),
                Color::srgb(0.45, 0.18, 0.14),
            );
        });
        if view.pending > 0 {
            claim_button(
                card,
                fonts,
                &format!(
                    "Забрать {} дублонов за лигу {}",
                    view.pending, view.pending_league
                ),
            );
        }
    });
    if cleared < data.unlock_cleared {
        tile(col).with_children(|card| {
            card.spawn(text(
                fonts,
                "Арена откроется после главы 1: пройди Грозового левиафана",
                true,
                16.0,
                PARCHMENT,
            ));
        });
        return;
    }
    let rivals = data.rivals(view.week, view.refresh, heroes, level);
    for (tier, rival) in rivals.iter().enumerate() {
        let t = &data.rivals[tier];
        col.spawn((
            Node {
                padding: UiRect::axes(px(14), px(12)),
                flex_direction: FlexDirection::Column,
                row_gap: px(6),
                border_radius: BorderRadius::all(px(12)),
                ..default()
            },
            Skin(Look::Parchment),
            Outline::new(px(2), px(0), Color::srgb(0.72, 0.55, 0.26)),
        ))
        .with_children(|card| {
            card.spawn(Node {
                justify_content: JustifyContent::SpaceBetween,
                align_items: AlignItems::Center,
                column_gap: px(6),
                ..default()
            })
            .with_children(|row| {
                row.spawn(Node {
                    flex_direction: FlexDirection::Column,
                    flex_shrink: 1.0,
                    ..default()
                })
                .with_children(|c| {
                    c.spawn(style::ink(fonts, rival.captain.clone(), 18.0, style::INK));
                    c.spawn(Node {
                        column_gap: px(8),
                        align_items: AlignItems::Center,
                        ..default()
                    })
                    .with_children(|line| {
                        line.spawn(text(
                            fonts,
                            rival.ship.clone(),
                            false,
                            13.0,
                            style::INK_SOFT,
                        ));
                        // Мощь соперника: красная — сильнее нашего отряда.
                        let theirs: u32 = rival
                            .team
                            .iter()
                            .filter_map(|(id, lvl)| {
                                game.hero(id)
                                    .map(|h| battle_core::unit_power(&h.to_unit(*lvl)))
                            })
                            .sum();
                        let color = if theirs > mine {
                            crate::power::SHORT
                        } else {
                            crate::power::ENOUGH
                        };
                        crate::power::badge(line, assets, fonts, theirs, 15.0, color);
                    });
                });
                let bg = [
                    Color::srgb(0.25, 0.5, 0.28),
                    Color::srgb(0.55, 0.42, 0.15),
                    Color::srgb(0.6, 0.2, 0.15),
                ][tier.min(2)];
                row.spawn((
                    Node {
                        padding: UiRect::axes(px(10), px(4)),
                        border_radius: BorderRadius::all(px(12)),
                        flex_shrink: 0.0,
                        ..default()
                    },
                    BackgroundColor(bg),
                ))
                .with_child(style::heading(
                    fonts,
                    format!("{} · +{} очков", t.name, t.points),
                    14.0,
                    Color::WHITE,
                ));
            });
            card.spawn(Node {
                column_gap: px(8),
                align_items: AlignItems::Center,
                ..default()
            })
            .with_children(|row| {
                for (id, lvl) in &rival.team {
                    row.spawn(Node {
                        flex_direction: FlexDirection::Column,
                        align_items: AlignItems::Center,
                        ..default()
                    })
                    .with_children(|c| {
                        c.spawn(style::medallion(assets, id, 62.0, false));
                        c.spawn(style::heading(
                            fonts,
                            format!("ур. {lvl}"),
                            13.0,
                            style::INK,
                        ));
                    });
                }
                row.spawn(Node {
                    flex_grow: 1.0,
                    ..default()
                });
                fight_button(row, fonts, tier);
            });
        });
    }
    refresh_button(col, fonts, data.refresh_silver);
}

fn tile<'a>(col: &'a mut ChildSpawnerCommands) -> EntityCommands<'a> {
    col.spawn((
        Node {
            padding: UiRect::axes(px(16), px(14)),
            flex_direction: FlexDirection::Column,
            row_gap: px(8),
            border: UiRect::all(px(2)),
            border_radius: BorderRadius::all(px(14)),
            ..default()
        },
        Skin(Look::Tile),
    ))
}

fn button<'a>(
    parent: &'a mut ChildSpawnerCommands,
    fonts: &Fonts,
    label: &str,
    look: Look,
) -> EntityCommands<'a> {
    let mut b = parent.spawn((
        Node {
            height: px(50),
            padding: UiRect::horizontal(px(16)),
            flex_shrink: 0.0,
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            border: UiRect::all(px(2)),
            border_radius: BorderRadius::all(px(12)),
            ..default()
        },
        Skin(look),
    ));
    b.with_child(text(fonts, label, true, 17.0, Color::WHITE));
    pressable(&mut b);
    b
}

fn fight_button(row: &mut ChildSpawnerCommands, fonts: &Fonts, tier: usize) {
    button(row, fonts, "В бой", Look::Go).observe(
        move |_: On<PointerClick>,
              data: Res<ArenaData>,
              mut arena: ResMut<Arena>,
              roster: Res<crate::crew::Roster>,
              screen: Res<crate::battle_screen::BattleScreen>,
              mut voyage: ResMut<Voyage>,
              mut requests: MessageWriter<VoyageRequest>| {
            arena.roll(&data, crate::cabins::today());
            let heroes: Vec<String> = screen.data.heroes.iter().map(|h| h.id.clone()).collect();
            let rivals = data.rivals(arena.week, arena.refresh, &heroes, team_level(&roster));
            *voyage = Voyage {
                stage: None,
                custom: Some(data.fight(tier, &rivals[tier])),
                from: Screen::Activities,
            };
            requests.write(VoyageRequest);
        },
    );
}

fn refresh_button(col: &mut ChildSpawnerCommands, fonts: &Fonts, price: u32) {
    button(
        col,
        fonts,
        &format!("Сменить соперников · {price} серебра"),
        Look::Dark,
    )
    .observe(
        move |_: On<PointerClick>,
              data: Res<ArenaData>,
              mut arena: ResMut<Arena>,
              mut profile: ResMut<Profile>,
              mut toast: ResMut<Toast>,
              mut dirty: ResMut<crate::abyss::ActivitiesDirty>| {
            if profile.silver < price {
                toast.show(&format!("Нужно {price} серебра"));
                return;
            }
            profile.silver -= price;
            arena.roll(&data, crate::cabins::today());
            arena.refresh += 1;
            dirty.0 = true;
        },
    );
}

fn claim_button(card: &mut ChildSpawnerCommands, fonts: &Fonts, label: &str) {
    button(card, fonts, label, Look::Go).observe(
        |_: On<PointerClick>,
         data: Res<ArenaData>,
         mut arena: ResMut<Arena>,
         mut profile: ResMut<Profile>,
         mut toast: ResMut<Toast>,
         mut dirty: ResMut<crate::abyss::ActivitiesDirty>| {
            arena.roll(&data, crate::cabins::today());
            profile.doubloons += arena.pending;
            toast.show(&format!("+{} дублонов", arena.pending));
            arena.pending = 0;
            dirty.0 = true;
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::menu::MenuData;

    fn heroes() -> Vec<String> {
        ["borg", "ren", "kaya", "mei", "lira", "sora", "tess"]
            .map(String::from)
            .to_vec()
    }

    #[test]
    fn rivals_are_stable_and_scale_with_tier() {
        let data = ArenaData::bundled();
        let a = data.rivals(5, 2, &heroes(), 20);
        let b = data.rivals(5, 2, &heroes(), 20);
        assert_eq!(a.len(), data.rivals.len());
        for (x, y) in a.iter().zip(&b) {
            assert_eq!(x.team, y.team);
            assert_eq!(x.team.len(), data.team_size);
        }
        assert!(a[0].team[0].1 < a[2].team[0].1);
        assert_ne!(a[0].captain, a[1].captain);
        assert_ne!(a[1].captain, a[2].captain);
        // После боя — другой список.
        let c = data.rivals(5, 3, &heroes(), 20);
        assert!(
            a.iter()
                .zip(&c)
                .any(|(x, y)| x.team != y.team || x.captain != y.captain)
        );
    }

    #[test]
    fn rating_daily_cap_and_weekly_league() {
        let data = ArenaData::bundled();
        let mut arena = Arena::default();
        let mut p = Profile::new(&MenuData::bundled().start);
        for _ in 0..data.daily_rewarded_wins + 3 {
            arena.finish(true, 2, &data, 100, &mut p);
        }
        assert_eq!(arena.wins_today, data.daily_rewarded_wins);
        let rating = arena.rating;
        assert_eq!(
            rating,
            data.rivals[2].points * (data.daily_rewarded_wins + 3)
        );
        arena.finish(false, 0, &data, 100, &mut p);
        assert_eq!(arena.rating, rating - data.loss_points);
        // Новый день — награды снова идут.
        arena.roll(&data, 101);
        assert_eq!(arena.wins_today, 0);
        // Новая неделя — дублоны за лигу ждут, очки урезаны.
        let before = arena.rating;
        arena.week -= 1;
        arena.roll(&data, 101);
        assert_eq!(arena.pending, data.leagues[data.league(before)].doubloons);
        assert!(arena.rating < before);
    }
}
