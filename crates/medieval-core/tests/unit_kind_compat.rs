use medieval_core::{
    BattlePoint, BattleSide, FlatBattlefield, Formation, TacticalBattle, TacticalUnit,
};

#[test]
fn legacy_missing_kinds_do_not_reinterpret_ranged_or_melee_simulation() {
    let mut legacy = TacticalBattle::new(
        FlatBattlefield::new(100_000, 100_000),
        vec![
            TacticalUnit::new(
                "looks-like-knights",
                BattleSide::Attacker,
                40,
                BattlePoint::new(10_000, 10_000),
                Formation::Line { files: 10 },
                100,
            )
            .with_attack_range_mm(25_000),
            TacticalUnit::new(
                "looks-like-archers",
                BattleSide::Defender,
                80,
                BattlePoint::new(20_000, 10_000),
                Formation::Column { files: 5 },
                200,
            ),
        ],
    )
    .unwrap();
    let encoded = serde_json::to_string(&legacy).unwrap();
    assert!(!encoded.contains("unitKind"));
    legacy = serde_json::from_str(&encoded).unwrap();
    assert!(legacy.units().iter().all(|unit| unit.unit_kind().is_none()));
    let mut document = serde_json::to_value(&legacy).unwrap();
    document["units"][0]["unitKind"] = serde_json::json!("knights");
    document["units"][1]["unitKind"] = serde_json::json!("archers");
    let mut explicit: TacticalBattle = serde_json::from_value(document).unwrap();
    legacy
        .issue_engagement_order("looks-like-knights", "looks-like-archers")
        .unwrap();
    explicit
        .issue_engagement_order("looks-like-knights", "looks-like-archers")
        .unwrap();
    legacy.advance_ticks(120);
    explicit.advance_ticks(120);
    for (old, new) in legacy.units().iter().zip(explicit.units()) {
        assert_eq!(
            (
                old.soldiers(),
                old.morale(),
                old.fatigue(),
                old.position(),
                old.attack_range_mm(),
                old.is_routed(),
                old.is_destroyed()
            ),
            (
                new.soldiers(),
                new.morale(),
                new.fatigue(),
                new.position(),
                new.attack_range_mm(),
                new.is_routed(),
                new.is_destroyed()
            )
        );
    }
    assert_eq!(
        serde_json::from_str::<TacticalBattle>(&serde_json::to_string(&explicit).unwrap()).unwrap(),
        explicit
    );
}
