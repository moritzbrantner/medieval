//! Acceptance for #124: presentation projects core tower activity and never
//! decides tower targets or casualties.
//!
//! Public seams: core `TacticalBattle::siege_tower_activity()` (see
//! `crates/medieval-core/tests/siege_tower_combat.rs`) and a renderer
//! `RenderSiegeTower::active: bool` copied from the core `staffed` flag.

use medieval_core::{
    BattlePoint, BattleSide, BattlefieldLocation, FlatBattlefield, Formation, MovementOrder,
    SiegeProfile, TacticalBattle, TacticalUnit,
};
use medieval_renderer::{BattleRenderSnapshot, RenderViewState};

const BATTLEFIELD: FlatBattlefield = FlatBattlefield::new(300_000, 200_000);

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

/// Stone siege with a defender standing inside tower 0's footprint on the
/// defender side of the wall and an attacker in front of the wall.
fn staffed_siege() -> TacticalBattle {
    let mut battle = TacticalBattle::deploy_siege_with_profile(
        BATTLEFIELD,
        vec![
            unit("near", BattleSide::Attacker, 100_000, 30_000),
            unit("staff", BattleSide::Defender, 200_000, 30_000),
        ],
        BattlefieldLocation::ForestClearing,
        SiegeProfile::StoneWallsV1,
    )
    .unwrap();
    let siege = battle.siege_snapshot().unwrap();
    let tower = siege.layout.towers[0];
    let wall_max_x = siege.layout.wall_segments[0].max_x_mm;
    let staffing = BattlePoint::new(
        wall_max_x + (tower.center.x_mm + tower.radius_mm - wall_max_x) / 2,
        tower.center.y_mm,
    );
    let range = SiegeProfile::StoneWallsV1.tower_range_mm();
    let attacker = BattlePoint::new(
        tower.center.x_mm - (tower.radius_mm + range) / 2,
        tower.center.y_mm,
    );
    for (id, destination) in [("staff", staffing), ("near", attacker)] {
        battle
            .issue_move_order(MovementOrder {
                unit_id: id.into(),
                destination,
            })
            .unwrap();
    }
    battle.advance_ticks(400);
    battle
}

#[test]
fn render_snapshot_projects_core_tower_activity_without_changing_the_battle() {
    let battle = staffed_siege();
    let activity = battle.siege_tower_activity().unwrap();
    assert!(activity[0].staffed, "fixture: tower 0 is staffed");
    assert!(activity[1..].iter().all(|tower| !tower.staffed));

    let before = battle.clone();
    let snapshot = BattleRenderSnapshot::capture(&battle, &RenderViewState::fit(BATTLEFIELD));
    let siege = snapshot.siege.unwrap();
    for (projected, core) in siege.towers.iter().zip(&activity) {
        assert_eq!(projected.active, core.staffed);
    }
    assert_eq!(battle, before, "projection never mutates simulation truth");
}
