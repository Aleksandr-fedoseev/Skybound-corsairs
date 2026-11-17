use battle_core::*;

fn ability(name: &str, effects: Vec<(Target, Effect)>) -> Ability {
    Ability {
        name: name.into(),
        effects: effects
            .into_iter()
            .map(|(target, effect)| TargetedEffect { target, effect })
            .collect(),
    }
}

fn hit(multiplier: f32) -> Ability {
    ability(
        "Удар",
        vec![(Target::SingleEnemy, Effect::Damage { multiplier })],
    )
}

fn unit(id: &str, element: Element, hp: u32, atk: u32, spd: u32) -> UnitDef {
    UnitDef {
        id: id.into(),
        name: id.into(),
        element,
        role: None,
        stats: Stats {
            max_hp: hp,
            atk,
            def: 50,
            spd,
        },
        basic: hit(1.0),
        skill: None,
        ultimate: None,
    }
}

/// Мешок с песком: очень медленный и живучий враг, чтобы союзник ходил много раз подряд.
fn sandbag() -> UnitDef {
    unit("sandbag", Element::Steel, 1_000_000, 1, 1)
}

const SHIP: Ship = Ship {
    broadside_power: 100,
};

fn count_turns(events: &[Event], id: UnitId) -> usize {
    events
        .iter()
        .filter(|e| matches!(e, Event::TurnStarted { unit } if *unit == id))
        .count()
}

#[test]
fn faster_unit_acts_twice_as_often() {
    let fast = unit("fast", Element::Wind, 100_000, 1, 200);
    let slow = unit("slow", Element::Wind, 100_000, 1, 100);
    let battle = Battle::new(vec![fast], vec![slow], SHIP, 1);

    let order = battle.turn_order_preview(12);
    let fast_turns = order.iter().filter(|id| **id == UnitId(0)).count();
    assert_eq!(fast_turns, 8, "порядок ходов: {order:?}");

    // Реальный бой совпадает с предпросмотром.
    let mut battle = battle;
    let mut events = vec![Event::TurnStarted {
        unit: battle.current().unwrap(),
    }];
    for _ in 0..11 {
        events.extend(battle.act(Action::basic(None)).unwrap());
    }
    assert_eq!(count_turns(&events, UnitId(0)), 8);
    assert_eq!(count_turns(&events, UnitId(1)), 4);
}

#[test]
fn weakness_increases_damage_by_half() {
    // Ветер сильнее Тумана, против Стали бонуса нет.
    let mut weak_total = 0u64;
    let mut neutral_total = 0u64;
    for seed in 0..500 {
        for (target_element, total) in [
            (Element::Mist, &mut weak_total),
            (Element::Steel, &mut neutral_total),
        ] {
            let attacker = unit("a", Element::Wind, 1000, 100, 200);
            let target = unit("t", target_element, 100_000, 1, 1);
            let mut battle = Battle::new(vec![attacker], vec![target], SHIP, seed);
            let events = battle.act(Action::basic(None)).unwrap();
            let (amount, weakness) = events
                .iter()
                .find_map(|e| match e {
                    Event::Damage {
                        amount,
                        weakness,
                        source: Some(_),
                        ..
                    } => Some((*amount, *weakness)),
                    _ => None,
                })
                .unwrap();
            assert_eq!(weakness, target_element == Element::Mist);
            *total += amount as u64;
        }
    }
    let ratio = weak_total as f64 / neutral_total as f64;
    assert!((1.45..1.55).contains(&ratio), "отношение урона {ratio}");
}

#[test]
fn skill_points_are_spent_and_restored() {
    let mut hero = unit("hero", Element::Wind, 1000, 10, 1000);
    hero.skill = Some(hit(2.0));
    let mut battle = Battle::new(vec![hero], vec![sandbag()], SHIP, 1);

    assert_eq!(battle.skill_points(), SKILL_POINTS_START);
    for _ in 0..SKILL_POINTS_START {
        battle.act(Action::skill(None)).unwrap();
    }
    assert_eq!(battle.skill_points(), 0);
    assert_eq!(
        battle.act(Action::skill(None)),
        Err(BattleError::NotEnoughSkillPoints)
    );

    for _ in 0..10 {
        battle.act(Action::basic(None)).unwrap();
    }
    assert_eq!(
        battle.skill_points(),
        SKILL_POINTS_MAX,
        "очки не превышают максимум"
    );
}

#[test]
fn ultimate_needs_full_energy_and_keeps_the_turn() {
    let mut hero = unit("hero", Element::Wind, 1000, 10, 1000);
    hero.ultimate = Some(hit(5.0));
    let mut battle = Battle::new(vec![hero], vec![sandbag()], SHIP, 1);
    let id = UnitId(0);

    assert_eq!(
        battle.use_ultimate(id, None),
        Err(BattleError::UltimateNotReady)
    );
    for _ in 0..(ENERGY_MAX / ENERGY_BASIC) {
        battle.act(Action::basic(None)).unwrap();
    }
    assert!(battle.unit(id).ultimate_ready());

    let current = battle.current();
    let turn = battle.turn();
    battle.use_ultimate(id, None).unwrap();
    assert_eq!(battle.unit(id).energy, 0);
    assert_eq!(battle.current(), current, "ульта не тратит ход");
    assert_eq!(battle.turn(), turn);
}

#[test]
fn enemies_cannot_use_ultimates() {
    let mut boss = sandbag();
    boss.ultimate = Some(hit(1.0));
    let mut battle = Battle::new(
        vec![unit("hero", Element::Wind, 1000, 10, 100)],
        vec![boss],
        SHIP,
        1,
    );
    assert_eq!(
        battle.use_ultimate(UnitId(1), None),
        Err(BattleError::NotAnAlly)
    );
}

#[test]
fn broadside_fires_after_enough_ally_actions() {
    let hero = unit("hero", Element::Wind, 1000, 1, 1000);
    let mut battle = Battle::new(vec![hero], vec![sandbag()], SHIP, 1);
    let actions_needed = BROADSIDE_MAX.div_ceil(BROADSIDE_PER_ACTION);

    for i in 1..=actions_needed {
        let events = battle.act(Action::basic(None)).unwrap();
        let fired = events.iter().any(|e| matches!(e, Event::BroadsideFired));
        assert_eq!(fired, i == actions_needed, "действие {i}");
        if fired {
            assert!(
                events
                    .iter()
                    .any(|e| matches!(e, Event::Damage { source: None, .. }))
            );
        }
    }
    assert_eq!(battle.broadside(), 0);
}

#[test]
fn delay_pushes_enemy_back_and_advance_pulls_ally_forward() {
    let mut navigator = unit("nav", Element::Wind, 1000, 1, 1000);
    navigator.basic = ability(
        "Манёвр",
        vec![
            (Target::SingleEnemy, Effect::Delay { fraction: 0.5 }),
            (Target::Myself, Effect::Advance { fraction: 0.5 }),
        ],
    );
    let enemy = unit("enemy", Element::Steel, 1000, 1, 100);
    let mut battle = Battle::new(vec![navigator], vec![enemy], SHIP, 1);

    let enemy_before = battle.unit(UnitId(1)).action_value;
    battle.act(Action::basic(None)).unwrap();
    // После хода штурмана всё сдвигается к следующему ходу, поэтому сравниваем разницу между бойцами.
    let nav = battle.unit(UnitId(0));
    let en = battle.unit(UnitId(1));
    assert!(en.action_value - nav.action_value > enemy_before - nav.base_action_value());
}

#[test]
fn healing_is_capped_at_max_hp() {
    let mut medic = unit("medic", Element::Mist, 1000, 10, 1000);
    medic.skill = Some(ability(
        "Перевязка",
        vec![(Target::SingleAlly, Effect::Heal { percent: 0.5 })],
    ));
    let mut battle = Battle::new(vec![medic], vec![sandbag()], SHIP, 1);

    let events = battle.act(Action::skill(Some(UnitId(0)))).unwrap();
    assert!(events.contains(&Event::Healed {
        source: UnitId(0),
        target: UnitId(0),
        amount: 0
    }));
    assert_eq!(battle.unit(UnitId(0)).hp, 1000);
}

#[test]
fn defeated_units_never_take_turns() {
    let hero = unit("hero", Element::Wind, 10_000, 1000, 100);
    let weak = unit("weak", Element::Mist, 1, 1, 50);
    let other = unit("other", Element::Steel, 100_000, 1, 50);
    let mut battle = Battle::new(vec![hero], vec![weak, other], SHIP, 1);

    let mut events = Vec::new();
    for _ in 0..20 {
        events.extend(
            battle
                .act(Action::basic(Some(UnitId(1))))
                .unwrap_or_default(),
        );
    }
    let defeated_at = events
        .iter()
        .position(|e| *e == Event::Defeated { unit: UnitId(1) })
        .expect("слабый враг повержен");
    assert_eq!(count_turns(&events[defeated_at..], UnitId(1)), 0);
}

#[test]
fn victory_ends_the_battle() {
    let hero = unit("hero", Element::Wind, 10_000, 10_000, 100);
    let mut battle = Battle::new(
        vec![hero],
        vec![unit("e", Element::Mist, 10, 1, 50)],
        SHIP,
        1,
    );
    let events = battle.act(Action::basic(None)).unwrap();
    assert!(events.contains(&Event::BattleEnded {
        outcome: Outcome::Victory
    }));
    assert_eq!(battle.outcome(), Some(Outcome::Victory));
    assert_eq!(battle.current(), None);
    assert_eq!(
        battle.act(Action::basic(None)),
        Err(BattleError::BattleOver)
    );
}

#[test]
fn auto_battle_is_deterministic_for_the_same_seed() {
    let team = || {
        vec![
            unit("a", Element::Wind, 1000, 90, 110),
            unit("b", Element::Steel, 1200, 80, 100),
        ]
    };
    let foes = || {
        vec![
            unit("x", Element::Mist, 900, 70, 105),
            unit("y", Element::Powder, 900, 70, 95),
        ]
    };

    let run = |seed| {
        let mut battle = Battle::new(team(), foes(), SHIP, seed);
        let mut log = Vec::new();
        for _ in 0..1000 {
            if battle.outcome().is_some() {
                break;
            }
            log.extend(battle.auto_step());
        }
        (battle.outcome(), log)
    };

    let (outcome_a, log_a) = run(99);
    let (outcome_b, log_b) = run(99);
    assert!(outcome_a.is_some(), "бой должен закончиться");
    assert_eq!(outcome_a, outcome_b);
    assert_eq!(log_a, log_b);
}
