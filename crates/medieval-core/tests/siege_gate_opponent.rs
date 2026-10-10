//! Acceptance for #123: the AI command surface reaches the same gate target.
//!
//! The opponent planner (`TacticalBattle::plan_opponent_orders`, replanned once
//! per second by the browser and native adapters) is the AI command surface.
//! These tests use only existing public APIs: an attacking AI facing a closed
//! gate with every defender behind the wall must breach the gate through the
//! core gate-attack rule and then reach the defenders, while a defending AI
//! never damages its own gate.

use medieval_core::{
    BattlePoint, BattleSide, FlatBattlefield, Formation, TACTICAL_TICKS_PER_SECOND, TacticalBattle,
    TacticalUnit,
};

/// Liveness guard only (30 minutes of battle time), not a tuning budget.
const ASSAULT_TICK_LIMIT: u64 = 20 * 60 * 30;

fn unit(id: &str, side: BattleSide, x: u32, y: u32) -> TacticalUnit {
    TacticalUnit::new(
        id,
        side,
        100,
        BattlePoint::new(x, y),
        Formation::Line { files: 10 },
        1_000,
    )
}

fn siege() -> TacticalBattle {
    TacticalBattle::deploy_siege(
        FlatBattlefield::new(100_000, 100_000),
        vec![
            unit("attacker-a", BattleSide::Attacker, 30_000, 50_000),
            unit("attacker-b", BattleSide::Attacker, 30_000, 40_000),
            unit("defender-north", BattleSide::Defender, 90_000, 10_000),
            unit("defender-south", BattleSide::Defender, 90_000, 90_000),
        ],
    )
    .unwrap()
}

fn gate_state(battle: &TacticalBattle) -> String {
    serde_json::to_value(battle.siege_snapshot().unwrap().gate_state)
        .unwrap()
        .as_str()
        .unwrap()
        .to_owned()
}

/// Runs the adapters' loop shape: replan, then one second of ticks.
fn run_ai(battle: &mut TacticalBattle, side: BattleSide, until: impl Fn(&TacticalBattle) -> bool) {
    while !until(battle) && battle.tick() < ASSAULT_TICK_LIMIT {
        battle.plan_opponent_orders(side).unwrap();
        battle.advance_ticks(TACTICAL_TICKS_PER_SECOND);
    }
}

#[test]
fn attacking_ai_breaches_a_closed_gate_and_then_crosses_it() {
    let mut battle = siege();
    run_ai(&mut battle, BattleSide::Attacker, |battle| {
        gate_state(battle) == "destroyed"
    });
    assert_eq!(
        gate_state(&battle),
        "destroyed",
        "an attacking AI blocked by a closed gate must target and destroy it"
    );
    let breached_at = battle.tick();

    let gate = battle.siege_snapshot().unwrap().layout.gate;
    run_ai(&mut battle, BattleSide::Attacker, |battle| {
        battle
            .units()
            .iter()
            .any(|unit| unit.side() == BattleSide::Attacker && unit.position().x_mm > gate.max_x_mm)
    });
    assert!(
        battle
            .units()
            .iter()
            .any(|unit| unit.side() == BattleSide::Attacker && unit.position().x_mm > gate.max_x_mm),
        "after the breach the AI paths through the destroyed gate"
    );

    // The same inputs breach on the same tick.
    let mut replay = siege();
    run_ai(&mut replay, BattleSide::Attacker, |battle| {
        gate_state(battle) == "destroyed"
    });
    assert_eq!(replay.tick(), breached_at);
}

#[test]
fn defending_ai_never_attacks_its_own_gate() {
    let mut battle = TacticalBattle::deploy_siege(
        FlatBattlefield::new(100_000, 100_000),
        vec![
            unit("attacker", BattleSide::Attacker, 30_000, 50_000),
            unit("defender", BattleSide::Defender, 70_000, 50_000),
        ],
    )
    .unwrap();
    let before = battle.siege_snapshot().unwrap();
    for _ in 0..120 {
        battle.plan_opponent_orders(BattleSide::Defender).unwrap();
        battle.advance_ticks(TACTICAL_TICKS_PER_SECOND);
    }
    let after = battle.siege_snapshot().unwrap();
    assert_eq!(gate_state(&battle), "closed");
    assert_eq!(after.gate_state, before.gate_state);
    // Whatever integrity representation the gate has, the defender leaves it untouched.
    assert_eq!(
        serde_json::to_value(after).unwrap()["gateIntegrity"],
        serde_json::to_value(before).unwrap()["gateIntegrity"]
    );
}
