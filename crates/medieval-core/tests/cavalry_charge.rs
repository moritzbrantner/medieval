use medieval_core::{
    BattlePoint, BattleSide, CavalryChargeState as Charge, Facing, FlatBattlefield, Formation,
    MovementOrder, TacticalBattle, TacticalUnit, UnitCombatProfile, UnitKind,
};

fn battle(distance: u32, defender: UnitKind, facing: Facing, reverse: bool) -> TacticalBattle {
    let mut units = vec![
        TacticalUnit::new(
            "a",
            BattleSide::Attacker,
            320,
            BattlePoint::new(10_000, 10_000),
            Formation::Line { files: 80 },
            100,
        )
        .with_combat_stats(UnitCombatProfile::v1(UnitKind::Knights)),
        TacticalUnit::new(
            "d",
            BattleSide::Defender,
            320,
            BattlePoint::new(10_000 + distance, 10_000),
            Formation::Line { files: 80 },
            100,
        )
        .with_combat_stats(UnitCombatProfile::v1(defender))
        .with_facing(facing),
    ];
    if reverse {
        units.reverse();
    }
    let mut battle = TacticalBattle::new(FlatBattlefield::new(100_000, 100_000), units).unwrap();
    battle.issue_engagement_order("a", "d").unwrap();
    battle
}

fn charge(battle: &TacticalBattle) -> Charge {
    battle
        .units()
        .iter()
        .find(|unit| unit.id() == "a")
        .unwrap()
        .charge_state()
        .unwrap()
}
fn survivors(battle: &TacticalBattle) -> u16 {
    battle
        .units()
        .iter()
        .find(|unit| unit.id() == "d")
        .unwrap()
        .soldiers()
}
fn without_charge(battle: &TacticalBattle) -> TacticalBattle {
    let mut value = serde_json::to_value(battle).unwrap();
    value["units"][0].as_object_mut().unwrap().remove("charge");
    serde_json::from_value(value).unwrap()
}
fn contact(mut battle: TacticalBattle) -> TacticalBattle {
    for _ in 0..100 {
        battle.advance_ticks(1);
        if matches!(charge(&battle), Charge::Contact { .. }) {
            return battle;
        }
    }
    panic!("formation did not reach contact");
}
fn resolve_next_pulse(battle: &mut TacticalBattle) {
    let remaining = 20 - battle.tick() % 20;
    battle.advance_ticks(u32::try_from(remaining).unwrap());
}

#[test]
fn full_run_up_arms_bounded_impact_and_consumes_it_once() {
    let mut moving = battle(8_000, UnitKind::Levy, Facing::west(), false);
    moving.advance_ticks(20);
    assert!(matches!(charge(&moving), Charge::Approaching { .. }));
    moving.advance_ticks(4);
    assert_eq!(charge(&moving), Charge::Charging { run_up_mm: 4_000 });
    let mut charged = contact(moving);
    assert_eq!(charge(&charged), Charge::Contact { run_up_mm: 4_000 });
    let mut neutral = without_charge(&charged);
    let before = survivors(&charged);
    resolve_next_pulse(&mut charged);
    resolve_next_pulse(&mut neutral);
    let impact = before - survivors(&charged);
    let ordinary = before - survivors(&neutral);
    assert!(impact > ordinary);
    assert!(impact <= ordinary * 2);
    assert_eq!(
        charge(&charged),
        Charge::Recovering {
            ticks_remaining: 40
        }
    );
    let before_charged = survivors(&charged);
    let before_neutral = survivors(&neutral);
    charged.advance_ticks(20);
    neutral.advance_ticks(20);
    assert!(
        (before_charged - survivors(&charged)).abs_diff(before_neutral - survivors(&neutral)) <= 1
    );
}

#[test]
fn short_approach_has_no_full_impact_bonus() {
    let mut short = contact(battle(3_000, UnitKind::Levy, Facing::west(), false));
    assert!(matches!(charge(&short), Charge::Contact { run_up_mm } if run_up_mm < 4_000));
    let mut neutral = without_charge(&short);
    resolve_next_pulse(&mut short);
    resolve_next_pulse(&mut neutral);
    assert_eq!(survivors(&short), survivors(&neutral));
}

#[test]
fn halt_and_retarget_interrupt_momentum_without_order_spam_resetting_cooldown() {
    let mut interrupted = battle(8_000, UnitKind::Levy, Facing::west(), false);
    interrupted.advance_ticks(24);
    assert!(matches!(charge(&interrupted), Charge::Charging { .. }));
    let position = interrupted.units()[0].position();
    interrupted
        .issue_move_order(MovementOrder {
            unit_id: "a".into(),
            destination: position,
        })
        .unwrap();
    assert_eq!(
        charge(&interrupted),
        Charge::Recovering {
            ticks_remaining: 40
        }
    );
    interrupted.issue_engagement_order("a", "d").unwrap();
    interrupted.advance_ticks(4);
    let state = charge(&interrupted);
    interrupted.issue_engagement_order("a", "d").unwrap();
    assert_eq!(charge(&interrupted), state);
    let mut neutral = without_charge(&interrupted);
    interrupted.advance_ticks(12);
    neutral.advance_ticks(12);
    assert_eq!(survivors(&interrupted), survivors(&neutral));
    assert!(matches!(charge(&interrupted), Charge::Recovering { .. }));
}

#[test]
fn frontal_spears_counter_charge_while_flank_contact_receives_impact() {
    for (facing, bonus) in [(Facing::west(), false), (Facing::new(0, 1).unwrap(), true)] {
        let mut charged = contact(battle(8_000, UnitKind::Spearmen, facing, false));
        let mut neutral = without_charge(&charged);
        resolve_next_pulse(&mut charged);
        resolve_next_pulse(&mut neutral);
        if bonus {
            assert!(survivors(&charged) < survivors(&neutral));
        } else {
            assert_eq!(survivors(&charged), survivors(&neutral));
        }
    }
}

#[test]
fn charge_states_replay_across_serialization_storage_order_and_tick_partition() {
    for (distance, kind, facing) in [
        (8_000, UnitKind::Levy, Facing::west()),
        (3_000, UnitKind::Levy, Facing::west()),
        (8_000, UnitKind::Spearmen, Facing::west()),
        (8_000, UnitKind::Spearmen, Facing::new(0, 1).unwrap()),
    ] {
        let mut direct = battle(distance, kind, facing, false);
        let mut replay = battle(distance, kind, facing, true);
        direct.advance_ticks(24);
        replay.advance_ticks(12);
        let checkpoint = serde_json::to_string(&replay).unwrap();
        replay = serde_json::from_str(&checkpoint).unwrap();
        replay.advance_ticks(12);
        assert_eq!(direct, replay);
        direct.advance_ticks(96);
        for _ in 0..12 {
            replay.advance_ticks(8);
        }
        assert_eq!(direct, replay);
    }
}

#[test]
fn turning_or_changing_target_interrupts_charge_but_invalid_orders_are_atomic() {
    let mut active = battle(8_000, UnitKind::Levy, Facing::west(), false);
    active.advance_ticks(24);
    let before = active.clone();
    assert!(active.issue_engagement_order("a", "missing").is_err());
    assert_eq!(active, before);
    active
        .issue_facing_order("a", Facing::new(0, 1).unwrap())
        .unwrap();
    assert_eq!(
        charge(&active),
        Charge::Recovering {
            ticks_remaining: 40
        }
    );
    let mut value = serde_json::to_value(before).unwrap();
    let mut extra = value["units"][1].clone();
    extra["id"] = serde_json::json!("other");
    extra["position"]["yMm"] = serde_json::json!(20_000);
    value["units"].as_array_mut().unwrap().push(extra);
    let mut retarget: TacticalBattle = serde_json::from_value(value).unwrap();
    retarget.issue_engagement_order("a", "other").unwrap();
    assert_eq!(
        charge(&retarget),
        Charge::Recovering {
            ticks_remaining: 40
        }
    );
}

#[test]
fn historical_profiled_knights_without_charge_state_keep_previous_melee() {
    let mut historical = without_charge(&battle(8_000, UnitKind::Levy, Facing::west(), false));
    historical.advance_ticks(40);
    assert!(historical.units()[0].charge_state().is_none());
    let value = serde_json::to_string(&historical).unwrap();
    let mut replay: TacticalBattle = serde_json::from_str(&value).unwrap();
    historical.advance_ticks(60);
    replay.advance_ticks(60);
    assert_eq!(historical, replay);
}

#[test]
fn forest_entry_and_intercepting_formations_interrupt_run_up() {
    let mut value =
        serde_json::to_value(battle(8_000, UnitKind::Levy, Facing::west(), false)).unwrap();
    value["units"][0]["position"] = serde_json::json!({"xMm":22_000,"yMm":15_000});
    value["units"][1]["position"] = serde_json::json!({"xMm":30_000,"yMm":15_000});
    let mut forest: TacticalBattle = serde_json::from_value(value).unwrap();
    forest.advance_ticks(10);
    assert!(matches!(charge(&forest), Charge::Approaching { .. }));
    forest.advance_ticks(20);
    assert!(matches!(charge(&forest), Charge::Recovering { .. }));

    let mut active = battle(8_000, UnitKind::Levy, Facing::west(), false);
    active.advance_ticks(24);
    let mut value = serde_json::to_value(active).unwrap();
    let mut interceptor = value["units"][1].clone();
    interceptor["id"] = serde_json::json!("interceptor");
    interceptor["position"] = value["units"][0]["position"].clone();
    interceptor["position"]["xMm"] = serde_json::json!(15_000);
    value["units"].as_array_mut().unwrap().push(interceptor);
    let mut intercepted: TacticalBattle = serde_json::from_value(value).unwrap();
    intercepted.advance_ticks(1);
    assert_eq!(
        charge(&intercepted),
        Charge::Recovering {
            ticks_remaining: 40
        }
    );
}

#[test]
fn final_contact_displacement_completes_a_threshold_charge() {
    let mut charged = contact(battle(5_500, UnitKind::Levy, Facing::west(), false));
    assert_eq!(charge(&charged), Charge::Contact { run_up_mm: 4_000 });
    let mut neutral = without_charge(&charged);
    resolve_next_pulse(&mut charged);
    resolve_next_pulse(&mut neutral);
    assert!(survivors(&charged) < survivors(&neutral));
}

#[test]
fn moving_target_cannot_consume_impact_before_actual_melee_contact() {
    let mut moving = battle(4_575, UnitKind::Levy, Facing::west(), false);
    moving
        .issue_move_order(MovementOrder {
            unit_id: "d".into(),
            destination: BattlePoint::new(25_000, 10_000),
        })
        .unwrap();
    moving.advance_ticks(40);
    assert_eq!(
        moving.units()[1].position().x_mm - moving.units()[0].position().x_mm,
        1_575
    );
    assert_eq!(charge(&moving), Charge::Charging { run_up_mm: 4_000 });
    assert_eq!(survivors(&moving), 320);
    let checkpoint = serde_json::to_string(&moving).unwrap();
    let mut replay: TacticalBattle = serde_json::from_str(&checkpoint).unwrap();
    for battle in [&mut moving, &mut replay] {
        let position = battle.units()[1].position();
        battle
            .issue_move_order(MovementOrder {
                unit_id: "d".into(),
                destination: position,
            })
            .unwrap();
    }
    let mut neutral = without_charge(&moving);
    moving.advance_ticks(20);
    replay.advance_ticks(10);
    replay.advance_ticks(10);
    neutral.advance_ticks(20);
    assert_eq!(moving, replay);
    assert!(survivors(&moving) < survivors(&neutral));
    assert_eq!(
        charge(&moving),
        Charge::Recovering {
            ticks_remaining: 40
        }
    );
}

#[test]
fn closing_target_receives_impact_on_the_first_actual_contact_pulse() {
    let mut closing = battle(12_500, UnitKind::Levy, Facing::west(), false);
    closing
        .issue_move_order(MovementOrder {
            unit_id: "d".into(),
            destination: BattlePoint::new(8_000, 10_000),
        })
        .unwrap();
    closing.advance_ticks(39);
    assert_eq!(charge(&closing), Charge::Charging { run_up_mm: 4_000 });
    let mut neutral = without_charge(&closing);
    closing.advance_ticks(1);
    neutral.advance_ticks(1);
    assert_eq!(
        closing.units()[1].position().x_mm - closing.units()[0].position().x_mm,
        1_500
    );
    assert!(survivors(&closing) < survivors(&neutral));
    assert_eq!(
        charge(&closing),
        Charge::Recovering {
            ticks_remaining: 40
        }
    );
}
