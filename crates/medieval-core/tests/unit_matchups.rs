use medieval_core::{
    BattlePoint, BattleSide, Facing, FlatBattlefield, Formation, TacticalBattle, TacticalUnit,
    UnitCombatProfile, UnitKind,
};

fn contact(
    left_kind: UnitKind,
    right_kind: UnitKind,
    left_facing: Facing,
    right_facing: Facing,
    swapped: bool,
    reversed: bool,
) -> TacticalBattle {
    let mut units = vec![];
    for (id, kind, x, facing, side) in [
        (
            "left",
            left_kind,
            20_000,
            left_facing,
            if swapped {
                BattleSide::Defender
            } else {
                BattleSide::Attacker
            },
        ),
        (
            "right",
            right_kind,
            21_000,
            right_facing,
            if swapped {
                BattleSide::Attacker
            } else {
                BattleSide::Defender
            },
        ),
    ] {
        units.push(
            TacticalUnit::new(
                id,
                side,
                320,
                BattlePoint::new(x, 20_000),
                Formation::Line { files: 80 },
                100,
            )
            .with_combat_stats(UnitCombatProfile::v1(kind))
            .with_facing(facing),
        );
    }
    if reversed {
        units.reverse();
    }
    let mut battle = TacticalBattle::new(FlatBattlefield::new(100_000, 100_000), units).unwrap();
    battle.issue_engagement_order("left", "right").unwrap();
    battle
}

fn soldiers(battle: &TacticalBattle, id: &str) -> u16 {
    battle
        .units()
        .iter()
        .find(|unit| unit.id() == id)
        .unwrap()
        .soldiers()
}

#[test]
fn frontal_spears_damage_cavalry_and_reduce_its_incoming_damage() {
    let mut front = contact(
        UnitKind::Spearmen,
        UnitKind::Knights,
        Facing::east(),
        Facing::west(),
        false,
        false,
    );
    let mut exposed = contact(
        UnitKind::Spearmen,
        UnitKind::Knights,
        Facing::new(0, 1).unwrap(),
        Facing::west(),
        false,
        false,
    );
    front.advance_ticks(20);
    exposed.advance_ticks(20);
    assert!(soldiers(&front, "right") < soldiers(&exposed, "right"));
    assert!(soldiers(&front, "left") > soldiers(&exposed, "left"));
}

#[test]
fn cavalry_exploits_flanked_levy_and_archers() {
    for kind in [UnitKind::Levy, UnitKind::Archers] {
        let mut front = contact(
            UnitKind::Knights,
            kind,
            Facing::east(),
            Facing::west(),
            false,
            false,
        );
        let mut flank = contact(
            UnitKind::Knights,
            kind,
            Facing::east(),
            Facing::new(0, 1).unwrap(),
            false,
            false,
        );
        let mut rear = contact(
            UnitKind::Knights,
            kind,
            Facing::east(),
            Facing::east(),
            false,
            false,
        );
        for battle in [&mut front, &mut flank, &mut rear] {
            battle.advance_ticks(20);
        }
        assert!(soldiers(&flank, "right") < soldiers(&front, "right"));
        assert!(soldiers(&rear, "right") <= soldiers(&flank, "right"));
        assert!(320 - soldiers(&rear, "right") <= 3 * (320 - soldiers(&front, "right")));
    }
}

#[test]
fn supported_matchups_are_side_symmetric_and_independent_of_storage_order() {
    for (left, right, facing) in [
        (UnitKind::Spearmen, UnitKind::Knights, Facing::west()),
        (
            UnitKind::Knights,
            UnitKind::Levy,
            Facing::new(0, 1).unwrap(),
        ),
        (UnitKind::Knights, UnitKind::Archers, Facing::east()),
    ] {
        let mut direct = contact(left, right, Facing::east(), facing, false, false);
        let mut reversed = contact(left, right, Facing::east(), facing, false, true);
        let mut swapped = contact(left, right, Facing::east(), facing, true, true);
        direct.advance_ticks(60);
        for _ in 0..6 {
            reversed.advance_ticks(10);
            swapped.advance_ticks(10);
        }
        assert_eq!(direct, reversed);
        for id in ["left", "right"] {
            let a = direct.units().iter().find(|unit| unit.id() == id).unwrap();
            let b = swapped.units().iter().find(|unit| unit.id() == id).unwrap();
            assert_eq!(
                (a.soldiers(), a.morale(), a.fatigue(), a.is_routed()),
                (b.soldiers(), b.morale(), b.fatigue(), b.is_routed())
            );
        }
    }
}

#[test]
fn historical_metadata_does_not_activate_matchups() {
    let battle = contact(
        UnitKind::Spearmen,
        UnitKind::Knights,
        Facing::east(),
        Facing::west(),
        false,
        false,
    );
    let mut document = serde_json::to_value(&battle).unwrap();
    for unit in document["units"].as_array_mut().unwrap() {
        unit.as_object_mut().unwrap().remove("combatProfile");
    }
    let mut metadata: TacticalBattle = serde_json::from_value(document.clone()).unwrap();
    for unit in document["units"].as_array_mut().unwrap() {
        unit.as_object_mut().unwrap().remove("unitKind");
    }
    let mut untyped: TacticalBattle = serde_json::from_value(document).unwrap();
    metadata.advance_ticks(60);
    untyped.advance_ticks(60);
    for id in ["left", "right"] {
        assert_eq!(soldiers(&metadata, id), soldiers(&untyped, id));
    }
}

#[test]
fn frontal_matchup_is_additional_to_neutral_front_contact() {
    let mut front = contact(
        UnitKind::Spearmen,
        UnitKind::Knights,
        Facing::east(),
        Facing::west(),
        false,
        false,
    );
    let mut document = serde_json::to_value(&front).unwrap();
    for unit in document["units"].as_array_mut().unwrap() {
        unit.as_object_mut().unwrap().remove("facing");
    }
    let mut neutral: TacticalBattle = serde_json::from_value(document).unwrap();
    front.advance_ticks(20);
    neutral.advance_ticks(20);
    assert!(soldiers(&front, "right") < soldiers(&neutral, "right"));
    assert!(soldiers(&front, "left") > soldiers(&neutral, "left"));
}

#[test]
fn missile_armor_and_forest_cover_compose_at_equal_elevation() {
    let make = |kind, x| {
        let attacker = TacticalUnit::new(
            "left",
            BattleSide::Attacker,
            320,
            BattlePoint::new(15_000, 5_000),
            Formation::Line { files: 80 },
            100,
        )
        .with_combat_stats(UnitCombatProfile::v1(UnitKind::Archers));
        let defender = TacticalUnit::new(
            "right",
            BattleSide::Defender,
            320,
            BattlePoint::new(x, 15_000),
            Formation::Line { files: 80 },
            100,
        )
        .with_combat_stats(UnitCombatProfile::v1(kind));
        let mut battle = TacticalBattle::new(
            FlatBattlefield::new(80_000, 80_000),
            vec![attacker, defender],
        )
        .unwrap();
        battle.issue_engagement_order("left", "right").unwrap();
        battle.advance_ticks(120);
        battle
    };
    let terrain = medieval_core::TacticalTerrain::battlefield_foundation();
    let field = FlatBattlefield::new(80_000, 80_000);
    assert_eq!(
        terrain.height_mm(field, BattlePoint::new(15_000, 15_000)),
        terrain.height_mm(field, BattlePoint::new(25_000, 15_000))
    );
    let levy_open = make(UnitKind::Levy, 15_000);
    let levy_forest = make(UnitKind::Levy, 25_000);
    let knight_open = make(UnitKind::Knights, 15_000);
    let knight_forest = make(UnitKind::Knights, 25_000);
    assert!(soldiers(&levy_forest, "right") > soldiers(&levy_open, "right"));
    assert!(soldiers(&knight_forest, "right") > soldiers(&knight_open, "right"));
    assert!(soldiers(&knight_open, "right") > soldiers(&levy_open, "right"));
    assert!(soldiers(&knight_forest, "right") > soldiers(&levy_forest, "right"));
    assert!(soldiers(&knight_forest, "right") < 320);
}

#[test]
fn ten_file_formations_preserve_matchup_effects_and_fractional_replay() {
    let front = contact(
        UnitKind::Spearmen,
        UnitKind::Knights,
        Facing::east(),
        Facing::west(),
        false,
        false,
    );
    let mut document = serde_json::to_value(&front).unwrap();
    for unit in document["units"].as_array_mut().unwrap() {
        unit["soldiers"] = serde_json::json!(80);
        unit["formation"] = serde_json::json!({"line":{"files":10}});
    }
    let mut small: TacticalBattle = serde_json::from_value(document.clone()).unwrap();
    for unit in document["units"].as_array_mut().unwrap() {
        unit.as_object_mut().unwrap().remove("facing");
    }
    let mut neutral: TacticalBattle = serde_json::from_value(document).unwrap();
    small.advance_ticks(20);
    let checkpoint = serde_json::to_value(&small).unwrap();
    assert!(
        checkpoint["meleeDamageCredit"]["left"]["right"]
            .as_u64()
            .unwrap()
            > 0
    );
    let mut replay: TacticalBattle = serde_json::from_value(checkpoint.clone()).unwrap();
    small.advance_ticks(100);
    neutral.advance_ticks(120);
    for _ in 0..10 {
        replay.advance_ticks(10);
    }
    assert_eq!(small, replay);
    assert!(soldiers(&small, "right") < soldiers(&neutral, "right"));
    assert!(soldiers(&small, "left") > soldiers(&neutral, "left"));
    let mut corrupt = checkpoint;
    corrupt["meleeDamageCredit"]["left"]["right"] = serde_json::json!(1_000);
    assert!(serde_json::from_value::<TacticalBattle>(corrupt).is_err());
}
