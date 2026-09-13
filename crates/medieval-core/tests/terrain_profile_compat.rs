use medieval_core::{
    BattlePoint, BattleSide, FlatBattlefield, Formation, TacticalBattle, TacticalTerrainProfile,
    TacticalUnit,
};

#[test]
fn legacy_battle_documents_keep_height_only_terrain_semantics() {
    let battle = TacticalBattle::new(
        FlatBattlefield::new(80_000, 80_000),
        vec![TacticalUnit::new(
            "attacker",
            BattleSide::Attacker,
            40,
            BattlePoint::new(10_000, 10_000),
            Formation::Line { files: 10 },
            1_200,
        )],
    )
    .unwrap();
    assert_eq!(
        battle.terrain().profile(),
        TacticalTerrainProfile::RiverCrossingsV3
    );
    assert!(!battle.terrain().river_cells().is_empty());

    let mut legacy_document = serde_json::to_value(&battle).unwrap();
    legacy_document
        .as_object_mut()
        .unwrap()
        .remove("terrain")
        .unwrap();

    let restored: TacticalBattle = serde_json::from_value(legacy_document).unwrap();
    assert_eq!(
        restored.terrain().profile(),
        TacticalTerrainProfile::HeightFoundationV1
    );
    assert!(restored.terrain().forest_cells().is_empty());
    assert!(restored.terrain().river_cells().is_empty());
}
