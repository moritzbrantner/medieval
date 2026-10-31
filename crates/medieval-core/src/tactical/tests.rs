use super::damage::{melee_casualties, pursuit_casualties, ranged_casualties};
use super::error::TacticalError;
use super::geometry::{point_distance_squared, points_within_distance, square_u32};
use super::*;
use crate::deployment::siege::{SiegeBattleState, SiegeGateState};
use crate::deployment::standard_deployment_zones;
use crate::terrain::TacticalTerrain;

fn sample_unit(
    id: &str,
    side: BattleSide,
    position: BattlePoint,
    formation: Formation,
) -> TacticalUnit {
    TacticalUnit::new(id, side, 80, position, formation, 1_200)
}

fn sample_battle() -> TacticalBattle {
    TacticalBattle::deploy(
        FlatBattlefield::new(300_000, 200_000),
        vec![
            sample_unit(
                "attacker-spears",
                BattleSide::Attacker,
                BattlePoint::new(20_000, 100_000),
                Formation::Line { files: 20 },
            ),
            sample_unit(
                "defender-spears",
                BattleSide::Defender,
                BattlePoint::new(280_000, 100_000),
                Formation::Line { files: 20 },
            ),
        ],
    )
    .unwrap()
}

fn contact_battle(attacker_formation: Formation, defender_formation: Formation) -> TacticalBattle {
    TacticalBattle::new(
        FlatBattlefield::new(50_000, 50_000),
        vec![
            sample_unit(
                "attacker",
                BattleSide::Attacker,
                BattlePoint::new(24_500, 25_000),
                attacker_formation,
            ),
            sample_unit(
                "defender",
                BattleSide::Defender,
                BattlePoint::new(25_500, 25_000),
                defender_formation,
            ),
        ],
    )
    .unwrap()
}

fn unit<'a>(battle: &'a TacticalBattle, id: &str) -> &'a TacticalUnit {
    battle.units().iter().find(|unit| unit.id() == id).unwrap()
}

fn engage(battle: &mut TacticalBattle, attacker: &str, defender: &str) {
    battle.issue_engagement_order(attacker, defender).unwrap();
}

#[test]
fn proximity_preserves_tactical_distance_boundaries() {
    let origin = BattlePoint::new(0, 0);
    assert!(points_within_distance(
        origin,
        BattlePoint::new(900, 1_200),
        COMBAT_CONTACT_DISTANCE_MM,
    ));
    assert!(!points_within_distance(
        origin,
        BattlePoint::new(901, 1_200),
        COMBAT_CONTACT_DISTANCE_MM,
    ));
}

#[test]
fn proximity_handles_large_battlefield_coordinates() {
    let edge = BattlePoint::new(u32::MAX, u32::MAX);
    assert!(points_within_distance(
        BattlePoint::new(u32::MAX - 900, u32::MAX - 1_200),
        edge,
        COMBAT_CONTACT_DISTANCE_MM,
    ));
    assert!(!points_within_distance(
        BattlePoint::new(0, 0),
        edge,
        PURSUIT_DISTANCE_MM,
    ));
}

#[test]
fn proximity_is_inclusive_symmetric_and_exact_at_boundaries() {
    let origin = BattlePoint::new(1_000, 1_000);
    // Axial: exactly on the radius, then one millimetre beyond.
    assert!(points_within_distance(
        origin,
        BattlePoint::new(2_500, 1_000),
        1_500
    ));
    assert!(!points_within_distance(
        origin,
        BattlePoint::new(2_501, 1_000),
        1_500
    ));
    assert!(points_within_distance(
        origin,
        BattlePoint::new(1_000, 0),
        1_000
    ));
    assert!(!points_within_distance(
        origin,
        BattlePoint::new(1_000, 0),
        999
    ));
    // Diagonal 3-4-5: on, just inside the axis box but outside the circle.
    assert!(points_within_distance(
        origin,
        BattlePoint::new(4_000, 5_000),
        5_000
    ));
    assert!(!points_within_distance(
        origin,
        BattlePoint::new(4_000, 5_001),
        5_000
    ));
    assert!(!points_within_distance(
        origin,
        BattlePoint::new(4_536, 4_536),
        5_000
    ));
    assert!(points_within_distance(
        origin,
        BattlePoint::new(4_535, 4_535),
        5_000
    ));
    // Reversed order gives the same answer.
    assert!(points_within_distance(
        BattlePoint::new(4_000, 5_000),
        origin,
        5_000
    ));
    assert!(!points_within_distance(
        BattlePoint::new(4_000, 5_001),
        origin,
        5_000
    ));
    // Zero radius admits only coincident points.
    assert!(points_within_distance(origin, origin, 0));
    assert!(!points_within_distance(
        origin,
        BattlePoint::new(1_001, 1_000),
        0
    ));
}

#[test]
fn proximity_handles_full_u32_range_without_overflow() {
    let low = BattlePoint::new(0, 0);
    let high = BattlePoint::new(u32::MAX, u32::MAX);
    assert!(!points_within_distance(low, high, u32::MAX));
    assert!(points_within_distance(
        low,
        BattlePoint::new(u32::MAX, 0),
        u32::MAX
    ));
    assert!(points_within_distance(
        BattlePoint::new(0, u32::MAX),
        low,
        u32::MAX
    ));
    assert!(!points_within_distance(
        low,
        BattlePoint::new(u32::MAX, 1),
        u32::MAX
    ));
    assert!(points_within_distance(high, high, 0));
    assert!(points_within_distance(
        BattlePoint::new(u32::MAX - 3, u32::MAX - 4),
        high,
        5,
    ));
    assert!(!points_within_distance(
        BattlePoint::new(u32::MAX - 3, u32::MAX - 5),
        high,
        5,
    ));
}

#[test]
fn setup_validation_rejects_duplicate_ids_and_invalid_units() {
    let duplicate = TacticalBattle::new(
        FlatBattlefield::new(10_000, 10_000),
        vec![
            sample_unit(
                "spears",
                BattleSide::Attacker,
                BattlePoint::new(1_000, 1_000),
                Formation::Line { files: 10 },
            ),
            sample_unit(
                "spears",
                BattleSide::Defender,
                BattlePoint::new(9_000, 9_000),
                Formation::Line { files: 10 },
            ),
        ],
    );
    assert!(matches!(duplicate, Err(TacticalError::DuplicateUnitId(_))));

    let invalid_formation = TacticalBattle::new(
        FlatBattlefield::new(10_000, 10_000),
        vec![sample_unit(
            "spears",
            BattleSide::Attacker,
            BattlePoint::new(1_000, 1_000),
            Formation::Column { files: 0 },
        )],
    );
    assert!(matches!(
        invalid_formation,
        Err(TacticalError::InvalidFormation(_))
    ));

    let invalid_range = TacticalBattle::new(
        FlatBattlefield::new(10_000, 10_000),
        vec![
            sample_unit(
                "archers",
                BattleSide::Attacker,
                BattlePoint::new(1_000, 1_000),
                Formation::Line { files: 10 },
            )
            .with_attack_range_mm(COMBAT_CONTACT_DISTANCE_MM - 1),
        ],
    );
    assert!(matches!(
        invalid_range,
        Err(TacticalError::InvalidAttackRange { .. })
    ));
}

#[test]
fn setup_rejects_units_on_blocked_river_cells() {
    let battle = TacticalBattle::new(
        FlatBattlefield::new(80_000, 80_000),
        vec![sample_unit(
            "river-unit",
            BattleSide::Attacker,
            BattlePoint::new(35_000, 15_000),
            Formation::Line { files: 10 },
        )],
    );
    assert!(matches!(
        battle,
        Err(TacticalError::UnitOnImpassableTerrain { .. })
    ));
}

#[test]
fn deployment_validation_accepts_own_back_thirds_and_rejects_neutral_setup() {
    let battlefield = FlatBattlefield::new(90_000, 60_000);
    let deployed = TacticalBattle::deploy(
        battlefield,
        vec![
            sample_unit(
                "attacker",
                BattleSide::Attacker,
                BattlePoint::new(30_000, 20_000),
                Formation::Line { files: 10 },
            ),
            sample_unit(
                "defender",
                BattleSide::Defender,
                BattlePoint::new(60_000, 40_000),
                Formation::Line { files: 10 },
            ),
        ],
    )
    .unwrap();
    assert_eq!(
        deployed.deployment_zones(),
        standard_deployment_zones(battlefield)
    );

    let invalid = TacticalBattle::deploy(
        battlefield,
        vec![sample_unit(
            "attacker",
            BattleSide::Attacker,
            BattlePoint::new(45_000, 30_000),
            Formation::Line { files: 10 },
        )],
    );
    assert!(matches!(
        invalid,
        Err(TacticalError::UnitOutsideDeploymentZone {
            side: BattleSide::Attacker,
            ..
        })
    ));
}

#[test]
fn siege_deployment_uses_siege_zones_and_roundtrips() {
    let battlefield = FlatBattlefield::new(100_000, 100_000);
    let battle = TacticalBattle::deploy_siege(
        battlefield,
        vec![
            sample_unit(
                "attacker",
                BattleSide::Attacker,
                BattlePoint::new(20_000, 50_000),
                Formation::Line { files: 10 },
            ),
            sample_unit(
                "defender",
                BattleSide::Defender,
                BattlePoint::new(80_000, 50_000),
                Formation::Line { files: 10 },
            ),
        ],
    )
    .unwrap();
    let [attacker_zone, defender_zone] = battle.deployment_zones();
    assert_eq!(attacker_zone.max_x_mm, 35_000);
    assert_eq!(defender_zone.min_x_mm, 65_000);
    assert!(battle.siege.is_some());
    assert_eq!(battle.siege_snapshot(), battle.siege);

    let encoded = serde_json::to_string(&battle).unwrap();
    let decoded: TacticalBattle = serde_json::from_str(&encoded).unwrap();
    assert_eq!(decoded, battle);

    let invalid = TacticalBattle::deploy_siege(
        battlefield,
        vec![sample_unit(
            "attacker",
            BattleSide::Attacker,
            BattlePoint::new(40_000, 50_000),
            Formation::Line { files: 10 },
        )],
    );
    assert!(matches!(
        invalid,
        Err(TacticalError::UnitOutsideDeploymentZone { .. })
    ));
}

#[test]
fn field_battle_serialization_has_no_siege_drift() {
    let battle = sample_battle();
    let encoded = serde_json::to_value(&battle).unwrap();
    assert!(encoded.get("siege").is_none());
    assert!(battle.siege_snapshot().is_none());

    let decoded: TacticalBattle = serde_json::from_value(encoded).unwrap();
    assert_eq!(decoded, battle);
    assert!(decoded.siege.is_none());
}

#[test]
fn closed_siege_gate_holds_orders_until_core_opens_it() {
    let battlefield = FlatBattlefield::new(80_000, 80_000);
    let destination = BattlePoint::new(70_000, 40_000);
    let mut battle = TacticalBattle::deploy_siege(
        battlefield,
        vec![sample_unit(
            "attacker",
            BattleSide::Attacker,
            BattlePoint::new(10_000, 40_000),
            Formation::Line { files: 10 },
        )],
    )
    .unwrap();
    battle
        .issue_move_order(MovementOrder {
            unit_id: "attacker".into(),
            destination,
        })
        .unwrap();

    battle.advance_ticks(200);
    let closed_position = unit(&battle, "attacker").position();
    let gate = battle.siege.unwrap().layout.gate;
    assert!(closed_position.x_mm < gate.min_x_mm);
    assert!(unit(&battle, "attacker").destination().is_some());

    battle.open_siege_gate().unwrap();
    battle.advance_ticks(200);
    assert_eq!(unit(&battle, "attacker").position(), destination);
    assert!(unit(&battle, "attacker").destination().is_none());
}

#[test]
fn field_battle_loads_drop_pending_ladder_orders() {
    let battlefield = FlatBattlefield::new(100_000, 100_000);
    let battle = TacticalBattle::new(
        battlefield,
        vec![sample_unit(
            "attacker",
            BattleSide::Attacker,
            BattlePoint::new(10_000, 20_000),
            Formation::Line { files: 10 },
        )],
    )
    .unwrap();
    // A field battle document carrying a pending ladder order, as a siege save
    // stripped of its siege would.
    let mut encoded = serde_json::to_value(&battle).unwrap();
    encoded["units"][0]["ladderOrder"] =
        serde_json::to_value(BattlePoint::new(50_000, 20_000)).unwrap();
    let field: TacticalBattle = serde_json::from_value(encoded).unwrap();
    assert_eq!(unit(&field, "attacker").ladder_order(), None);
    let reencoded = serde_json::to_string(&field).unwrap();
    assert!(!reencoded.contains("ladderOrder"));
}

#[test]
fn destroyed_siege_gate_is_terminal() {
    let battlefield = FlatBattlefield::new(100_000, 100_000);
    let mut siege = SiegeBattleState::test_siege(battlefield);
    siege.gate_state = SiegeGateState::Closed;
    let mut battle = TacticalBattle::new(
        battlefield,
        vec![sample_unit(
            "attacker",
            BattleSide::Attacker,
            siege.layout.capture_point.center,
            Formation::Line { files: 10 },
        )],
    )
    .unwrap();
    battle.siege = Some(siege);
    battle.destroy_siege_gate().unwrap();
    for result in [
        battle.close_siege_gate(),
        battle.open_siege_gate(),
        battle.destroy_siege_gate(),
    ] {
        assert!(matches!(result, Err(TacticalError::SiegeGateDestroyed)));
    }
    let siege = battle.siege.unwrap();
    assert_eq!(siege.gate_state, SiegeGateState::Destroyed);
    assert_eq!(siege.gate_integrity, 0);
}

#[test]
fn siege_capture_advances_on_combat_pulses_and_contention_fails_closed() {
    let battlefield = FlatBattlefield::new(100_000, 100_000);
    let mut siege = SiegeBattleState::test_siege(battlefield);
    siege.gate_state = SiegeGateState::Open;
    let point = siege.layout.capture_point.center;
    let mut uncontested = TacticalBattle::new(
        battlefield,
        vec![sample_unit(
            "attacker",
            BattleSide::Attacker,
            point,
            Formation::Line { files: 10 },
        )],
    )
    .unwrap();
    uncontested.siege = Some(siege);
    uncontested.advance_ticks(TACTICAL_TICKS_PER_SECOND * 10);
    assert_eq!(
        uncontested.siege.unwrap().capture.captured_by,
        Some(BattleSide::Attacker)
    );

    let mut contested = TacticalBattle::new(
        battlefield,
        vec![
            sample_unit(
                "attacker",
                BattleSide::Attacker,
                point,
                Formation::Line { files: 10 },
            ),
            sample_unit(
                "defender",
                BattleSide::Defender,
                point,
                Formation::Line { files: 10 },
            ),
        ],
    )
    .unwrap();
    contested.siege = Some(siege);
    contested.advance_ticks(TACTICAL_TICKS_PER_SECOND);
    assert_eq!(contested.siege.unwrap().capture.progress, 0);
}

#[test]
fn forest_ground_cover_reduces_tick_movement_without_changing_orders() {
    let battlefield = FlatBattlefield::new(80_000, 80_000);
    let destination = BattlePoint::new(29_000, 15_000);
    let mut forest = TacticalBattle::new(
        battlefield,
        vec![sample_unit(
            "forest",
            BattleSide::Attacker,
            BattlePoint::new(25_000, 15_000),
            Formation::Line { files: 10 },
        )],
    )
    .unwrap();
    forest
        .issue_move_order(MovementOrder {
            unit_id: "forest".into(),
            destination,
        })
        .unwrap();
    forest.advance_ticks(1);
    assert_eq!(
        unit(&forest, "forest").position(),
        BattlePoint::new(25_600, 15_000)
    );
    assert_eq!(unit(&forest, "forest").destination(), Some(destination));

    let mut open = TacticalBattle::new(
        battlefield,
        vec![sample_unit(
            "open",
            BattleSide::Attacker,
            BattlePoint::new(5_000, 15_000),
            Formation::Line { files: 10 },
        )],
    )
    .unwrap();
    open.issue_move_order(MovementOrder {
        unit_id: "open".into(),
        destination: BattlePoint::new(15_000, 15_000),
    })
    .unwrap();
    open.advance_ticks(1);
    assert_eq!(
        unit(&open, "open").position(),
        BattlePoint::new(6_200, 15_000)
    );
}

#[test]
fn rejected_orders_do_not_mutate_authoritative_state() {
    let mut battle = sample_battle();
    let before = battle.clone();
    assert!(matches!(
        battle.issue_move_order(MovementOrder {
            unit_id: "missing".into(),
            destination: BattlePoint::new(50_000, 50_000),
        }),
        Err(TacticalError::UnitNotFound(_))
    ));
    assert_eq!(battle, before);

    assert!(matches!(
        battle.issue_move_order(MovementOrder {
            unit_id: "attacker-spears".into(),
            destination: BattlePoint::new(300_001, 50_000),
        }),
        Err(TacticalError::DestinationOutOfBounds { .. })
    ));
    assert_eq!(battle, before);

    assert!(matches!(
        battle.issue_move_order(MovementOrder {
            unit_id: "attacker-spears".into(),
            destination: BattlePoint::new(130_000, 25_000),
        }),
        Err(TacticalError::DestinationImpassable { .. })
    ));
    assert_eq!(battle, before);
}

#[test]
fn move_orders_route_through_crossing_without_entering_blocked_water() {
    let battlefield = FlatBattlefield::new(80_000, 80_000);
    let destination = BattlePoint::new(70_000, 10_000);
    let mut battle = TacticalBattle::new(
        battlefield,
        vec![sample_unit(
            "crossing",
            BattleSide::Attacker,
            BattlePoint::new(10_000, 10_000),
            Formation::Line { files: 10 },
        )],
    )
    .unwrap();
    battle
        .issue_move_order(MovementOrder {
            unit_id: "crossing".into(),
            destination,
        })
        .unwrap();

    let mut visited_crossing = false;
    for _ in 0..200 {
        battle.advance_ticks(1);
        let position = unit(&battle, "crossing").position();
        assert!(battle.terrain().is_passable_at(battlefield, position));
        if battle.terrain().river_crossing_cells().iter().any(|cell| {
            battle
                .terrain()
                .cell_bounds_mm(battlefield, cell.cell_x, cell.cell_z)
                .is_some_and(|(x0, x1, z0, z1)| {
                    position.x_mm >= x0
                        && position.x_mm < x1
                        && position.y_mm >= z0
                        && position.y_mm < z1
                })
        }) {
            visited_crossing = true;
        }
        if unit(&battle, "crossing").destination().is_none() {
            break;
        }
    }
    assert!(visited_crossing);
    assert_eq!(unit(&battle, "crossing").position(), destination);
}

#[test]
fn engagement_across_river_uses_the_same_crossing_path() {
    let battlefield = FlatBattlefield::new(80_000, 80_000);
    let mut battle = TacticalBattle::new(
        battlefield,
        vec![
            sample_unit(
                "attacker",
                BattleSide::Attacker,
                BattlePoint::new(10_000, 10_000),
                Formation::Line { files: 10 },
            ),
            sample_unit(
                "defender",
                BattleSide::Defender,
                BattlePoint::new(70_000, 10_000),
                Formation::Line { files: 10 },
            ),
        ],
    )
    .unwrap();
    engage(&mut battle, "attacker", "defender");

    let mut visited_crossing = false;
    for _ in 0..100 {
        battle.advance_ticks(1);
        let position = unit(&battle, "attacker").position();
        assert!(battle.terrain().is_passable_at(battlefield, position));
        if position.x_mm >= 30_000 && position.x_mm < 40_000 && position.y_mm >= 30_000 {
            visited_crossing = true;
            break;
        }
    }
    assert!(visited_crossing);
}

#[test]
fn formation_orders_change_core_formation_without_replacing_other_orders() {
    let mut battle = sample_battle();
    battle
        .issue_move_order(MovementOrder {
            unit_id: "attacker-spears".into(),
            destination: BattlePoint::new(80_000, 120_000),
        })
        .unwrap();
    battle
        .issue_formation_order("attacker-spears", Formation::Column { files: 20 })
        .unwrap();
    assert_eq!(
        unit(&battle, "attacker-spears").formation(),
        Formation::Column { files: 20 }
    );
    assert_eq!(
        unit(&battle, "attacker-spears").destination(),
        Some(BattlePoint::new(80_000, 120_000))
    );
}

#[test]
fn ranged_units_hold_at_range_and_inflict_losses_before_contact() {
    let battlefield = FlatBattlefield::new(50_000, 50_000);
    let mut battle = TacticalBattle::new(
        battlefield,
        vec![
            sample_unit(
                "archers",
                BattleSide::Attacker,
                BattlePoint::new(10_000, 25_000),
                Formation::Line { files: 24 },
            )
            .with_attack_range_mm(20_000),
            sample_unit(
                "defender",
                BattleSide::Defender,
                BattlePoint::new(25_000, 25_000),
                Formation::Line { files: 20 },
            ),
        ],
    )
    .unwrap();
    engage(&mut battle, "archers", "defender");
    let start = unit(&battle, "archers").position();
    battle.advance_ticks(TACTICAL_TICKS_PER_SECOND);
    assert_eq!(unit(&battle, "archers").position(), start);
    assert_eq!(unit(&battle, "archers").attack_range_mm(), 20_000);
    assert!(unit(&battle, "defender").soldiers() < 80);
    assert!(!points_within_distance(
        unit(&battle, "archers").position(),
        unit(&battle, "defender").position(),
        COMBAT_CONTACT_DISTANCE_MM,
    ));
}

#[test]
fn higher_ground_increases_melee_and_pursuit_effectiveness() {
    let terrain = TacticalTerrain::battlefield_foundation();
    let battlefield = FlatBattlefield::new(100_000, 100_000);
    let high = sample_unit(
        "high",
        BattleSide::Attacker,
        BattlePoint::new(55_000, 55_000),
        Formation::Line { files: 80 },
    );
    let low = sample_unit(
        "low",
        BattleSide::Defender,
        BattlePoint::new(5_000, 5_000),
        Formation::Line { files: 80 },
    );

    assert!(
        melee_casualties(&high, &low, 80, terrain, battlefield)
            > melee_casualties(&low, &high, 80, terrain, battlefield)
    );
    assert!(
        pursuit_casualties(&high, &low, terrain, battlefield)
            > pursuit_casualties(&low, &high, terrain, battlefield)
    );
}

#[test]
fn forest_cover_reduces_ranged_losses_at_equal_elevation() {
    let terrain = TacticalTerrain::battlefield_foundation();
    let battlefield = FlatBattlefield::new(80_000, 80_000);
    let archers = sample_unit(
        "archers",
        BattleSide::Attacker,
        BattlePoint::new(15_000, 5_000),
        Formation::Line { files: 48 },
    );
    let open = sample_unit(
        "open",
        BattleSide::Defender,
        BattlePoint::new(15_000, 15_000),
        Formation::Line { files: 20 },
    );
    let forest = sample_unit(
        "forest",
        BattleSide::Defender,
        BattlePoint::new(25_000, 15_000),
        Formation::Line { files: 20 },
    );
    assert_eq!(
        terrain.height_mm(battlefield, open.position),
        terrain.height_mm(battlefield, forest.position)
    );
    assert!(
        ranged_casualties(&archers, &forest, terrain, battlefield)
            < ranged_casualties(&archers, &open, terrain, battlefield)
    );
}

#[test]
fn legacy_units_without_attack_range_default_to_melee_contact() {
    let battle = sample_battle();
    let mut encoded = serde_json::to_value(&battle).unwrap();
    for unit in encoded["units"].as_array_mut().unwrap() {
        unit.as_object_mut().unwrap().remove("attackRangeMm");
    }
    let decoded: TacticalBattle = serde_json::from_value(encoded).unwrap();
    assert!(
        decoded
            .units()
            .iter()
            .all(|unit| unit.attack_range_mm() == COMBAT_CONTACT_DISTANCE_MM)
    );
}

#[test]
fn fixed_tick_replay_is_deterministic_and_serializable() {
    let mut first = sample_battle();
    let mut second = sample_battle();
    let order = MovementOrder {
        unit_id: "attacker-spears".into(),
        destination: BattlePoint::new(100_000, 140_000),
    };
    first.issue_move_order(order.clone()).unwrap();
    second.issue_move_order(order).unwrap();
    first.advance_ticks(TACTICAL_TICKS_PER_SECOND * 7);
    second.advance_ticks(TACTICAL_TICKS_PER_SECOND * 7);
    assert_eq!(first.tick(), 140);
    assert_eq!(first, second);

    let encoded = serde_json::to_string(&first).unwrap();
    let decoded: TacticalBattle = serde_json::from_str(&encoded).unwrap();
    assert_eq!(decoded, first);
}

#[test]
fn deserialization_restores_canonical_order_for_unit_lookup() {
    let battle = sample_battle();
    let mut encoded = serde_json::to_value(&battle).unwrap();
    encoded["units"].as_array_mut().unwrap().reverse();

    let mut decoded: TacticalBattle = serde_json::from_value(encoded).unwrap();
    decoded
        .issue_engagement_order("attacker-spears", "defender-spears")
        .unwrap();

    assert_eq!(decoded.units()[0].id(), "attacker-spears");
    assert_eq!(
        unit(&decoded, "attacker-spears").engagement_target(),
        Some("defender-spears")
    );
}

#[test]
fn legacy_battle_without_terrain_preserves_height_only_v1_semantics() {
    let battle = sample_battle();
    let mut encoded = serde_json::to_value(&battle).unwrap();
    encoded.as_object_mut().unwrap().remove("terrain");

    let decoded: TacticalBattle = serde_json::from_value(encoded).unwrap();
    assert_eq!(
        decoded.terrain().profile(),
        crate::terrain::TacticalTerrainProfile::HeightFoundationV1
    );
    assert!(decoded.terrain().forest_cells().is_empty());
    assert!(decoded.terrain().river_cells().is_empty());
    assert!(decoded.siege.is_none());
}

#[test]
fn movement_never_exceeds_speed_and_arrives_exactly() {
    let mut battle = sample_battle();
    let destination = BattlePoint::new(90_000, 150_000);
    battle
        .issue_move_order(MovementOrder {
            unit_id: "attacker-spears".into(),
            destination,
        })
        .unwrap();

    for _ in 0..200 {
        let before = unit(&battle, "attacker-spears").position();
        let speed = unit(&battle, "attacker-spears").speed_mm_per_tick();
        battle.advance_ticks(1);
        let after = unit(&battle, "attacker-spears").position();
        assert!(point_distance_squared(before, after) <= square_u32(speed));
        if unit(&battle, "attacker-spears").destination().is_none() {
            break;
        }
    }
    assert_eq!(unit(&battle, "attacker-spears").position(), destination);
}

#[test]
fn extreme_diagonal_movement_still_respects_speed_bound() {
    let speed = 1_000_000;
    let mut battle = TacticalBattle::new(
        FlatBattlefield::new(u32::MAX, u32::MAX),
        vec![TacticalUnit::new(
            "scouts",
            BattleSide::Attacker,
            20,
            BattlePoint::new(0, 0),
            Formation::Column { files: 4 },
            speed,
        )],
    )
    .unwrap();
    battle
        .issue_move_order(MovementOrder {
            unit_id: "scouts".into(),
            destination: BattlePoint::new(u32::MAX, u32::MAX),
        })
        .unwrap();
    let before = unit(&battle, "scouts").position();
    battle.advance_ticks(1);
    let after = unit(&battle, "scouts").position();
    assert!(point_distance_squared(before, after) <= square_u32(speed));
}

#[test]
fn storage_order_is_normalized_and_combat_is_order_independent() {
    let attacker = sample_unit(
        "attacker",
        BattleSide::Attacker,
        BattlePoint::new(10_000, 10_000),
        Formation::Line { files: 20 },
    );
    let defender = sample_unit(
        "defender",
        BattleSide::Defender,
        BattlePoint::new(11_000, 10_000),
        Formation::Line { files: 20 },
    );
    let mut first = TacticalBattle::new(
        FlatBattlefield::new(50_000, 50_000),
        vec![attacker.clone(), defender.clone()],
    )
    .unwrap();
    let mut second = TacticalBattle::new(
        FlatBattlefield::new(50_000, 50_000),
        vec![defender, attacker],
    )
    .unwrap();
    engage(&mut first, "attacker", "defender");
    engage(&mut second, "attacker", "defender");
    first.advance_ticks(TACTICAL_TICKS_PER_SECOND * 5);
    second.advance_ticks(TACTICAL_TICKS_PER_SECOND * 5);
    assert_eq!(first, second);
}

#[test]
fn line_frontage_inflicts_more_losses_than_column_frontage() {
    let mut line = contact_battle(Formation::Line { files: 24 }, Formation::Line { files: 20 });
    let mut column = contact_battle(
        Formation::Column { files: 24 },
        Formation::Line { files: 20 },
    );
    engage(&mut line, "attacker", "defender");
    engage(&mut column, "attacker", "defender");
    line.advance_ticks(TACTICAL_TICKS_PER_SECOND);
    column.advance_ticks(TACTICAL_TICKS_PER_SECOND);
    assert!(unit(&line, "defender").soldiers() < unit(&column, "defender").soldiers());
}

#[test]
fn simultaneous_opponents_share_one_frontage_budget() {
    let attacker = sample_unit(
        "attacker",
        BattleSide::Attacker,
        BattlePoint::new(25_000, 25_000),
        Formation::Line { files: 40 },
    );
    let first_defender = sample_unit(
        "defender-a",
        BattleSide::Defender,
        BattlePoint::new(24_000, 25_000),
        Formation::Line { files: 20 },
    );
    let second_defender = sample_unit(
        "defender-b",
        BattleSide::Defender,
        BattlePoint::new(26_000, 25_000),
        Formation::Line { files: 20 },
    );
    let battlefield = FlatBattlefield::new(50_000, 50_000);
    let mut single =
        TacticalBattle::new(battlefield, vec![attacker.clone(), first_defender.clone()]).unwrap();
    let mut surrounded =
        TacticalBattle::new(battlefield, vec![attacker, first_defender, second_defender]).unwrap();
    engage(&mut single, "attacker", "defender-a");
    engage(&mut surrounded, "attacker", "defender-a");
    engage(&mut surrounded, "defender-b", "attacker");

    single.advance_ticks(TACTICAL_TICKS_PER_SECOND);
    surrounded.advance_ticks(TACTICAL_TICKS_PER_SECOND);

    let single_losses = 80 - unit(&single, "defender-a").soldiers();
    let combined_losses = 160
        - unit(&surrounded, "defender-a").soldiers()
        - unit(&surrounded, "defender-b").soldiers();
    assert!(combined_losses <= single_losses);
    assert!(unit(&surrounded, "defender-a").soldiers() < 80);
    assert!(unit(&surrounded, "defender-b").soldiers() < 80);
}

#[test]
fn fatigue_rises_and_reduces_melee_effectiveness() {
    let mut fresh = contact_battle(Formation::Line { files: 40 }, Formation::Line { files: 20 });
    let mut tired = fresh.clone();
    let tired_index = tired.unit_index("attacker").unwrap();
    tired.units[tired_index].fatigue = 800;
    engage(&mut fresh, "attacker", "defender");
    engage(&mut tired, "attacker", "defender");
    fresh.advance_ticks(TACTICAL_TICKS_PER_SECOND);
    tired.advance_ticks(TACTICAL_TICKS_PER_SECOND);
    assert!(unit(&fresh, "defender").soldiers() < unit(&tired, "defender").soldiers());
    assert!(unit(&fresh, "attacker").fatigue() > 0);
}

#[test]
fn casualties_apply_morale_shocks_and_eventually_route_units() {
    let mut battle = contact_battle(
        Formation::Line { files: 60 },
        Formation::Column { files: 4 },
    );
    engage(&mut battle, "attacker", "defender");
    for _ in 0..60 {
        battle.advance_ticks(TACTICAL_TICKS_PER_SECOND);
        if unit(&battle, "defender").is_routed() || unit(&battle, "defender").is_destroyed() {
            break;
        }
    }
    let defender = unit(&battle, "defender");
    assert!(defender.soldiers() < 80);
    assert!(defender.morale() < MAX_TACTICAL_MORALE);
    assert!(defender.is_routed() || defender.is_destroyed());
}

#[test]
fn routed_units_reject_orders_and_move_away_from_enemy() {
    let mut battle = contact_battle(Formation::Line { files: 20 }, Formation::Line { files: 20 });
    let defender_index = battle.unit_index("defender").unwrap();
    battle.units[defender_index].state = TacticalUnitState::Routed;
    battle.units[defender_index].morale = ROUT_MORALE_THRESHOLD;
    let before = battle.units[defender_index].position;
    assert!(matches!(
        battle.issue_move_order(MovementOrder {
            unit_id: "defender".into(),
            destination: BattlePoint::new(40_000, 40_000),
        }),
        Err(TacticalError::UnitCannotReceiveOrders { .. })
    ));
    battle.advance_ticks(1);
    let after = unit(&battle, "defender").position();
    assert!(battle.terrain().is_passable_at(battle.battlefield(), after));
    assert!(
        point_distance_squared(after, BattlePoint::new(24_500, 25_000))
            > point_distance_squared(before, BattlePoint::new(24_500, 25_000))
    );
}

#[test]
fn pursuit_causes_bounded_losses_while_routed_target_is_close() {
    let mut battle = contact_battle(Formation::Line { files: 24 }, Formation::Line { files: 20 });
    engage(&mut battle, "attacker", "defender");
    let defender_index = battle.unit_index("defender").unwrap();
    battle.units[defender_index].state = TacticalUnitState::Routed;
    battle.units[defender_index].morale = ROUT_MORALE_THRESHOLD;
    let before = battle.units[defender_index].soldiers;
    battle.advance_ticks(TACTICAL_TICKS_PER_SECOND);
    let after = unit(&battle, "defender").soldiers();
    assert!(after < before);
    assert!(before - after <= unit(&battle, "attacker").frontage_slots());
}

#[test]
fn zero_soldiers_destroy_unit_without_underflow_or_resurrection() {
    let mut battle = contact_battle(
        Formation::Line { files: 80 },
        Formation::Column { files: 2 },
    );
    let defender_index = battle.unit_index("defender").unwrap();
    battle.units[defender_index].soldiers = 1;
    engage(&mut battle, "attacker", "defender");
    battle.advance_ticks(TACTICAL_TICKS_PER_SECOND);
    let defender = unit(&battle, "defender");
    assert_eq!(defender.soldiers(), 0);
    assert!(defender.is_destroyed());
    battle.advance_ticks(TACTICAL_TICKS_PER_SECOND * 5);
    assert_eq!(unit(&battle, "defender").soldiers(), 0);
}

#[test]
fn movement_preserves_identity_side_formation_and_soldier_count() {
    let mut battle = sample_battle();
    let before = unit(&battle, "attacker-spears").clone();
    battle
        .issue_move_order(MovementOrder {
            unit_id: "attacker-spears".into(),
            destination: BattlePoint::new(80_000, 120_000),
        })
        .unwrap();
    battle.advance_ticks(10);
    let after = unit(&battle, "attacker-spears");
    assert_eq!(after.id(), before.id());
    assert_eq!(after.side(), before.side());
    assert_eq!(after.formation(), before.formation());
    assert_eq!(after.soldiers(), before.soldiers());
    assert_eq!(after.attack_range_mm(), before.attack_range_mm());
    assert_ne!(after.position(), before.position());
}
