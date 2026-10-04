use medieval_core::{
    BattlePoint, BattleSide, Facing, FlatBattlefield, Formation, FormationOrder, MovementOrder,
    TacticalBattle, TacticalUnit,
};

fn battle() -> TacticalBattle {
    TacticalBattle::new(
        FlatBattlefield::new(100_000, 100_000),
        vec![
            TacticalUnit::new(
                "a",
                BattleSide::Attacker,
                80,
                BattlePoint::new(20_000, 20_000),
                Formation::Line { files: 10 },
                1_000,
            )
            .with_facing(Facing::east()),
            TacticalUnit::new(
                "b",
                BattleSide::Attacker,
                80,
                BattlePoint::new(80_000, 20_000),
                Formation::Line { files: 10 },
                1_000,
            )
            .with_facing(Facing::east()),
            TacticalUnit::new(
                "d",
                BattleSide::Defender,
                80,
                BattlePoint::new(80_000, 80_000),
                Formation::Line { files: 10 },
                1_000,
            )
            .with_facing(Facing::west()),
        ],
    )
    .unwrap()
}
fn unit<'a>(battle: &'a TacticalBattle, id: &str) -> &'a TacticalUnit {
    battle.units().iter().find(|unit| unit.id() == id).unwrap()
}
fn movement(facing: Option<Facing>) -> FormationOrder {
    FormationOrder::MoveAndFace {
        unit_id: "a".into(),
        destination: BattlePoint::new(20_000, 30_000),
        facing,
    }
}
#[test]
fn rotate_halts_travel_and_engagement() {
    let mut battle = battle();
    battle.issue_engagement_order("a", "d").unwrap();
    let facing = Facing::new(0, -1).unwrap();
    battle
        .issue_formation_orders(&[FormationOrder::Rotate {
            unit_id: "a".into(),
            facing,
        }])
        .unwrap();
    assert_eq!(unit(&battle, "a").facing(), Some(facing));
    assert_eq!(unit(&battle, "a").destination(), None);
    assert_eq!(unit(&battle, "a").engagement_target(), None);
    battle.advance_ticks(2);
    assert_eq!(
        unit(&battle, "a").position(),
        BattlePoint::new(20_000, 20_000)
    );
}
#[test]
fn quarter_turns_have_exact_core_owned_directions() {
    let mut battle = battle();
    for (turns, expected) in [(-1, [0, -1]), (2, [0, 1]), (1, [-1, 0]), (4, [-1, 0])] {
        battle
            .issue_formation_orders(&[FormationOrder::Turn {
                unit_id: "a".into(),
                quarter_turns: turns,
            }])
            .unwrap();
        assert_eq!(unit(&battle, "a").facing().unwrap().direction(), expected);
    }
}
#[test]
fn move_and_face_uses_travel_direction_then_requested_arrival_direction() {
    let mut battle = battle();
    let north = Facing::new(0, -1).unwrap();
    battle
        .issue_formation_orders(&[movement(Some(north))])
        .unwrap();
    battle.advance_ticks(1);
    assert_eq!(unit(&battle, "a").facing().unwrap().direction(), [0, 1]);
    battle.advance_ticks(9);
    assert_eq!(unit(&battle, "a").destination(), None);
    assert_eq!(unit(&battle, "a").facing(), Some(north));
}
#[test]
fn omitted_arrival_direction_holds_current_facing() {
    let mut battle = battle();
    battle.issue_formation_orders(&[movement(None)]).unwrap();
    battle.advance_ticks(10);
    assert_eq!(unit(&battle, "a").facing(), Some(Facing::east()));
}
#[test]
fn stationary_move_and_face_applies_immediately() {
    let mut battle = battle();
    battle
        .issue_formation_orders(&[FormationOrder::MoveAndFace {
            unit_id: "a".into(),
            destination: BattlePoint::new(20_000, 20_000),
            facing: Some(Facing::west()),
        }])
        .unwrap();
    assert_eq!(unit(&battle, "a").facing(), Some(Facing::west()));
}
#[test]
fn arrival_direction_survives_save_and_deterministic_replay() {
    let mut first = battle();
    first
        .issue_formation_orders(&[movement(Some(Facing::west()))])
        .unwrap();
    first.advance_ticks(3);
    let mut restored: TacticalBattle =
        serde_json::from_str(&serde_json::to_string(&first).unwrap()).unwrap();
    first.advance_ticks(7);
    restored.advance_ticks(7);
    assert_eq!(first, restored);
    assert_eq!(unit(&restored, "a").facing(), Some(Facing::west()));
}
#[test]
fn replacement_move_cancels_queued_arrival_direction() {
    let mut battle = battle();
    battle
        .issue_formation_orders(&[movement(Some(Facing::west()))])
        .unwrap();
    battle
        .issue_move_order(MovementOrder {
            unit_id: "a".into(),
            destination: BattlePoint::new(30_000, 20_000),
        })
        .unwrap();
    battle.advance_ticks(10);
    assert_eq!(unit(&battle, "a").facing(), Some(Facing::east()));
}
#[test]
fn requested_width_uses_whole_metre_files_and_caps_at_soldiers() {
    let units = battle().units().to_vec();
    let mut battle = TacticalBattle::new_at_location(
        FlatBattlefield::new(100_000, 100_000),
        units,
        medieval_core::BattlefieldLocation::ForestClearing,
    )
    .unwrap();
    battle
        .issue_formation_orders(&[FormationOrder::Frontage {
            unit_id: "a".into(),
            width_mm: 9_999,
        }])
        .unwrap();
    assert_eq!(unit(&battle, "a").formation(), Formation::Line { files: 9 });
    // Centre this unit so the maximum legal width fits.
    battle
        .issue_move_order(MovementOrder {
            unit_id: "a".into(),
            destination: BattlePoint::new(50_000, 20_000),
        })
        .unwrap();
    for _ in 0..500 {
        if unit(&battle, "a").destination().is_none() {
            break;
        }
        battle.advance_ticks(1);
    }
    assert_eq!(
        unit(&battle, "a").position(),
        BattlePoint::new(50_000, 20_000)
    );
    battle
        .issue_formation_orders(&[FormationOrder::Frontage {
            unit_id: "a".into(),
            width_mm: 500_000,
        }])
        .unwrap();
    assert_eq!(
        unit(&battle, "a").formation(),
        Formation::Line { files: 80 }
    );
}
#[test]
fn batch_rejection_rolls_back_earlier_valid_commands() {
    let mut battle = battle();
    let before = battle.clone();
    assert!(
        battle
            .issue_formation_orders(&[
                FormationOrder::Rotate {
                    unit_id: "a".into(),
                    facing: Facing::west()
                },
                FormationOrder::Frontage {
                    unit_id: "b".into(),
                    width_mm: 80_000
                },
            ])
            .is_err()
    );
    assert_eq!(battle, before);
}
#[test]
fn complete_footprint_must_fit_even_when_destination_centre_fits() {
    let mut battle = battle();
    let before = battle.clone();
    assert!(
        battle
            .issue_formation_orders(&[FormationOrder::MoveAndFace {
                unit_id: "a".into(),
                destination: BattlePoint::new(1_000, 20_000),
                facing: None
            }])
            .is_err()
    );
    assert_eq!(battle, before);
    assert!(
        battle
            .issue_formation_orders(&[FormationOrder::Frontage {
                unit_id: "a".into(),
                width_mm: 999
            }])
            .is_err()
    );
    assert_eq!(battle, before);
}
#[test]
fn frontage_validates_existing_destination_before_committing() {
    let mut battle = battle();
    battle
        .issue_move_order(MovementOrder {
            unit_id: "a".into(),
            destination: BattlePoint::new(6_000, 20_000),
        })
        .unwrap();
    let before = battle.clone();
    assert!(
        battle
            .issue_formation_orders(&[FormationOrder::Frontage {
                unit_id: "a".into(),
                width_mm: 20_000
            }])
            .is_err()
    );
    assert_eq!(battle, before);
}
#[test]
fn withdrawing_units_reject_formation_commands() {
    let mut battle = battle();
    battle = battle.start();
    battle.withdraw(BattleSide::Attacker).unwrap();
    let before = battle.clone();
    assert!(
        battle
            .issue_formation_orders(&[FormationOrder::Turn {
                unit_id: "a".into(),
                quarter_turns: 1
            }])
            .is_err()
    );
    assert_eq!(battle, before);
}

#[test]
fn terrain_rejection_checks_footprint_and_allows_the_ford() {
    let mut battle = TacticalBattle::new_at_location(
        FlatBattlefield::new(100_000, 100_000),
        battle().units().to_vec(),
        medieval_core::BattlefieldLocation::RiverFord,
    )
    .unwrap();
    let before = battle.clone();
    assert!(
        battle
            .issue_formation_orders(&[FormationOrder::MoveAndFace {
                unit_id: "a".into(),
                destination: BattlePoint::new(35_000, 20_000),
                facing: None
            }])
            .is_err()
    );
    assert_eq!(battle, before);
    battle
        .issue_formation_orders(&[FormationOrder::MoveAndFace {
            unit_id: "a".into(),
            destination: BattlePoint::new(45_000, 50_000),
            facing: None,
        }])
        .unwrap();
}
#[test]
fn siege_wall_and_closed_gate_reject_while_open_gate_allows() {
    let units = vec![
        TacticalUnit::new(
            "a",
            BattleSide::Attacker,
            8,
            BattlePoint::new(20_000, 20_000),
            Formation::Line { files: 4 },
            1000,
        ),
        TacticalUnit::new(
            "d",
            BattleSide::Defender,
            8,
            BattlePoint::new(80_000, 80_000),
            Formation::Line { files: 4 },
            1000,
        ),
    ];
    let mut battle =
        TacticalBattle::deploy_siege(FlatBattlefield::new(100_000, 100_000), units).unwrap();
    for y in [20_000, 50_000] {
        let before = battle.clone();
        assert!(
            battle
                .issue_formation_orders(&[FormationOrder::MoveAndFace {
                    unit_id: "a".into(),
                    destination: BattlePoint::new(50_000, y),
                    facing: None
                }])
                .is_err()
        );
        assert_eq!(battle, before);
    }
    battle.open_siege_gate().unwrap();
    battle
        .issue_formation_orders(&[FormationOrder::MoveAndFace {
            unit_id: "a".into(),
            destination: BattlePoint::new(50_000, 50_000),
            facing: None,
        }])
        .unwrap();
}
