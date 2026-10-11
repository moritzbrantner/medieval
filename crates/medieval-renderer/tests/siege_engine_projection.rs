//! Acceptance for #126: presentation projects core siege-engine state and
//! never decides damage, range, reload or ammunition.
//!
//! Public seams: core `TacticalBattle::siege_engines()` (see
//! `crates/medieval-core/tests/siege_engines.rs`) and a renderer
//! `BattleRenderSnapshot::siege_engines: Vec<RenderSiegeEngine>` whose
//! `kind`, `position`, `ammunition` and `last_shot_tick` are copied from the
//! core `SiegeEngineStatus` in core order (empty outside a siege or without
//! engines). Shot trajectories and effects are renderer-owned decoration
//! keyed on `last_shot_tick`.

use medieval_core::{
    BattlePoint, BattleSide, BattlefieldLocation, CATAPULT_MAX_RANGE_MM, CATAPULT_MIN_RANGE_MM,
    FlatBattlefield, Formation, MovementOrder, SiegeEngineKind, SiegeEngineSetup,
    SiegeEngineTarget, SiegeProfile, TacticalBattle, TacticalUnit,
};
use medieval_renderer::{BattleRenderSnapshot, RenderViewState};

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

fn battlefield() -> FlatBattlefield {
    FlatBattlefield::new(2 * CATAPULT_MAX_RANGE_MM + 20_000, 100_000)
}

/// A siege whose catapult has fired once at the gate.
fn bombarded_siege() -> TacticalBattle {
    let field = battlefield();
    let mut battle = TacticalBattle::deploy_siege_with_engines(
        field,
        vec![
            unit("ram-crew", BattleSide::Attacker, 1_000, 30_000),
            unit("catapult-crew", BattleSide::Attacker, 1_000, 50_000),
            unit(
                "defender",
                BattleSide::Defender,
                field.width_mm - 1_000,
                10_000,
            ),
        ],
        BattlefieldLocation::ForestClearing,
        SiegeProfile::StoneWallsV1,
        vec![
            SiegeEngineSetup {
                kind: SiegeEngineKind::BatteringRam,
                crew_unit_id: "ram-crew".into(),
            },
            SiegeEngineSetup {
                kind: SiegeEngineKind::Catapult,
                crew_unit_id: "catapult-crew".into(),
            },
        ],
    )
    .unwrap();
    let gate = battle.siege_snapshot().unwrap().layout.gate.center();
    let distance = CATAPULT_MIN_RANGE_MM + (CATAPULT_MAX_RANGE_MM - CATAPULT_MIN_RANGE_MM) / 2;
    battle
        .issue_move_order(MovementOrder {
            unit_id: "catapult-crew".into(),
            destination: BattlePoint::new(gate.x_mm - distance, gate.y_mm),
        })
        .unwrap();
    for _ in 0..20_000 {
        if battle
            .units()
            .iter()
            .all(|unit| unit.destination().is_none())
        {
            break;
        }
        battle.advance_ticks(1);
    }
    battle
        .issue_siege_engine_fire_order("catapult-crew", SiegeEngineTarget::Gate)
        .unwrap();
    battle.advance_ticks(40);
    battle
}

#[test]
fn render_snapshot_projects_core_engine_state_without_changing_the_battle() {
    let battle = bombarded_siege();
    let core = battle.siege_engines().unwrap();
    assert!(
        core[1].last_shot_tick.is_some(),
        "fixture: the catapult has fired"
    );

    let before = battle.clone();
    let snapshot = BattleRenderSnapshot::capture(&battle, &RenderViewState::fit(battlefield()));
    assert_eq!(snapshot.siege_engines.len(), core.len());
    for (projected, status) in snapshot.siege_engines.iter().zip(&core) {
        assert_eq!(projected.kind, status.kind);
        assert_eq!(projected.position, status.position);
        assert_eq!(projected.ammunition, status.ammunition);
        assert_eq!(projected.last_shot_tick, status.last_shot_tick);
    }
    assert_eq!(battle, before, "projection never mutates simulation truth");

    // Without engines nothing is projected.
    let plain = TacticalBattle::deploy_siege(
        FlatBattlefield::new(100_000, 100_000),
        vec![unit("a", BattleSide::Attacker, 10_000, 10_000)],
    )
    .unwrap();
    let snapshot = BattleRenderSnapshot::capture(
        &plain,
        &RenderViewState::fit(FlatBattlefield::new(100_000, 100_000)),
    );
    assert!(snapshot.siege_engines.is_empty());
}
