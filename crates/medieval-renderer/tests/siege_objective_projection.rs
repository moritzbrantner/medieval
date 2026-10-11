//! Acceptance for #127: presentation projects every core siege objective, in
//! core order, and never decides capture.
//!
//! Public seams: core `TacticalBattle::siege_objectives()` and
//! `SiegeProfile::StagedKeepV1` (see
//! `crates/medieval-core/tests/siege_staged_objectives.rs`), and
//! `RenderSiegeSnapshot::objectives` — one `RenderSiegeCapture` per core
//! objective (indexable and iterable, core order) with a new
//! `RenderSiegeCapture::unlocked: bool`, all fields copied from the core
//! `SiegeObjectiveStatus`. The existing `RenderSiegeSnapshot::capture` stays.

use medieval_core::{
    BattlePoint, BattleSide, BattlefieldLocation, FlatBattlefield, Formation, MovementOrder,
    SiegeObjectiveStatus, SiegeProfile, TACTICAL_TICKS_PER_SECOND, TacticalBattle, TacticalUnit,
};
use medieval_renderer::{BattleRenderSnapshot, RenderSiegeCapture, RenderViewState};

const FIELD: FlatBattlefield = FlatBattlefield::new(100_000, 100_000);

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

fn projected(objective: &SiegeObjectiveStatus) -> RenderSiegeCapture {
    RenderSiegeCapture {
        center: objective.center,
        radius_mm: objective.radius_mm,
        progress: objective.progress,
        capturing_side: objective.capturing_side,
        captured_by: objective.captured_by,
        unlocked: objective.unlocked,
    }
}

/// Places a unit through the saved document so no route crosses an objective.
fn place(battle: &mut TacticalBattle, id: &str, point: BattlePoint) {
    let mut document = serde_json::to_value(&*battle).unwrap();
    let unit = document["units"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|unit| unit["id"] == id)
        .unwrap();
    unit["position"] = serde_json::to_value(point).unwrap();
    unit["destination"] = serde_json::Value::Null;
    *battle = serde_json::from_value(document).unwrap();
}

/// Staged siege after the first objective fell and the second is partly
/// taken: captured, in-progress and (with more than two objectives) locked
/// objectives side by side.
fn mid_assault() -> TacticalBattle {
    let mut battle = TacticalBattle::deploy_siege_with_profile(
        FIELD,
        vec![
            unit("first", BattleSide::Attacker, 10_000, 20_000),
            unit("second", BattleSide::Attacker, 10_000, 80_000),
            unit("guard", BattleSide::Defender, 99_000, 99_000),
        ],
        BattlefieldLocation::ForestClearing,
        SiegeProfile::StagedKeepV1,
    )
    .unwrap();
    let objectives = battle.siege_objectives().unwrap();
    let guard = BattlePoint::new(99_000, 99_000);
    assert!(
        objectives.iter().all(|objective| {
            guard.x_mm.abs_diff(objective.center.x_mm) > objective.radius_mm
                || guard.y_mm.abs_diff(objective.center.y_mm) > objective.radius_mm
        }),
        "fixture: the idle defender stands off every objective"
    );
    place(&mut battle, "first", objectives[0].center);
    place(&mut battle, "second", objectives[1].center);
    for _ in 0..200 {
        let now = battle.siege_objectives().unwrap();
        if now[0].captured_by.is_some() && now[1].progress > 0 {
            return battle;
        }
        battle.advance_ticks(TACTICAL_TICKS_PER_SECOND);
    }
    panic!("fixture: the staged assault reaches the second objective");
}

#[test]
fn render_snapshot_projects_every_core_objective_in_order_without_changing_the_battle() {
    let battle = mid_assault();
    let core = battle.siege_objectives().unwrap();
    assert_eq!(core[0].captured_by, Some(BattleSide::Attacker));
    assert!(core[1].unlocked && core[1].captured_by.is_none());

    let before = battle.clone();
    let snapshot = BattleRenderSnapshot::capture(&battle, &RenderViewState::fit(FIELD));
    let siege = snapshot.siege.unwrap();
    let expected: Vec<_> = core.iter().map(projected).collect();
    let actual: Vec<_> = siege.objectives.iter().copied().collect();
    assert_eq!(
        actual, expected,
        "one projected objective per core objective"
    );
    assert_eq!(battle, before, "projection never mutates simulation truth");
}

#[test]
fn single_point_projection_keeps_the_legacy_capture_as_its_only_objective() {
    let mut battle = TacticalBattle::deploy_siege_with_profile(
        FIELD,
        vec![
            unit("a", BattleSide::Attacker, 30_000, 50_000),
            unit("d", BattleSide::Defender, 100_000, 100_000),
        ],
        BattlefieldLocation::ForestClearing,
        SiegeProfile::StoneWallsV1,
    )
    .unwrap();
    battle.open_siege_gate().unwrap();
    battle
        .issue_move_order(MovementOrder {
            unit_id: "a".into(),
            destination: BattlePoint::new(80_000, 50_000),
        })
        .unwrap();
    battle.advance_ticks(160);
    let snapshot = BattleRenderSnapshot::capture(&battle, &RenderViewState::fit(FIELD));
    let siege = snapshot.siege.unwrap();
    assert!(siege.capture.progress > 0, "fixture: capture under way");
    let objectives: Vec<_> = siege.objectives.iter().copied().collect();
    assert_eq!(
        objectives,
        vec![RenderSiegeCapture {
            unlocked: true,
            ..siege.capture
        }]
    );
}
