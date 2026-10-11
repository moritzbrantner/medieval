//! Acceptance for issue #94: the tactical deployment phase and its placement
//! rule (owner decision 2026-10-11, option A).
//!
//! A formation is legally deployed only when its *whole* formation footprint
//! lies inside its own side's deployment zone and does not overlap any other
//! formation of the same side. The rule is enforced by the authoritative Rust
//! battle for every deployment edit: repositioning, rotation (facing) and
//! frontage/formation changes. A rejected edit changes nothing.
//!
//! Seams named here (public, minimal):
//! - `TacticalBattleState::Deployment`, serialized as `{"phase":"deployment"}`.
//! - `TacticalBattle::into_deployment(self) -> Result<Self, TacticalError>`
//!   enters the phase and rejects an initial layout that breaks the rule.
//! - `TacticalBattle::deploy_formation(&mut self, unit_id, position)` places a
//!   formation instantly (no march, no tick).
//! - `TacticalBattle::begin_battle(&mut self)` leaves deployment exactly once.
//! - `TacticalError::InvalidDeploymentPlacement { unit_id, reason }` with
//!   `DeploymentRejection::{OutsideZone, FriendlyOverlap}`.
//! - The existing formation APIs (`issue_formation_orders`,
//!   `issue_formation_order`) work during deployment, in place, under the rule.
//!
//! The footprint is the core's own `TacticalUnit::formation_footprint()`, so
//! these tests hold whether or not a later footprint model depends on facing.
//! Arrangements are computed from reported footprints and only assume that
//! moving a formation translates its footprint.

use medieval_core::{
    BattlePoint, BattleSide, BattlefieldLocation, DeploymentRejection, DeploymentZone, Facing,
    FlatBattlefield, Formation, FormationFootprint, FormationOrder, MovementOrder, TacticalBattle,
    TacticalBattleState, TacticalError, TacticalFinishReason, TacticalUnit, new_campaign,
};

/// 90 m × 60 m: the attacker zone is x 0..=30 000, the defender zone
/// x 60 000..=90 000, both over the full depth.
fn field() -> FlatBattlefield {
    FlatBattlefield::new(90_000, 60_000)
}

/// 40 soldiers in a 10-file line: a 10 m × 4 m footprint today.
fn line_unit(id: &str, side: BattleSide, x_mm: u32, y_mm: u32) -> TacticalUnit {
    TacticalUnit::new(
        id,
        side,
        40,
        BattlePoint::new(x_mm, y_mm),
        Formation::Line { files: 10 },
        1_000,
    )
    .with_facing(match side {
        BattleSide::Attacker => Facing::east(),
        BattleSide::Defender => Facing::west(),
    })
}

fn units() -> Vec<TacticalUnit> {
    vec![
        line_unit("a1", BattleSide::Attacker, 10_000, 10_000),
        line_unit("a2", BattleSide::Attacker, 10_000, 30_000),
        line_unit("a3", BattleSide::Attacker, 10_000, 50_000),
        line_unit("d1", BattleSide::Defender, 80_000, 10_000),
        line_unit("d2", BattleSide::Defender, 80_000, 30_000),
    ]
}

fn deploying() -> TacticalBattle {
    TacticalBattle::deploy_at_location(field(), units(), BattlefieldLocation::ForestClearing)
        .expect("fixture anchors are inside their zones")
        .into_deployment()
        .expect("fixture layout satisfies the deployment rule")
}

fn unit<'a>(battle: &'a TacticalBattle, id: &str) -> &'a TacticalUnit {
    battle
        .units()
        .iter()
        .find(|unit| unit.id() == id)
        .unwrap_or_else(|| panic!("no unit {id}"))
}

fn zone(battle: &TacticalBattle, side: BattleSide) -> DeploymentZone {
    battle
        .deployment_zones()
        .into_iter()
        .find(|zone| zone.side == side)
        .expect("both sides have a deployment zone")
}

fn footprint(battle: &TacticalBattle, id: &str) -> FormationFootprint {
    unit(battle, id).formation_footprint()
}

/// Footprint extents around the anchor: (left, right, below, above), in mm.
fn extents(battle: &TacticalBattle, id: &str) -> (i64, i64, i64, i64) {
    let unit = unit(battle, id);
    let bounds = unit.formation_footprint();
    let x = i64::from(unit.position().x_mm);
    let y = i64::from(unit.position().y_mm);
    (
        x - bounds.min_x_mm,
        bounds.max_x_mm - x,
        y - bounds.min_y_mm,
        bounds.max_y_mm - y,
    )
}

fn point(x_mm: i64, y_mm: i64) -> BattlePoint {
    BattlePoint::new(
        u32::try_from(x_mm).expect("test point x is non-negative"),
        u32::try_from(y_mm).expect("test point y is non-negative"),
    )
}

/// The rule itself, checked against the core's reported footprints, plus a
/// sanity check that footprints are not degenerate (they contain the anchor
/// and cover at least the soldier grid's span).
fn assert_legal_layout(battle: &TacticalBattle) {
    let units = battle.units();
    for (index, unit) in units.iter().enumerate() {
        let bounds = unit.formation_footprint();
        let position = unit.position();
        assert!(
            bounds.min_x_mm <= i64::from(position.x_mm)
                && i64::from(position.x_mm) <= bounds.max_x_mm
                && bounds.min_y_mm <= i64::from(position.y_mm)
                && i64::from(position.y_mm) <= bounds.max_y_mm,
            "{} footprint {bounds:?} contains its anchor {position:?}",
            unit.id()
        );
        let files = i64::from(unit.frontage_slots().max(1));
        let rows = i64::from(unit.soldiers()).div_euclid(files)
            + i64::from(i64::from(unit.soldiers()) % files != 0);
        let area = (bounds.max_x_mm - bounds.min_x_mm) * (bounds.max_y_mm - bounds.min_y_mm);
        assert!(
            area >= (files - 1) * 1_000 * (rows - 1) * 1_000,
            "{} footprint {bounds:?} covers its {files}×{rows} soldier grid",
            unit.id()
        );
        assert!(
            bounds.is_inside(zone(battle, unit.side())),
            "{} footprint {bounds:?} lies inside its zone",
            unit.id()
        );
        for other in &units[index + 1..] {
            if other.side() == unit.side() {
                assert!(
                    !bounds.overlaps(other.formation_footprint()),
                    "{} and {} must not overlap",
                    unit.id(),
                    other.id()
                );
            }
        }
    }
}

fn assert_rejected(
    result: Result<(), TacticalError>,
    unit_id: &str,
    expected: DeploymentRejection,
) {
    match result {
        Err(TacticalError::InvalidDeploymentPlacement {
            unit_id: rejected,
            reason,
        }) => {
            assert_eq!(rejected, unit_id);
            assert_eq!(reason, expected);
        }
        other => panic!("expected {expected:?} for {unit_id}, got {other:?}"),
    }
}

/// Asserts the edit was refused for exactly this reason and changed nothing.
fn assert_refused(
    battle: &mut TacticalBattle,
    unit_id: &str,
    expected: DeploymentRejection,
    edit: impl FnOnce(&mut TacticalBattle) -> Result<(), TacticalError>,
) {
    let before = battle.clone();
    assert_rejected(edit(battle), unit_id, expected);
    assert_eq!(
        *battle, before,
        "a rejected deployment edit changes nothing"
    );
}

fn assert_placed_in_place(battle: &TacticalBattle, id: &str, position: BattlePoint) {
    let unit = unit(battle, id);
    assert_eq!(unit.position(), position, "{id} is placed instantly");
    assert_eq!(unit.destination(), None, "{id} has no movement order");
    assert!(unit.queued_movements().is_empty());
    assert_eq!(battle.tick(), 0);
    assert_eq!(battle.state(), TacticalBattleState::Deployment);
}

/// `a1` in the middle of the zone, hemmed by `a2` on its right and `a3` above,
/// each exactly one metre clear of it, both centred on `a1`'s axis.
fn hemmed() -> TacticalBattle {
    let mut battle = deploying();
    let centre = point(12_000, 30_000);
    let (_, a1_right, _, a1_above) = extents(&battle, "a1");
    let (a2_left, ..) = extents(&battle, "a2");
    let (_, _, a3_below, _) = extents(&battle, "a3");
    // Clear the centre first, then close in.
    battle
        .deploy_formation("a2", point(12_000 + a1_right + 1_000 + a2_left, 30_000))
        .expect("a2 fits right of the centre");
    battle
        .deploy_formation("a3", point(12_000, 30_000 + a1_above + 1_000 + a3_below))
        .expect("a3 fits above the centre");
    battle
        .deploy_formation("a1", centre)
        .expect("a1 fits in the centre");
    assert_legal_layout(&battle);
    battle
}

#[test]
fn entering_deployment_freezes_the_clock_and_refuses_orders() {
    let mut battle = deploying();
    assert_eq!(battle.state(), TacticalBattleState::Deployment);
    assert_eq!(
        serde_json::to_value(battle.state()).unwrap(),
        serde_json::json!({ "phase": "deployment" })
    );
    assert_eq!(battle.tick(), 0);
    assert_legal_layout(&battle);

    let before = battle.clone();
    battle.advance_ticks(40);
    assert_eq!(battle, before, "no tick runs during deployment");

    assert!(
        battle
            .issue_move_order(MovementOrder {
                unit_id: "a1".into(),
                destination: BattlePoint::new(20_000, 10_000),
            })
            .is_err(),
        "nothing marches before the start"
    );
    assert!(battle.issue_engagement_order("a1", "d1").is_err());
    assert_eq!(battle, before, "refused orders change nothing");
}

#[test]
fn placement_is_instant_and_only_moves_the_placed_formation() {
    let mut battle = deploying();
    let before = battle.clone();
    battle
        .deploy_formation("a1", BattlePoint::new(15_000, 20_000))
        .unwrap();
    assert_placed_in_place(&battle, "a1", BattlePoint::new(15_000, 20_000));
    for other in before.units().iter().filter(|unit| unit.id() != "a1") {
        assert_eq!(
            unit(&battle, other.id()),
            other,
            "{} is untouched",
            other.id()
        );
    }
    // A small shift overlapping only its own previous footprint is legal.
    battle
        .deploy_formation("a1", BattlePoint::new(15_500, 20_500))
        .unwrap();
    assert_placed_in_place(&battle, "a1", BattlePoint::new(15_500, 20_500));
    // Defenders deploy in their own zone under the same rule.
    battle
        .deploy_formation("d1", BattlePoint::new(75_000, 50_000))
        .unwrap();
    assert_placed_in_place(&battle, "d1", BattlePoint::new(75_000, 50_000));
    assert_legal_layout(&battle);
    assert!(matches!(
        battle.deploy_formation("missing", BattlePoint::new(10_000, 10_000)),
        Err(TacticalError::UnitNotFound(_))
    ));
}

#[test]
fn the_whole_footprint_must_lie_inside_the_own_zone() {
    let mut battle = deploying();
    let attacker = zone(&battle, BattleSide::Attacker);
    let (left, right, below, above) = extents(&battle, "a1");
    let max_x = i64::from(attacker.max_x_mm);
    let max_y = i64::from(attacker.max_y_mm);
    assert!(
        right >= 2_000,
        "the fixture footprint is wider than its anchor"
    );

    // Touching the zone edge is legal (zones are inclusive)...
    battle
        .deploy_formation("a1", point(max_x - right, 20_000))
        .unwrap();
    assert_eq!(footprint(&battle, "a1").max_x_mm, max_x);
    assert_legal_layout(&battle);
    // ...one millimetre past it is not, although the anchor stays inside.
    assert_refused(
        &mut battle,
        "a1",
        DeploymentRejection::OutsideZone,
        |battle| battle.deploy_formation("a1", point(max_x - right + 1, 20_000)),
    );
    assert_refused(
        &mut battle,
        "a1",
        DeploymentRejection::OutsideZone,
        |battle| battle.deploy_formation("a1", point(max_x - 1, 20_000)),
    );
    // The same at the far depth edge and the near depth edge.
    battle
        .deploy_formation("a1", point(15_000, max_y - above))
        .unwrap();
    assert_legal_layout(&battle);
    assert_refused(
        &mut battle,
        "a1",
        DeploymentRejection::OutsideZone,
        |battle| battle.deploy_formation("a1", point(15_000, max_y - above + 1)),
    );
    battle.deploy_formation("a1", point(15_000, below)).unwrap();
    assert_legal_layout(&battle);
    assert_refused(
        &mut battle,
        "a1",
        DeploymentRejection::OutsideZone,
        |battle| battle.deploy_formation("a1", point(15_000, below - 1)),
    );
    // And at the near width edge.
    battle.deploy_formation("a1", point(left, 20_000)).unwrap();
    assert_refused(
        &mut battle,
        "a1",
        DeploymentRejection::OutsideZone,
        |battle| battle.deploy_formation("a1", point(left - 1, 20_000)),
    );

    // The neutral centre and the enemy zone are refused outright.
    assert_refused(
        &mut battle,
        "a1",
        DeploymentRejection::OutsideZone,
        |battle| battle.deploy_formation("a1", BattlePoint::new(45_000, 20_000)),
    );
    assert_refused(
        &mut battle,
        "a1",
        DeploymentRejection::OutsideZone,
        |battle| battle.deploy_formation("a1", BattlePoint::new(75_000, 50_000)),
    );
    assert_refused(
        &mut battle,
        "d1",
        DeploymentRejection::OutsideZone,
        |battle| battle.deploy_formation("d1", BattlePoint::new(15_000, 40_000)),
    );
    let defender = zone(&battle, BattleSide::Defender);
    let (d_left, ..) = extents(&battle, "d1");
    assert_refused(
        &mut battle,
        "d1",
        DeploymentRejection::OutsideZone,
        |battle| {
            battle.deploy_formation(
                "d1",
                point(i64::from(defender.min_x_mm) + d_left - 1, 50_000),
            )
        },
    );
}

#[test]
fn friendly_formations_must_not_overlap() {
    let mut battle = deploying();
    let a2 = unit(&battle, "a2").position();
    let (_, _, a1_below, a1_above) = extents(&battle, "a1");
    let (_, _, a2_below, a2_above) = extents(&battle, "a2");
    let a2_y = i64::from(a2.y_mm);

    // Straight onto a friendly formation.
    assert_refused(
        &mut battle,
        "a1",
        DeploymentRejection::FriendlyOverlap,
        |battle| battle.deploy_formation("a1", a2),
    );
    // Partly onto it: the anchors are apart but the footprints intersect.
    assert_refused(
        &mut battle,
        "a1",
        DeploymentRejection::FriendlyOverlap,
        |battle| {
            battle.deploy_formation(
                "a1",
                point(i64::from(a2.x_mm), a2_y - a2_below - a1_above + 500),
            )
        },
    );
    assert_refused(
        &mut battle,
        "a2",
        DeploymentRejection::FriendlyOverlap,
        |battle| {
            let a3 = unit(battle, "a3").position();
            battle.deploy_formation("a2", BattlePoint::new(a3.x_mm + 2_000, a3.y_mm))
        },
    );
    // A metre clear of it is legal.
    battle
        .deploy_formation(
            "a1",
            point(i64::from(a2.x_mm), a2_y + a2_above + 1_000 + a1_below),
        )
        .unwrap();
    assert_legal_layout(&battle);
    // Defenders may not overlap each other either.
    assert_refused(
        &mut battle,
        "d1",
        DeploymentRejection::FriendlyOverlap,
        |battle| {
            let d2 = unit(battle, "d2").position();
            battle.deploy_formation("d1", d2)
        },
    );
    // Outside the zone and overlapping: the zone verdict comes first.
    let mut edge = deploying();
    let attacker = zone(&edge, BattleSide::Attacker);
    let (_, right, ..) = extents(&edge, "a2");
    edge.deploy_formation("a2", point(i64::from(attacker.max_x_mm) - right, 30_000))
        .unwrap();
    let target = unit(&edge, "a2").position();
    assert_refused(
        &mut edge,
        "a1",
        DeploymentRejection::OutsideZone,
        |battle| battle.deploy_formation("a1", BattlePoint::new(target.x_mm + 1_000, target.y_mm)),
    );
}

#[test]
fn frontage_and_formation_changes_obey_the_rule_in_place() {
    // Room all around: a wider frontage is applied in place.
    let mut open = deploying();
    open.deploy_formation("a1", BattlePoint::new(15_000, 20_000))
        .unwrap();
    open.issue_formation_orders(&[FormationOrder::Frontage {
        unit_id: "a1".into(),
        width_mm: 16_000,
    }])
    .unwrap();
    assert_eq!(unit(&open, "a1").formation(), Formation::Line { files: 16 });
    assert_placed_in_place(&open, "a1", BattlePoint::new(15_000, 20_000));
    assert_legal_layout(&open);
    open.issue_formation_order("a1", Formation::Column { files: 16 })
        .unwrap();
    assert_eq!(
        unit(&open, "a1").formation(),
        Formation::Column { files: 16 }
    );
    assert_placed_in_place(&open, "a1", BattlePoint::new(15_000, 20_000));
    assert_legal_layout(&open);

    // In the zone's corner, any wider frontage pushes the footprint out.
    let mut corner = deploying();
    let attacker = zone(&corner, BattleSide::Attacker);
    let (_, right, _, above) = extents(&corner, "a1");
    corner
        .deploy_formation(
            "a1",
            point(
                i64::from(attacker.max_x_mm) - right,
                i64::from(attacker.max_y_mm) - above,
            ),
        )
        .unwrap();
    // a3 sits near the far edge in the fixture; move it out of the way.
    corner
        .deploy_formation("a3", BattlePoint::new(10_000, 40_000))
        .unwrap();
    assert_legal_layout(&corner);
    assert_refused(
        &mut corner,
        "a1",
        DeploymentRejection::OutsideZone,
        |battle| {
            battle.issue_formation_orders(&[FormationOrder::Frontage {
                unit_id: "a1".into(),
                width_mm: 20_000,
            }])
        },
    );
    assert_refused(
        &mut corner,
        "a1",
        DeploymentRejection::OutsideZone,
        |battle| battle.issue_formation_order("a1", Formation::Line { files: 20 }),
    );
    assert_refused(
        &mut corner,
        "a1",
        DeploymentRejection::OutsideZone,
        |battle| battle.issue_formation_order("a1", Formation::Column { files: 10 }),
    );

    // Hemmed in by friends: wider or deeper both overlap a neighbour.
    let mut tight = hemmed();
    assert_refused(
        &mut tight,
        "a1",
        DeploymentRejection::FriendlyOverlap,
        |battle| {
            battle.issue_formation_orders(&[FormationOrder::Frontage {
                unit_id: "a1".into(),
                width_mm: 20_000,
            }])
        },
    );
    assert_refused(
        &mut tight,
        "a1",
        DeploymentRejection::FriendlyOverlap,
        |battle| battle.issue_formation_order("a1", Formation::Column { files: 10 }),
    );

    // A batch with one illegal edit commits nothing.
    let mut batch = deploying();
    let before = batch.clone();
    let corner_point = unit(&corner, "a1").position();
    batch
        .deploy_formation("a3", BattlePoint::new(10_000, 40_000))
        .unwrap();
    batch.deploy_formation("a1", corner_point).unwrap();
    let arranged = batch.clone();
    assert!(
        batch
            .issue_formation_orders(&[
                FormationOrder::Rotate {
                    unit_id: "a2".into(),
                    facing: Facing::new(0, 1).unwrap(),
                },
                FormationOrder::Frontage {
                    unit_id: "a1".into(),
                    width_mm: 20_000,
                },
            ])
            .is_err()
    );
    assert_eq!(batch, arranged, "an atomic batch commits none of its edits");
    assert_ne!(batch, before);
}

#[test]
fn rotation_happens_in_place_and_never_yields_an_illegal_footprint() {
    // Room all around: rotating changes facing only.
    let mut open = deploying();
    open.deploy_formation("a1", BattlePoint::new(15_000, 20_000))
        .unwrap();
    let north = Facing::new(0, -1).unwrap();
    open.issue_formation_orders(&[FormationOrder::Rotate {
        unit_id: "a1".into(),
        facing: north,
    }])
    .unwrap();
    assert_eq!(unit(&open, "a1").facing(), Some(north));
    assert_placed_in_place(&open, "a1", BattlePoint::new(15_000, 20_000));
    open.issue_formation_orders(&[FormationOrder::Turn {
        unit_id: "a1".into(),
        quarter_turns: 2,
    }])
    .unwrap();
    assert_eq!(unit(&open, "a1").facing(), Some(Facing::new(0, 1).unwrap()));
    assert_placed_in_place(&open, "a1", BattlePoint::new(15_000, 20_000));
    assert_legal_layout(&open);

    // At the zone corner and hemmed by friends, every rotation is either
    // applied in place with a legal footprint or refused with no change.
    let mut corner = deploying();
    let attacker = zone(&corner, BattleSide::Attacker);
    let (_, right, _, above) = extents(&corner, "a1");
    corner
        .deploy_formation("a3", BattlePoint::new(10_000, 40_000))
        .unwrap();
    corner
        .deploy_formation(
            "a1",
            point(
                i64::from(attacker.max_x_mm) - right,
                i64::from(attacker.max_y_mm) - above,
            ),
        )
        .unwrap();
    for mut battle in [corner, hemmed()] {
        for facing in [
            Facing::new(0, -1).unwrap(),
            Facing::new(0, 1).unwrap(),
            Facing::west(),
            Facing::east(),
        ] {
            let before = battle.clone();
            let position = unit(&battle, "a1").position();
            match battle.issue_formation_orders(&[FormationOrder::Rotate {
                unit_id: "a1".into(),
                facing,
            }]) {
                Ok(()) => {
                    assert_eq!(unit(&battle, "a1").facing(), Some(facing));
                    assert_placed_in_place(&battle, "a1", position);
                    assert_legal_layout(&battle);
                }
                Err(TacticalError::InvalidDeploymentPlacement { unit_id, .. }) => {
                    assert_eq!(unit_id, "a1");
                    assert_eq!(battle, before);
                }
                Err(other) => panic!("unexpected rotation error {other:?}"),
            }
        }
    }
}

#[test]
fn move_and_face_during_deployment_never_marches_or_breaks_the_rule() {
    let mut battle = deploying();
    let before = battle.clone();
    assert!(
        battle
            .issue_formation_orders(&[FormationOrder::MoveAndFace {
                unit_id: "a1".into(),
                destination: BattlePoint::new(45_000, 20_000),
                facing: None,
            }])
            .is_err()
    );
    assert_eq!(battle, before);
    let destination = BattlePoint::new(15_000, 20_000);
    let facing = Facing::new(0, -1).unwrap();
    match battle.issue_formation_orders(&[FormationOrder::MoveAndFace {
        unit_id: "a1".into(),
        destination,
        facing: Some(facing),
    }]) {
        Ok(()) => {
            assert_placed_in_place(&battle, "a1", destination);
            assert_eq!(unit(&battle, "a1").facing(), Some(facing));
            assert_legal_layout(&battle);
        }
        Err(_) => assert_eq!(battle, before),
    }
}

#[test]
fn entering_deployment_rejects_an_illegal_initial_layout() {
    // The former browser sandbox spears: 28 files at x = 22 m on a 100 m
    // field span 8..36 m, past the attacker zone edge at 33.3 m.
    let sandbox = FlatBattlefield::new(100_000, 100_000);
    let overhanging = TacticalBattle::deploy_at_location(
        sandbox,
        vec![
            TacticalUnit::new(
                "attacker-spears",
                BattleSide::Attacker,
                110,
                BattlePoint::new(22_000, 10_000),
                Formation::Line { files: 28 },
                450,
            ),
            line_unit("defender", BattleSide::Defender, 85_000, 50_000),
        ],
        BattlefieldLocation::ForestClearing,
    )
    .expect("the anchor alone is inside the zone");
    assert_rejected(
        overhanging.into_deployment().map(|_| ()),
        "attacker-spears",
        DeploymentRejection::OutsideZone,
    );

    let overlapping = TacticalBattle::deploy_at_location(
        field(),
        vec![
            line_unit("a1", BattleSide::Attacker, 10_000, 10_000),
            line_unit("a2", BattleSide::Attacker, 12_000, 11_000),
            line_unit("d1", BattleSide::Defender, 80_000, 10_000),
        ],
        BattlefieldLocation::ForestClearing,
    )
    .unwrap();
    match overlapping.into_deployment() {
        Err(TacticalError::InvalidDeploymentPlacement { reason, .. }) => {
            assert_eq!(reason, DeploymentRejection::FriendlyOverlap);
        }
        other => panic!("overlapping initial layout must be rejected, got {other:?}"),
    }

    // Enemy layouts are held to the same rule.
    let enemy_overhang = TacticalBattle::deploy_at_location(
        sandbox,
        vec![
            line_unit("attacker", BattleSide::Attacker, 15_000, 50_000),
            TacticalUnit::new(
                "defender-spears",
                BattleSide::Defender,
                110,
                BattlePoint::new(78_000, 24_000),
                Formation::Line { files: 28 },
                430,
            ),
        ],
        BattlefieldLocation::ForestClearing,
    )
    .unwrap();
    assert_rejected(
        enemy_overhang.into_deployment().map(|_| ()),
        "defender-spears",
        DeploymentRejection::OutsideZone,
    );
}

#[test]
fn campaign_packing_and_siege_layouts_enter_deployment_legally() {
    let mut campaign = new_campaign();
    campaign.move_army("england-main", "paris").unwrap();
    let battle = TacticalBattle::from_campaign_seed(
        FlatBattlefield::new(120_000, 80_000),
        campaign.pending_tactical_battle_seed().unwrap(),
    )
    .unwrap();
    let enemies: Vec<_> = battle
        .units()
        .iter()
        .filter(|unit| unit.side() == BattleSide::Defender)
        .cloned()
        .collect();
    let mut battle = battle
        .into_deployment()
        .expect("campaign packing satisfies the deployment rule");
    assert_eq!(battle.state(), TacticalBattleState::Deployment);
    assert_legal_layout(&battle);

    // Player edits never move the deterministic enemy layout.
    let player = battle
        .units()
        .iter()
        .find(|unit| unit.side() == BattleSide::Attacker)
        .unwrap()
        .id()
        .to_owned();
    let attacker = zone(&battle, BattleSide::Attacker);
    let _ = battle.deploy_formation(
        &player,
        BattlePoint::new(attacker.max_x_mm / 2, attacker.max_y_mm / 2),
    );
    let _ = battle.issue_formation_orders(&[FormationOrder::Turn {
        unit_id: player,
        quarter_turns: 1,
    }]);
    assert_legal_layout(&battle);
    let after: Vec<_> = battle
        .units()
        .iter()
        .filter(|unit| unit.side() == BattleSide::Defender)
        .cloned()
        .collect();
    assert_eq!(after, enemies);

    // Siege battles use the siege layout's zones.
    let mut siege = TacticalBattle::deploy_siege_at_location(
        FlatBattlefield::new(100_000, 100_000),
        vec![
            line_unit("attacker", BattleSide::Attacker, 15_000, 20_000),
            line_unit("defender", BattleSide::Defender, 85_000, 20_000),
        ],
        BattlefieldLocation::ForestClearing,
    )
    .unwrap()
    .into_deployment()
    .unwrap();
    let siege_zone = zone(&siege, BattleSide::Attacker);
    let (_, right, ..) = extents(&siege, "attacker");
    siege
        .deploy_formation(
            "attacker",
            point(i64::from(siege_zone.max_x_mm) - right, 40_000),
        )
        .unwrap();
    assert_legal_layout(&siege);
    assert_refused(
        &mut siege,
        "attacker",
        DeploymentRejection::OutsideZone,
        |battle| {
            battle.deploy_formation(
                "attacker",
                point(i64::from(siege_zone.max_x_mm) - right + 1, 40_000),
            )
        },
    );
}

#[test]
fn begin_battle_locks_deployment_and_starts_exactly_once() {
    let mut battle = deploying();
    battle
        .deploy_formation("a1", BattlePoint::new(15_000, 20_000))
        .unwrap();
    battle
        .issue_formation_orders(&[FormationOrder::Rotate {
            unit_id: "a1".into(),
            facing: Facing::new(0, 1).unwrap(),
        }])
        .unwrap();
    let deployed = battle.clone();

    battle.begin_battle().unwrap();
    assert_eq!(battle.state(), TacticalBattleState::Running);
    assert_eq!(battle.tick(), 0, "starting does not itself advance a tick");
    assert_eq!(
        battle.units(),
        deployed.units(),
        "the deployed layout is tick 0"
    );
    assert!(
        battle.can_withdraw(BattleSide::Attacker),
        "completion rules are active once the battle has begun"
    );

    let started = battle.clone();
    assert!(battle.begin_battle().is_err(), "a second start is refused");
    assert_eq!(battle, started);
    assert!(
        battle
            .deploy_formation("a1", BattlePoint::new(10_000, 20_000))
            .is_err(),
        "deployment is locked after the start"
    );
    assert_eq!(battle, started);

    battle.advance_ticks(3);
    assert_eq!(battle.tick(), 3, "ticks run after the start");
    battle
        .issue_move_order(MovementOrder {
            unit_id: "a1".into(),
            destination: BattlePoint::new(20_000, 20_000),
        })
        .expect("ordinary orders work after the start");
    assert_eq!(
        unit(&battle, "a1").destination(),
        Some(BattlePoint::new(20_000, 20_000))
    );
}

#[test]
fn withdraw_is_allowed_during_deployment() {
    let mut battle = deploying();
    let soldiers: Vec<_> = battle
        .units()
        .iter()
        .filter(|unit| unit.side() == BattleSide::Attacker)
        .map(|unit| (unit.id().to_owned(), unit.soldiers()))
        .collect();
    assert!(battle.can_withdraw(BattleSide::Attacker));
    battle.withdraw(BattleSide::Attacker).unwrap();
    assert_ne!(battle.state(), TacticalBattleState::Deployment);
    assert!(
        battle.begin_battle().is_err(),
        "a withdrawn battle cannot be started"
    );

    for _ in 0..200 {
        if matches!(battle.state(), TacticalBattleState::Finished { .. }) {
            break;
        }
        battle.advance_ticks(100);
    }
    match battle.state() {
        TacticalBattleState::Finished { winner, reason, .. } => {
            assert_eq!(reason, TacticalFinishReason::Withdrawal);
            assert_eq!(winner, Some(BattleSide::Defender));
        }
        other => panic!("withdrawal must finish the battle, got {other:?}"),
    }
    for (id, initial) in soldiers {
        let unit = unit(&battle, &id);
        assert!(unit.is_escaped(), "{id} escaped");
        assert_eq!(unit.soldiers(), initial, "{id} withdrew without losses");
    }
}

#[test]
fn deployment_outcomes_are_deterministic_and_survive_saves() {
    let script = |battle: &mut TacticalBattle| {
        [
            battle.deploy_formation("a1", BattlePoint::new(45_000, 20_000)),
            battle.deploy_formation("a1", BattlePoint::new(15_000, 20_000)),
            battle.deploy_formation("a2", BattlePoint::new(15_000, 21_000)),
            battle.issue_formation_orders(&[FormationOrder::Frontage {
                unit_id: "a3".into(),
                width_mm: 40_000,
            }]),
        ]
        .map(|result| format!("{result:?}"))
    };
    let mut first = deploying();
    let mut second = deploying();
    assert_eq!(script(&mut first), script(&mut second));
    assert_eq!(first, second);

    let encoded = serde_json::to_string(&first).unwrap();
    let mut decoded: TacticalBattle = serde_json::from_str(&encoded).unwrap();
    assert_eq!(decoded, first);
    assert_eq!(decoded.state(), TacticalBattleState::Deployment);
    assert_refused(
        &mut decoded,
        "a1",
        DeploymentRejection::OutsideZone,
        |battle| battle.deploy_formation("a1", BattlePoint::new(45_000, 20_000)),
    );
    decoded.begin_battle().unwrap();
    first.begin_battle().unwrap();
    decoded.advance_ticks(20);
    first.advance_ticks(20);
    assert_eq!(decoded, first);
}
