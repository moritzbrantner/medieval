use medieval_core::{
    BattlePoint, BattleSide, BattlefieldLocation, FlatBattlefield, Formation, GroupMovementOrder,
    MAX_QUEUED_WAYPOINTS, MovementMode, MovementOrder, TacticalBattle, TacticalUnit,
};
fn battle() -> TacticalBattle {
    TacticalBattle::new_at_location(
        FlatBattlefield::new(200_000, 100_000),
        vec![
            TacticalUnit::new(
                "a",
                BattleSide::Attacker,
                80,
                BattlePoint::new(30_000, 20_000),
                Formation::Line { files: 10 },
                1000,
            ),
            TacticalUnit::new(
                "b",
                BattleSide::Attacker,
                80,
                BattlePoint::new(50_000, 30_000),
                Formation::Column { files: 10 },
                1000,
            ),
            TacticalUnit::new(
                "d",
                BattleSide::Defender,
                80,
                BattlePoint::new(180_000, 80_000),
                Formation::Line { files: 10 },
                1000,
            ),
        ],
        BattlefieldLocation::ForestClearing,
    )
    .unwrap()
}
fn unit<'a>(battle: &'a TacticalBattle, id: &str) -> &'a TacticalUnit {
    battle.units().iter().find(|unit| unit.id() == id).unwrap()
}
fn group(ids: &[&str], x: u32, y: u32, queued: bool, mode: MovementMode) -> GroupMovementOrder {
    GroupMovementOrder {
        unit_ids: ids.iter().map(|id| (*id).into()).collect(),
        destination: BattlePoint::new(x, y),
        queued,
        mode,
    }
}
#[test]
fn mixed_formations_keep_their_relative_layout() {
    let mut battle = battle();
    battle
        .issue_group_movement(group(
            &["b", "a"],
            80_000,
            40_000,
            false,
            MovementMode::March,
        ))
        .unwrap();
    assert_eq!(
        unit(&battle, "a").destination(),
        Some(BattlePoint::new(70_000, 35_000))
    );
    assert_eq!(
        unit(&battle, "b").destination(),
        Some(BattlePoint::new(90_000, 45_000))
    );
}
#[test]
fn rejected_group_placement_rolls_back_every_unit() {
    let mut battle = battle();
    let before = battle.clone();
    assert!(
        battle
            .issue_group_movement(group(
                &["a", "b"],
                12_000,
                20_000,
                false,
                MovementMode::March
            ))
            .is_err()
    );
    assert_eq!(battle, before);
}
#[test]
fn group_membership_rejects_empty_duplicate_hostile_and_unknown_units() {
    let mut battle = battle();
    let before = battle.clone();
    for ids in [vec![], vec!["a", "a"], vec!["a", "d"], vec!["a", "missing"]] {
        assert!(
            battle
                .issue_group_movement(group(&ids, 80_000, 40_000, false, MovementMode::March))
                .is_err()
        );
        assert_eq!(battle, before);
    }
}
#[test]
fn queued_groups_use_planned_positions_and_preserve_current_destinations() {
    let mut battle = battle();
    battle
        .issue_group_movement(group(
            &["a", "b"],
            80_000,
            40_000,
            false,
            MovementMode::March,
        ))
        .unwrap();
    let before = unit(&battle, "a").destination();
    battle.advance_ticks(5);
    battle
        .issue_group_movement(group(
            &["b", "a"],
            100_000,
            60_000,
            true,
            MovementMode::AttackMove,
        ))
        .unwrap();
    assert_eq!(unit(&battle, "a").destination(), before);
    assert_eq!(
        unit(&battle, "a").queued_movements()[0].destination,
        BattlePoint::new(90_000, 55_000)
    );
    assert_eq!(
        unit(&battle, "b").queued_movements()[0].destination,
        BattlePoint::new(110_000, 65_000)
    );
}
#[test]
fn queue_limit_is_bounded_and_atomic() {
    let mut battle = battle();
    battle
        .issue_group_movement(group(
            &["a", "b"],
            80_000,
            40_000,
            false,
            MovementMode::March,
        ))
        .unwrap();
    for _ in 0..MAX_QUEUED_WAYPOINTS {
        battle
            .issue_group_movement(group(
                &["a", "b"],
                100_000,
                60_000,
                true,
                MovementMode::March,
            ))
            .unwrap();
    }
    let before = battle.clone();
    assert!(
        battle
            .issue_group_movement(group(
                &["a", "b"],
                80_000,
                40_000,
                true,
                MovementMode::March
            ))
            .is_err()
    );
    assert_eq!(battle, before);
}
#[test]
fn stop_clears_current_queue_and_attack_move_state() {
    let mut battle = battle();
    battle
        .issue_group_movement(group(
            &["a"],
            80_000,
            40_000,
            false,
            MovementMode::AttackMove,
        ))
        .unwrap();
    battle
        .issue_group_movement(group(&["a"], 90_000, 40_000, true, MovementMode::March))
        .unwrap();
    let position = unit(&battle, "a").position();
    battle
        .issue_move_order(MovementOrder {
            unit_id: "a".into(),
            destination: position,
        })
        .unwrap();
    assert_eq!(unit(&battle, "a").destination(), None);
    assert_eq!(unit(&battle, "a").engagement_target(), None);
    assert_eq!(unit(&battle, "a").movement_mode(), MovementMode::March);
    assert!(unit(&battle, "a").queued_movements().is_empty());
}
#[test]
fn waypoint_queue_survives_save_and_replay() {
    let mut first = battle();
    first
        .issue_group_movement(group(&["a"], 35_000, 20_000, false, MovementMode::March))
        .unwrap();
    first
        .issue_group_movement(group(&["a"], 45_000, 20_000, true, MovementMode::March))
        .unwrap();
    first.advance_ticks(2);
    let mut restored: TacticalBattle =
        serde_json::from_str(&serde_json::to_string(&first).unwrap()).unwrap();
    first.advance_ticks(30);
    restored.advance_ticks(30);
    assert_eq!(first, restored);
    assert_eq!(
        unit(&restored, "a").position(),
        BattlePoint::new(45_000, 20_000)
    );
    assert!(unit(&restored, "a").queued_movements().is_empty());
}
fn contact_battle(reversed: bool) -> TacticalBattle {
    let mut units = vec![
        TacticalUnit::new(
            "a",
            BattleSide::Attacker,
            80,
            BattlePoint::new(100_000, 50_000),
            Formation::Line { files: 10 },
            1000,
        ),
        TacticalUnit::new(
            "d-a",
            BattleSide::Defender,
            80,
            BattlePoint::new(104_000, 50_000),
            Formation::Line { files: 10 },
            1000,
        ),
        TacticalUnit::new(
            "d-z",
            BattleSide::Defender,
            80,
            BattlePoint::new(100_000, 54_000),
            Formation::Line { files: 10 },
            1000,
        ),
    ];
    if reversed {
        units.reverse();
    }
    TacticalBattle::new_at_location(
        FlatBattlefield::new(200_000, 100_000),
        units,
        BattlefieldLocation::ForestClearing,
    )
    .unwrap()
}
#[test]
fn attack_move_acquires_nearest_enemy_with_stable_id_ties() {
    for reversed in [false, true] {
        let mut battle = contact_battle(reversed);
        battle
            .issue_group_movement(group(
                &["a"],
                120_000,
                50_000,
                false,
                MovementMode::AttackMove,
            ))
            .unwrap();
        battle.advance_ticks(1);
        assert_eq!(unit(&battle, "a").engagement_target(), Some("d-a"));
        assert_eq!(
            unit(&battle, "a").destination(),
            Some(BattlePoint::new(120_000, 50_000))
        );
    }
}
#[test]
fn normal_march_does_not_acquire_attack_move_targets() {
    let mut battle = contact_battle(false);
    battle
        .issue_group_movement(group(&["a"], 120_000, 50_000, false, MovementMode::March))
        .unwrap();
    battle.advance_ticks(1);
    assert_eq!(unit(&battle, "a").engagement_target(), None);
}
#[test]
fn attack_move_drops_targets_that_leave_its_acquisition_radius() {
    let mut battle = contact_battle(false);
    battle
        .issue_group_movement(group(
            &["a"],
            120_000,
            50_000,
            false,
            MovementMode::AttackMove,
        ))
        .unwrap();
    battle.advance_ticks(1);
    let mut value = serde_json::to_value(&battle).unwrap();
    for unit in value["units"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .filter(|unit| unit["side"] == "defender")
    {
        unit["position"] = serde_json::json!({"xMm":180_000,"yMm":80_000});
    }
    let mut battle: TacticalBattle = serde_json::from_value(value).unwrap();
    battle.advance_ticks(1);
    assert_eq!(unit(&battle, "a").engagement_target(), None);
    assert_eq!(
        unit(&battle, "a").destination(),
        Some(BattlePoint::new(120_000, 50_000))
    );
}
#[test]
fn coincident_positions_get_distinct_deterministic_slots() {
    let mut value = serde_json::to_value(battle()).unwrap();
    value["units"][1]["position"] = value["units"][0]["position"].clone();
    let mut first: TacticalBattle = serde_json::from_value(value).unwrap();
    let mut second = first.clone();
    first
        .issue_group_movement(group(
            &["a", "b"],
            100_000,
            50_000,
            false,
            MovementMode::March,
        ))
        .unwrap();
    second
        .issue_group_movement(group(
            &["b", "a"],
            100_000,
            50_000,
            false,
            MovementMode::March,
        ))
        .unwrap();
    assert_eq!(first, second);
    assert_ne!(
        unit(&first, "a").destination(),
        unit(&first, "b").destination()
    );
}

#[test]
fn queued_terrain_rejection_preserves_current_and_pending_orders() {
    let mut battle = TacticalBattle::new_at_location(
        FlatBattlefield::new(200_000, 100_000),
        battle().units().to_vec(),
        BattlefieldLocation::RiverFord,
    )
    .unwrap();
    battle
        .issue_group_movement(group(&["a"], 50_000, 20_000, false, MovementMode::March))
        .unwrap();
    let before = battle.clone();
    // Centre is on the west bank; its complete footprint overlaps the river.
    assert!(
        battle
            .issue_group_movement(group(&["a"], 72_000, 20_000, true, MovementMode::March))
            .is_err()
    );
    assert_eq!(battle, before);
}
#[test]
fn widening_rejects_a_now_illegal_queued_footprint() {
    let mut battle = battle();
    battle
        .issue_group_movement(group(&["a"], 50_000, 20_000, false, MovementMode::March))
        .unwrap();
    battle
        .issue_group_movement(group(&["a"], 6_000, 20_000, true, MovementMode::March))
        .unwrap();
    let before = battle.clone();
    assert!(
        battle
            .issue_formation_orders(&[medieval_core::FormationOrder::Frontage {
                unit_id: "a".into(),
                width_mm: 20_000
            }])
            .is_err()
    );
    assert_eq!(battle, before);
}
#[test]
fn loading_an_oversized_waypoint_queue_is_rejected() {
    let mut value = serde_json::to_value(battle()).unwrap();
    value["units"][0]["queuedMovements"] = serde_json::json!(vec![
        serde_json::json!({"destination":{"xMm":50_000,"yMm":20_000},"mode":"march"});
        MAX_QUEUED_WAYPOINTS + 1
    ]);
    assert!(serde_json::from_value::<TacticalBattle>(value).is_err());
}
#[test]
fn attack_move_resumes_after_formed_targets_are_defeated() {
    let mut battle = contact_battle(false);
    battle
        .issue_group_movement(group(
            &["a"],
            120_000,
            50_000,
            false,
            MovementMode::AttackMove,
        ))
        .unwrap();
    battle.advance_ticks(1);
    let before = unit(&battle, "a").position();
    let mut value = serde_json::to_value(&battle).unwrap();
    for unit in value["units"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .filter(|unit| unit["side"] == "defender")
    {
        unit["state"] = serde_json::json!("destroyed");
        unit["soldiers"] = serde_json::json!(0);
    }
    let mut battle: TacticalBattle = serde_json::from_value(value).unwrap();
    battle.advance_ticks(1);
    assert_eq!(unit(&battle, "a").engagement_target(), None);
    assert!(unit(&battle, "a").position().x_mm > before.x_mm);
    assert_eq!(
        unit(&battle, "a").destination(),
        Some(BattlePoint::new(120_000, 50_000))
    );
}

#[test]
fn direct_formation_changes_preserve_queued_placement_constraints() {
    let mut battle = battle();
    battle
        .issue_group_movement(group(&["a"], 50_000, 20_000, false, MovementMode::March))
        .unwrap();
    battle
        .issue_group_movement(group(&["a"], 6_000, 20_000, true, MovementMode::March))
        .unwrap();
    let before = battle.clone();
    assert!(
        battle
            .issue_formation_order("a", Formation::Line { files: 20 })
            .is_err()
    );
    assert_eq!(battle, before);
}
