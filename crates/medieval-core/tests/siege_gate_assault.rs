//! Acceptance for #123: targeted gate attacks and deterministic gate destruction.
//!
//! Public seams these tests require (named here so the implementation stays
//! minimal):
//! - `TacticalBattle::issue_gate_attack_order(&mut self, unit_id: &str)
//!   -> Result<(), TacticalError>`: the semantic attack-gate order. A siege
//!   layout has exactly one gate, so the gate needs no separate identifier.
//! - `SiegeBattleState::gate_integrity: u16` (public, serialized like
//!   `capture.progress`), reached through `TacticalBattle::siege_snapshot`.
//! - `medieval_core::SIEGE_GATE_MAX_INTEGRITY: u16`, re-exported beside
//!   `SIEGE_CAPTURE_MAX_PROGRESS`. Integrity starts there and the gate becomes
//!   `destroyed` exactly when it reaches zero.
//!
//! Gate state is read through its stable camelCase wire form so the tests do
//! not depend on `SiegeGateState` being re-exported.

use medieval_core::{
    BattlePoint, BattleSide, FlatBattlefield, Formation, MovementOrder, SIEGE_GATE_MAX_INTEGRITY,
    TacticalBattle, TacticalError, TacticalUnit,
};

/// Liveness guard only (30 minutes of battle time), not a tuning budget.
const ASSAULT_TICK_LIMIT: u32 = 20 * 60 * 30;

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

/// Stone-walls siege on a 100 m square: wall at x 49–51 m, gate y 45–55 m.
/// The ram unit starts at the attacker deployment edge in front of the gate;
/// both defenders stay far from the gate and the capture point.
fn siege() -> TacticalBattle {
    TacticalBattle::deploy_siege(
        FlatBattlefield::new(100_000, 100_000),
        vec![
            unit("ram", BattleSide::Attacker, 35_000, 50_000),
            unit("reserve", BattleSide::Attacker, 20_000, 20_000),
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

fn gate_integrity(battle: &TacticalBattle) -> u16 {
    battle.siege_snapshot().unwrap().gate_integrity
}

fn unit_position(battle: &TacticalBattle, id: &str) -> BattlePoint {
    battle
        .units()
        .iter()
        .find(|unit| unit.id() == id)
        .unwrap()
        .position()
}

/// Advances one tick at a time until the gate is destroyed and returns the
/// destruction tick, recording every per-tick (integrity, state) sample.
fn assault_until_destroyed(battle: &mut TacticalBattle) -> (u64, Vec<(u16, String)>) {
    let mut samples = Vec::new();
    for _ in 0..ASSAULT_TICK_LIMIT {
        battle.advance_ticks(1);
        samples.push((gate_integrity(battle), gate_state(battle)));
        if gate_state(battle) == "destroyed" {
            return (battle.tick(), samples);
        }
    }
    panic!("an ordered gate assault must destroy the gate within the liveness limit");
}

#[test]
fn a_fresh_siege_gate_starts_closed_at_full_integrity() {
    let battle = siege();
    assert_eq!(gate_state(&battle), "closed");
    assert_eq!(gate_integrity(&battle), SIEGE_GATE_MAX_INTEGRITY);
    const { assert!(SIEGE_GATE_MAX_INTEGRITY > 0) };
}

#[test]
fn a_formed_attacker_may_be_ordered_to_attack_the_standing_gate() {
    let mut battle = siege();
    assert_eq!(battle.issue_gate_attack_order("ram"), Ok(()));
    // Issuing the order alone changes no gate truth; damage is applied by ticks.
    assert_eq!(gate_state(&battle), "closed");
    assert_eq!(gate_integrity(&battle), SIEGE_GATE_MAX_INTEGRITY);
}

#[test]
fn illegal_gate_attack_orders_are_rejected_without_changing_the_battle() {
    // Defenders do not assault their own gate.
    let mut battle = siege();
    let before = battle.clone();
    assert!(battle.issue_gate_attack_order("defender-north").is_err());
    assert_eq!(battle, before);

    // Unknown unit.
    assert_eq!(
        battle.issue_gate_attack_order("missing"),
        Err(TacticalError::UnitNotFound("missing".into()))
    );
    assert_eq!(battle, before);

    // A routed unit cannot receive orders.
    let mut document = serde_json::to_value(&battle).unwrap();
    for unit in document["units"].as_array_mut().unwrap() {
        if unit["id"] == "ram" {
            unit["state"] = serde_json::json!("routed");
        }
    }
    let mut routed: TacticalBattle = serde_json::from_value(document).unwrap();
    let routed_before = routed.clone();
    assert_eq!(
        routed.issue_gate_attack_order("ram"),
        Err(TacticalError::UnitCannotReceiveOrders {
            unit_id: "ram".into()
        })
    );
    assert_eq!(routed, routed_before);

    // An already destroyed gate cannot be attacked again.
    let mut destroyed = siege();
    destroyed.destroy_siege_gate().unwrap();
    let destroyed_before = destroyed.clone();
    assert!(destroyed.issue_gate_attack_order("ram").is_err());
    assert_eq!(destroyed, destroyed_before);

    // A field battle has no gate.
    let mut field = TacticalBattle::new(
        FlatBattlefield::new(100_000, 100_000),
        vec![
            unit("a", BattleSide::Attacker, 10_000, 10_000),
            unit("d", BattleSide::Defender, 90_000, 90_000),
        ],
    )
    .unwrap();
    let field_before = field.clone();
    assert_eq!(
        field.issue_gate_attack_order("a"),
        Err(TacticalError::NotSiegeBattle)
    );
    assert_eq!(field, field_before);

    // A finished battle rejects every gameplay command.
    let mut finished = TacticalBattle::deploy_siege(
        FlatBattlefield::new(100_000, 100_000),
        vec![unit("ram", BattleSide::Attacker, 35_000, 50_000)],
    )
    .unwrap()
    .start();
    let finished_before = finished.clone();
    assert_eq!(
        finished.issue_gate_attack_order("ram"),
        Err(TacticalError::BattleFinished)
    );
    assert_eq!(finished, finished_before);
}

#[test]
fn gate_damage_accumulates_and_destroys_the_gate_exactly_at_zero_integrity() {
    let mut battle = siege();
    battle.issue_gate_attack_order("ram").unwrap();
    let (_, samples) = assault_until_destroyed(&mut battle);

    let mut previous = SIEGE_GATE_MAX_INTEGRITY;
    let mut damaging_ticks = 0;
    for (index, (integrity, state)) in samples.iter().enumerate() {
        assert!(
            *integrity <= previous,
            "gate integrity must never recover during an assault"
        );
        if *integrity < previous {
            damaging_ticks += 1;
        }
        previous = *integrity;
        let last = index + 1 == samples.len();
        if last {
            assert_eq!(*integrity, 0, "the gate is destroyed exactly at zero");
            assert_eq!(state, "destroyed");
        } else {
            assert!(*integrity > 0, "zero integrity must destroy the gate");
            assert_eq!(state, "closed", "a damaged gate stands until zero");
        }
    }
    assert!(
        damaging_ticks >= 2,
        "gate damage accumulates over several hits rather than one instant switch"
    );

    // Once destroyed the gate stays destroyed and cannot be targeted again.
    assert!(battle.issue_gate_attack_order("ram").is_err());
    battle.advance_ticks(200);
    assert_eq!(gate_state(&battle), "destroyed");
    assert_eq!(gate_integrity(&battle), 0);
}

#[test]
fn gate_destruction_is_deterministic_and_replays_from_a_saved_battle() {
    let mut first = siege();
    first.issue_gate_attack_order("ram").unwrap();
    let mut second = first.clone();
    let (first_tick, first_samples) = assault_until_destroyed(&mut first);
    let (second_tick, second_samples) = assault_until_destroyed(&mut second);
    assert_eq!(first_tick, second_tick);
    assert_eq!(first_samples, second_samples);
    assert_eq!(first, second);

    // Save mid-assault, after the gate has taken damage but still stands.
    let mut saved = siege();
    saved.issue_gate_attack_order("ram").unwrap();
    while gate_integrity(&saved) == SIEGE_GATE_MAX_INTEGRITY {
        assert!(saved.tick() < u64::from(ASSAULT_TICK_LIMIT));
        saved.advance_ticks(1);
    }
    assert_eq!(gate_state(&saved), "closed");
    let encoded = serde_json::to_string(&saved).unwrap();
    let mut restored: TacticalBattle = serde_json::from_str(&encoded).unwrap();
    assert_eq!(restored, saved);
    assert_eq!(gate_integrity(&restored), gate_integrity(&saved));
    let (restored_tick, _) = assault_until_destroyed(&mut restored);
    assert_eq!(restored_tick, first_tick);
    assert_eq!(restored, first);
}

#[test]
fn sieges_recorded_before_gate_integrity_load_with_a_full_gate() {
    let battle = siege();
    let mut document = serde_json::to_value(&battle).unwrap();
    let siege = document["siege"].as_object_mut().unwrap();
    assert!(siege.remove("gateIntegrity").is_some());
    let legacy: TacticalBattle = serde_json::from_value(document).unwrap();
    assert_eq!(gate_integrity(&legacy), SIEGE_GATE_MAX_INTEGRITY);
    assert_eq!(legacy, battle);
}

#[test]
fn a_gate_destroyed_by_assault_opens_the_existing_siege_path() {
    let destination = BattlePoint::new(70_000, 50_000);
    let mut battle = siege();

    // Closed gate: the existing contract stops the unit at the near gate edge.
    battle
        .issue_move_order(MovementOrder {
            unit_id: "ram".into(),
            destination,
        })
        .unwrap();
    battle.advance_ticks(200);
    let gate = battle.siege_snapshot().unwrap().layout.gate;
    assert!(unit_position(&battle, "ram").x_mm < gate.min_x_mm);
    // Gate attacks are targeted: a unit pressed against the gate under a plain
    // move order does not damage it.
    assert_eq!(gate_state(&battle), "closed");
    assert_eq!(gate_integrity(&battle), SIEGE_GATE_MAX_INTEGRITY);
    assert!(
        !battle
            .siege_snapshot()
            .unwrap()
            .is_passable_at(gate.center())
    );

    battle.issue_gate_attack_order("ram").unwrap();
    assault_until_destroyed(&mut battle);
    let snapshot = battle.siege_snapshot().unwrap();
    assert!(snapshot.gate_state.is_traversable());
    assert!(snapshot.is_passable_at(gate.center()));
    assert!(!snapshot.layout.wall_contains(gate.center()));

    // Destroyed gate: the same order now crosses the wall line and arrives.
    battle
        .issue_move_order(MovementOrder {
            unit_id: "ram".into(),
            destination,
        })
        .unwrap();
    battle.advance_ticks(400);
    assert_eq!(unit_position(&battle, "ram"), destination);
}
