use medieval_core::{
    BattlePoint, BattleSide, CombatArc, Facing, FlatBattlefield, Formation, MovementOrder,
    TacticalBattle, TacticalUnit, UnitCombatProfile, UnitKind,
};

#[test]
fn front_flank_rear_and_boundary_angles_use_exact_integer_geometry() {
    let origin = BattlePoint::new(10_000, 10_000);
    for (x, y, arc) in [
        (11_000, 10_000, CombatArc::Front),
        (10_000, 11_000, CombatArc::Flank),
        (9_000, 10_000, CombatArc::Rear),
        (11_000, 11_000, CombatArc::Front),
        (10_999, 11_000, CombatArc::Flank),
        (9_000, 11_000, CombatArc::Rear),
        (9_001, 11_000, CombatArc::Flank),
        (10_000, 10_000, CombatArc::Front),
    ] {
        assert_eq!(Facing::east().classify(origin, BattlePoint::new(x, y)), arc);
    }
    let diagonal = Facing::new(1, 1).unwrap();
    assert_eq!(
        diagonal.classify(origin, BattlePoint::new(11_000, 10_000)),
        CombatArc::Front
    );
    assert_eq!(
        diagonal.classify(origin, BattlePoint::new(9_000, 10_000)),
        CombatArc::Rear
    );
    assert_eq!(
        diagonal.classify(origin, BattlePoint::new(9_000, 11_000)),
        CombatArc::Flank
    );
}

#[test]
fn directions_are_canonical_bounded_and_serialized_without_float_angles() {
    assert_eq!(Facing::new(8, 4), Facing::new(2, 1));
    assert!(Facing::new(0, 0).is_none());
    assert!(Facing::new(i64::MIN, 1).is_none());
    assert!(Facing::new(i64::from(u32::MAX) + 1, 0).is_none());
    assert!(serde_json::from_str::<Facing>(r#"{"x":0,"y":0}"#).is_err());
    assert_eq!(
        serde_json::from_str::<Facing>(r#"{"x":8,"y":4}"#).unwrap(),
        Facing::new(2, 1).unwrap()
    );
    let wide = Facing::new(i64::from(u32::MAX), 2).unwrap();
    assert_eq!(
        wide.classify(BattlePoint::new(u32::MAX, u32::MAX), BattlePoint::new(0, 0)),
        CombatArc::Rear
    );
}

fn contact(facing: Facing, reverse: bool) -> TacticalBattle {
    let mut units = vec![
        TacticalUnit::new(
            "a",
            BattleSide::Attacker,
            320,
            BattlePoint::new(20_000, 20_000),
            Formation::Line { files: 80 },
            100,
        )
        .with_combat_stats(UnitCombatProfile::v1(UnitKind::Levy)),
        TacticalUnit::new(
            "d",
            BattleSide::Defender,
            320,
            BattlePoint::new(21_000, 20_000),
            Formation::Line { files: 80 },
            100,
        )
        .with_combat_stats(UnitCombatProfile::v1(UnitKind::Spearmen))
        .with_facing(facing),
    ];
    if reverse {
        units.reverse();
    }
    let mut battle = TacticalBattle::new(FlatBattlefield::new(100_000, 100_000), units).unwrap();
    battle.issue_engagement_order("a", "d").unwrap();
    battle
}

fn defender_survivors(battle: &TacticalBattle) -> u16 {
    battle
        .units()
        .iter()
        .find(|unit| unit.id() == "d")
        .unwrap()
        .soldiers()
}

#[test]
fn flank_and_rear_contact_apply_bounded_combat_advantages() {
    let mut front = contact(Facing::west(), false);
    let mut flank = contact(Facing::new(0, 1).unwrap(), false);
    let mut rear = contact(Facing::east(), false);
    for battle in [&mut front, &mut flank, &mut rear] {
        battle.advance_ticks(20);
    }
    assert!(defender_survivors(&front) > defender_survivors(&flank));
    assert!(defender_survivors(&flank) > defender_survivors(&rear));
    let front_loss = 320 - defender_survivors(&front);
    let rear_loss = 320 - defender_survivors(&rear);
    assert!(rear_loss <= front_loss * 2);
}

#[test]
fn rotating_contact_changes_its_core_arc_and_result() {
    let mut unchanged = contact(Facing::west(), false);
    let mut rotated = unchanged.clone();
    rotated.issue_facing_order("d", Facing::east()).unwrap();
    let defender = rotated
        .units()
        .iter()
        .find(|unit| unit.id() == "d")
        .unwrap();
    assert_eq!(
        defender.incoming_arc(BattlePoint::new(20_000, 20_000)),
        Some(CombatArc::Rear)
    );
    unchanged.advance_ticks(20);
    rotated.advance_ticks(20);
    assert!(defender_survivors(&rotated) < defender_survivors(&unchanged));
    let before = rotated.clone();
    assert!(
        rotated
            .issue_facing_order("missing", Facing::west())
            .is_err()
    );
    assert_eq!(rotated, before);
}

#[test]
fn movement_updates_facing_from_the_actual_core_displacement() {
    let unit = TacticalUnit::new(
        "a",
        BattleSide::Attacker,
        80,
        BattlePoint::new(20_000, 20_000),
        Formation::Line { files: 10 },
        100,
    )
    .with_combat_stats(UnitCombatProfile::v1(UnitKind::Levy));
    let mut battle =
        TacticalBattle::new(FlatBattlefield::new(100_000, 100_000), vec![unit]).unwrap();
    battle
        .issue_move_order(MovementOrder {
            unit_id: "a".into(),
            destination: BattlePoint::new(20_000, 30_000),
        })
        .unwrap();
    battle.advance_ticks(1);
    assert_eq!(battle.units()[0].facing(), Facing::new(0, 1));
}

#[test]
fn directional_contact_replays_independently_of_storage_order_and_tick_partition() {
    let mut direct = contact(Facing::east(), false);
    let document = serde_json::to_string(&contact(Facing::east(), true)).unwrap();
    let mut replay: TacticalBattle = serde_json::from_str(&document).unwrap();
    direct.advance_ticks(60);
    for _ in 0..6 {
        replay.advance_ticks(10);
    }
    assert_eq!(direct, replay);
}

#[test]
fn missing_historical_facing_keeps_previous_contact_rules() {
    let front = contact(Facing::west(), false);
    let mut document = serde_json::to_value(&front).unwrap();
    for unit in document["units"].as_array_mut().unwrap() {
        unit.as_object_mut().unwrap().remove("facing");
    }
    let mut historical: TacticalBattle = serde_json::from_value(document).unwrap();
    assert!(
        historical
            .units()
            .iter()
            .all(|unit| unit.facing().is_none())
    );
    let mut front = front;
    front.advance_ticks(60);
    historical.advance_ticks(60);
    assert_eq!(defender_survivors(&historical), defender_survivors(&front));
}

#[test]
fn routed_units_cannot_receive_rotation_orders() {
    let mut document = serde_json::to_value(contact(Facing::east(), false)).unwrap();
    document["units"][1]["state"] = serde_json::json!("routed");
    let mut battle: TacticalBattle = serde_json::from_value(document).unwrap();
    let before = battle.clone();
    assert!(battle.issue_facing_order("d", Facing::west()).is_err());
    assert_eq!(battle, before);
}
