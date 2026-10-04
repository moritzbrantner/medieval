use medieval_core::{
    BattlePoint, BattleSide, FlatBattlefield, Formation, MovementOrder, TacticalBattle,
    TacticalBattleState, TacticalError, TacticalFinishReason, TacticalUnit,
};

fn unit(id: &str, side: BattleSide, soldiers: u16, x: u32, y: u32) -> TacticalUnit {
    TacticalUnit::new(
        id,
        side,
        soldiers,
        BattlePoint::new(x, y),
        Formation::Line { files: 10 },
        10_000,
    )
}

fn duel(attacker: u16, defender: u16) -> TacticalBattle {
    let mut battle = TacticalBattle::new(
        FlatBattlefield::new(100_000, 100_000),
        vec![
            unit("a", BattleSide::Attacker, attacker, 10_000, 10_000),
            unit("d", BattleSide::Defender, defender, 11_000, 10_000),
        ],
    )
    .unwrap()
    .start();
    battle.issue_engagement_order("a", "d").unwrap();
    battle
}

#[test]
fn field_victory_records_the_exact_finishing_tick_and_replays() {
    let mut first = duel(100, 1);
    assert_eq!(first.state(), TacticalBattleState::Running);
    let mut replay = first.clone();
    first.advance_ticks(1_000);
    replay.advance_ticks(20);
    assert_eq!(first, replay);
    assert_eq!(
        first.state(),
        TacticalBattleState::Finished {
            winner: Some(BattleSide::Attacker),
            finishing_tick: 20,
            reason: TacticalFinishReason::ForceDefeated
        }
    );
}

#[test]
fn simultaneous_destruction_finishes_as_a_draw() {
    let mut battle = duel(1, 1);
    battle.advance_ticks(1_000);
    assert_eq!(
        battle.state(),
        TacticalBattleState::Finished {
            winner: None,
            finishing_tick: 20,
            reason: TacticalFinishReason::MutualDefeat
        }
    );
}

#[test]
fn finished_battle_rejects_every_gameplay_command_and_stays_frozen() {
    let mut battle = duel(100, 1);
    battle.advance_ticks(20);
    let encoded = serde_json::to_string(&battle).unwrap();
    let mut restored: TacticalBattle = serde_json::from_str(&encoded).unwrap();
    assert_eq!(
        restored.issue_move_order(MovementOrder {
            unit_id: "a".into(),
            destination: BattlePoint::new(20_000, 20_000)
        }),
        Err(TacticalError::BattleFinished)
    );
    assert_eq!(
        restored.issue_engagement_order("a", "d"),
        Err(TacticalError::BattleFinished)
    );
    assert_eq!(
        restored.issue_formation_order("a", Formation::Column { files: 10 }),
        Err(TacticalError::BattleFinished)
    );
    assert_eq!(
        restored.open_siege_gate(),
        Err(TacticalError::BattleFinished)
    );
    assert_eq!(
        restored.close_siege_gate(),
        Err(TacticalError::BattleFinished)
    );
    assert_eq!(
        restored.destroy_siege_gate(),
        Err(TacticalError::BattleFinished)
    );
    restored.advance_ticks(1_000);
    assert_eq!(restored.start(), battle);
}

#[test]
fn siege_capture_finishes_through_the_existing_core_capture_rule() {
    let mut battle = TacticalBattle::deploy_siege(
        FlatBattlefield::new(100_000, 100_000),
        vec![
            unit("a", BattleSide::Attacker, 40, 30_000, 50_000),
            unit("d", BattleSide::Defender, 40, 100_000, 100_000),
        ],
    )
    .unwrap()
    .start();
    battle.open_siege_gate().unwrap();
    battle
        .issue_move_order(MovementOrder {
            unit_id: "a".into(),
            destination: BattlePoint::new(80_000, 50_000),
        })
        .unwrap();
    battle.advance_ticks(2_000);
    let TacticalBattleState::Finished {
        winner,
        finishing_tick,
        reason,
    } = battle.state()
    else {
        panic!("siege should finish on capture")
    };
    assert_eq!(winner, Some(BattleSide::Attacker));
    assert_eq!(reason, TacticalFinishReason::SiegeCapture);
    assert_eq!(finishing_tick, battle.tick());
    assert_eq!(battle.siege_snapshot().unwrap().capture.captured_by, winner);
}

#[test]
fn legacy_sandbox_replays_remain_open_ended() {
    let battle = TacticalBattle::new(
        FlatBattlefield::new(100_000, 100_000),
        vec![unit("a", BattleSide::Attacker, 40, 10_000, 10_000)],
    )
    .unwrap();
    let encoded = serde_json::to_string(&battle).unwrap();
    assert!(!encoded.contains("completionRules"));
    let mut replay: TacticalBattle = serde_json::from_str(&encoded).unwrap();
    replay.advance_ticks(100);
    assert_eq!(replay.tick(), 100);
    assert_eq!(replay.state(), TacticalBattleState::Running);
    assert!(matches!(
        replay.start().state(),
        TacticalBattleState::Finished {
            winner: Some(BattleSide::Attacker),
            finishing_tick: 100,
            ..
        }
    ));
}

#[test]
fn effective_rout_counts_only_formed_survivors() {
    let battle = TacticalBattle::new(
        FlatBattlefield::new(100_000, 100_000),
        vec![
            unit("a", BattleSide::Attacker, 40, 10_000, 10_000),
            unit("d", BattleSide::Defender, 40, 90_000, 90_000),
        ],
    )
    .unwrap();
    let mut document = serde_json::to_value(&battle).unwrap();
    document["units"][0]["state"] = serde_json::json!("routed");
    let restored: TacticalBattle = serde_json::from_value(document).unwrap();
    let started = restored.start();
    assert_eq!(started.units()[0].soldiers(), 40);
    assert_eq!(
        started.state(),
        TacticalBattleState::Finished {
            winner: Some(BattleSide::Defender),
            finishing_tick: 0,
            reason: TacticalFinishReason::ForceDefeated
        }
    );
}

#[test]
fn empty_started_battle_finishes_at_tick_zero() {
    let battle = TacticalBattle::new(FlatBattlefield::new(100_000, 100_000), vec![])
        .unwrap()
        .start();
    assert_eq!(
        battle.state(),
        TacticalBattleState::Finished {
            winner: None,
            finishing_tick: 0,
            reason: TacticalFinishReason::MutualDefeat
        }
    );
}
