use medieval_core::{
    BattlePoint, BattleSide, BattlefieldLocation, FlatBattlefield, Formation, TacticalBattle,
    TacticalBattleState, TacticalError, TacticalFinishReason, TacticalUnit,
};

fn unit(id: &str, side: BattleSide, soldiers: u16, x: u32, speed: u32) -> TacticalUnit {
    TacticalUnit::new(
        id,
        side,
        soldiers,
        BattlePoint::new(x, 1_000),
        Formation::Line { files: 10 },
        speed,
    )
}

fn field() -> TacticalBattle {
    TacticalBattle::new_at_location(
        FlatBattlefield::new(100_000, 100_000),
        vec![
            unit("a", BattleSide::Attacker, 40, 10_000, 1_000),
            unit("d", BattleSide::Defender, 40, 90_000, 1_000),
        ],
        BattlefieldLocation::ForestClearing,
    )
    .unwrap()
    .start()
}

#[test]
fn each_side_withdraws_to_its_own_edge_and_preserves_survivors() {
    for (side, id, winner, edge) in [
        (BattleSide::Attacker, "a", BattleSide::Defender, 0),
        (BattleSide::Defender, "d", BattleSide::Attacker, 100_000),
    ] {
        let mut battle = field();
        battle.withdraw(side).unwrap();
        battle.advance_ticks(5);
        assert_eq!(battle.state(), TacticalBattleState::Running);
        let retreating = battle.units().iter().find(|unit| unit.id() == id).unwrap();
        assert!(retreating.is_withdrawing());
        assert!(!retreating.can_receive_orders());
        battle.advance_ticks(1_000);
        let escaped = battle.units().iter().find(|unit| unit.id() == id).unwrap();
        assert!(escaped.is_escaped());
        assert_eq!(escaped.position().x_mm, edge);
        assert_eq!(escaped.soldiers(), 40);
        assert_eq!(escaped.pursuit_casualties(), 0);
        assert_eq!(
            battle.state(),
            TacticalBattleState::Finished {
                winner: Some(winner),
                finishing_tick: 10,
                reason: TacticalFinishReason::Withdrawal
            }
        );
    }
}

#[test]
fn pursuit_casualties_are_conserved_and_can_destroy_a_retreating_unit() {
    let mut battle = TacticalBattle::new_at_location(
        FlatBattlefield::new(100_000, 100_000),
        vec![
            unit("a", BattleSide::Attacker, 20, 30_000, 50),
            unit("d", BattleSide::Defender, 100, 33_000, 50),
        ],
        BattlefieldLocation::ForestClearing,
    )
    .unwrap()
    .start();
    battle.issue_engagement_order("d", "a").unwrap();
    battle.withdraw(BattleSide::Attacker).unwrap();
    battle.advance_ticks(20);
    let retreating = &battle.units()[0];
    assert!(retreating.pursuit_casualties() > 0);
    assert_eq!(retreating.soldiers() + retreating.pursuit_casualties(), 20);
    battle.advance_ticks(1_000);
    let destroyed = &battle.units()[0];
    assert!(destroyed.is_destroyed());
    assert!(!destroyed.is_escaped());
    assert_eq!(destroyed.pursuit_casualties(), 20);
    assert!(matches!(
        battle.state(),
        TacticalBattleState::Finished {
            winner: Some(BattleSide::Defender),
            reason: TacticalFinishReason::Withdrawal,
            ..
        }
    ));
}

#[test]
fn withdrawal_replays_identically_across_tick_partition_and_serialization() {
    let mut first = field();
    first.withdraw(BattleSide::Attacker).unwrap();
    first.advance_ticks(3);
    let mut replay: TacticalBattle =
        serde_json::from_str(&serde_json::to_string(&first).unwrap()).unwrap();
    replay.withdraw(BattleSide::Attacker).unwrap();
    first.advance_ticks(100);
    replay.advance_ticks(4);
    replay.advance_ticks(100);
    assert_eq!(first, replay);
}

#[test]
fn both_sides_withdrawing_finishes_as_a_draw_after_both_escape() {
    let mut battle = field();
    battle.withdraw(BattleSide::Attacker).unwrap();
    battle.withdraw(BattleSide::Defender).unwrap();
    battle.advance_ticks(100);
    assert!(battle.units().iter().all(|unit| unit.is_escaped()));
    assert_eq!(
        battle.state(),
        TacticalBattleState::Finished {
            winner: None,
            finishing_tick: 10,
            reason: TacticalFinishReason::MutualWithdrawal
        }
    );
}

#[test]
fn fortress_defenders_cannot_escape_without_a_core_owned_exit() {
    let mut battle = TacticalBattle::deploy_siege_at_location(
        FlatBattlefield::new(100_000, 100_000),
        vec![
            unit("a", BattleSide::Attacker, 40, 10_000, 1_000),
            unit("d", BattleSide::Defender, 40, 90_000, 1_000),
        ],
        BattlefieldLocation::ForestClearing,
    )
    .unwrap()
    .start();
    let before = battle.clone();
    assert_eq!(
        battle.withdraw(BattleSide::Defender),
        Err(TacticalError::WithdrawalBlockedBySiege(
            BattleSide::Defender
        ))
    );
    assert_eq!(battle, before);
    battle.withdraw(BattleSide::Attacker).unwrap();
    battle.advance_ticks(100);
    assert!(battle.units()[0].is_escaped());
}

#[test]
fn escape_at_the_maximum_battlefield_coordinate_does_not_narrow_coordinates() {
    let mut battle = TacticalBattle::new_at_location(
        FlatBattlefield::new(u32::MAX, 100_000),
        vec![
            unit("a", BattleSide::Attacker, 40, 1_000, 1_000),
            unit("d", BattleSide::Defender, 40, u32::MAX - 1_000, 1_000),
        ],
        BattlefieldLocation::ForestClearing,
    )
    .unwrap()
    .start();
    battle.withdraw(BattleSide::Defender).unwrap();
    battle.advance_ticks(1);
    assert!(battle.units()[1].is_escaped());
    assert_eq!(battle.units()[1].position().x_mm, u32::MAX);
}

#[test]
fn escaped_units_stop_being_targets_while_other_units_are_still_retreating() {
    let mut battle = TacticalBattle::new_at_location(
        FlatBattlefield::new(100_000, 100_000),
        vec![
            unit("a-near", BattleSide::Attacker, 40, 1_000, 1_000),
            unit("a-far", BattleSide::Attacker, 40, 20_000, 1_000),
            unit("d", BattleSide::Defender, 40, 3_500, 1_000),
        ],
        BattlefieldLocation::ForestClearing,
    )
    .unwrap()
    .start();
    battle.issue_engagement_order("d", "a-near").unwrap();
    battle.withdraw(BattleSide::Attacker).unwrap();
    battle.advance_ticks(1);
    assert_eq!(battle.state(), TacticalBattleState::Running);
    assert!(
        battle
            .units()
            .iter()
            .find(|unit| unit.id() == "a-near")
            .unwrap()
            .is_escaped()
    );
    assert!(
        battle
            .units()
            .iter()
            .find(|unit| unit.id() == "d")
            .unwrap()
            .engagement_target()
            .is_none()
    );
    assert_eq!(
        battle.issue_engagement_order("d", "a-near"),
        Err(TacticalError::TargetEscaped("a-near".into()))
    );
}

#[test]
fn a_closed_gate_rejects_attackers_trapped_inside_the_fortress() {
    let mut battle = TacticalBattle::deploy_siege_at_location(
        FlatBattlefield::new(100_000, 100_000),
        vec![
            TacticalUnit::new(
                "a",
                BattleSide::Attacker,
                40,
                BattlePoint::new(30_000, 50_000),
                Formation::Line { files: 10 },
                10_000,
            ),
            unit("d", BattleSide::Defender, 40, 90_000, 1_000),
        ],
        BattlefieldLocation::ForestClearing,
    )
    .unwrap()
    .start();
    battle.open_siege_gate().unwrap();
    battle
        .issue_move_order(medieval_core::MovementOrder {
            unit_id: "a".into(),
            destination: BattlePoint::new(80_000, 50_000),
        })
        .unwrap();
    battle.advance_ticks(25);
    assert_eq!(
        battle.units()[0].position(),
        BattlePoint::new(80_000, 50_000)
    );
    battle.close_siege_gate().unwrap();
    let before = battle.clone();
    assert_eq!(
        battle.withdraw(BattleSide::Attacker),
        Err(TacticalError::WithdrawalBlockedBySiege(
            BattleSide::Attacker
        ))
    );
    assert_eq!(battle, before);
}

#[test]
fn escaped_routed_survivors_retain_their_rout_provenance() {
    let battle = TacticalBattle::new_at_location(
        FlatBattlefield::new(100_000, 100_000),
        vec![
            unit("a-routed", BattleSide::Attacker, 40, 1_000, 1_000),
            unit("a-formed", BattleSide::Attacker, 40, 20_000, 1_000),
            unit("d", BattleSide::Defender, 40, 90_000, 1_000),
        ],
        BattlefieldLocation::ForestClearing,
    )
    .unwrap();
    let mut document = serde_json::to_value(battle).unwrap();
    let routed = document["units"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|unit| unit["id"] == "a-routed")
        .unwrap();
    routed["state"] = serde_json::json!("routed");
    let restored: TacticalBattle = serde_json::from_value(document).unwrap();
    let mut battle = restored.start();
    battle.withdraw(BattleSide::Attacker).unwrap();
    battle.advance_ticks(100);
    let escaped = battle
        .units()
        .iter()
        .find(|unit| unit.id() == "a-routed")
        .unwrap();
    assert!(escaped.is_escaped());
    assert!(escaped.is_routed());
    assert_eq!(escaped.soldiers(), 40);
}

#[test]
fn withdrawing_units_cannot_continue_capturing_a_siege_objective() {
    let mut battle = TacticalBattle::deploy_siege_at_location(
        FlatBattlefield::new(100_000, 100_000),
        vec![
            TacticalUnit::new(
                "a",
                BattleSide::Attacker,
                40,
                BattlePoint::new(30_000, 50_000),
                Formation::Line { files: 10 },
                10_000,
            ),
            unit("d", BattleSide::Defender, 40, 90_000, 1_000),
        ],
        BattlefieldLocation::ForestClearing,
    )
    .unwrap()
    .start();
    battle.open_siege_gate().unwrap();
    battle
        .issue_move_order(medieval_core::MovementOrder {
            unit_id: "a".into(),
            destination: BattlePoint::new(80_000, 50_000),
        })
        .unwrap();
    battle.advance_ticks(25);
    let progress = battle.siege_snapshot().unwrap().capture.progress;
    assert!(progress > 0);
    let mut document = serde_json::to_value(&battle).unwrap();
    document["units"][0]["speedMmPerTick"] = serde_json::json!(1);
    let mut battle: TacticalBattle = serde_json::from_value(document).unwrap();
    battle.withdraw(BattleSide::Attacker).unwrap();
    battle.advance_ticks(20);
    assert!(battle.units()[0].is_withdrawing());
    assert_eq!(battle.siege_snapshot().unwrap().capture.progress, progress);
}

#[test]
fn a_withdrawing_attacker_keeps_its_siege_exit_until_crossing_the_wall() {
    let mut battle = TacticalBattle::deploy_siege_at_location(
        FlatBattlefield::new(100_000, 100_000),
        vec![
            TacticalUnit::new(
                "a",
                BattleSide::Attacker,
                40,
                BattlePoint::new(30_000, 50_000),
                Formation::Line { files: 10 },
                10_000,
            ),
            unit("d", BattleSide::Defender, 40, 90_000, 1_000),
        ],
        BattlefieldLocation::ForestClearing,
    )
    .unwrap()
    .start();
    battle.open_siege_gate().unwrap();
    battle
        .issue_move_order(medieval_core::MovementOrder {
            unit_id: "a".into(),
            destination: BattlePoint::new(80_000, 50_000),
        })
        .unwrap();
    battle.advance_ticks(25);
    battle.withdraw(BattleSide::Attacker).unwrap();
    let mut battle: TacticalBattle =
        serde_json::from_str(&serde_json::to_string(&battle).unwrap()).unwrap();
    let before = battle.clone();
    assert_eq!(
        battle.close_siege_gate(),
        Err(TacticalError::SiegeExitInUse)
    );
    assert_eq!(battle, before);
    for _ in 0..100 {
        if battle.units()[0].position().x_mm < battle.siege_snapshot().unwrap().layout.gate.min_x_mm
        {
            break;
        }
        battle.advance_ticks(1);
    }
    assert_eq!(battle.state(), TacticalBattleState::Running);
    assert!(
        battle.units()[0].position().x_mm < battle.siege_snapshot().unwrap().layout.gate.min_x_mm
    );
    battle.close_siege_gate().unwrap();
    battle.advance_ticks(100);
    assert!(battle.units()[0].is_escaped());
    assert!(matches!(
        battle.state(),
        TacticalBattleState::Finished {
            winner: Some(BattleSide::Defender),
            reason: TacticalFinishReason::Withdrawal,
            ..
        }
    ));
}
