use medieval_core::{
    BattlePoint, BattleSide, COMBAT_CONTACT_DISTANCE_MM, FlatBattlefield, Formation,
    TACTICAL_TICKS_PER_SECOND, TacticalBattle, TacticalUnit,
};

fn unit(id: &str, side: BattleSide, position: BattlePoint, formation: Formation) -> TacticalUnit {
    TacticalUnit::new(id, side, 80, position, formation, 1_200)
}

fn soldiers(battle: &TacticalBattle, unit_id: &str) -> u16 {
    battle
        .units()
        .iter()
        .find(|unit| unit.id() == unit_id)
        .expect("test unit must exist")
        .soldiers()
}

#[test]
fn deserialization_rejects_attack_ranges_outside_core_bounds() {
    let battle = TacticalBattle::new(
        FlatBattlefield::new(50_000, 50_000),
        vec![
            unit(
                "archers",
                BattleSide::Attacker,
                BattlePoint::new(10_000, 25_000),
                Formation::Line { files: 24 },
            )
            .with_attack_range_mm(20_000),
        ],
    )
    .unwrap();
    let encoded = serde_json::to_value(&battle).unwrap();
    let above_physics_limit = u32::try_from(i32::MAX).unwrap() + 1;

    for invalid_range in [COMBAT_CONTACT_DISTANCE_MM - 1, above_physics_limit] {
        let mut invalid = encoded.clone();
        invalid["units"][0]["attackRangeMm"] = serde_json::json!(invalid_range);
        assert!(serde_json::from_value::<TacticalBattle>(invalid).is_err());
    }
}

#[test]
fn ranged_unit_does_not_fire_while_another_enemy_is_in_melee_contact() {
    let mut battle = TacticalBattle::new(
        FlatBattlefield::new(50_000, 50_000),
        vec![
            unit(
                "archers",
                BattleSide::Attacker,
                BattlePoint::new(10_000, 25_000),
                Formation::Line { files: 24 },
            )
            .with_attack_range_mm(20_000),
            unit(
                "ranged-target",
                BattleSide::Defender,
                BattlePoint::new(25_000, 25_000),
                Formation::Line { files: 20 },
            ),
            unit(
                "melee-attacker",
                BattleSide::Defender,
                BattlePoint::new(11_000, 25_000),
                Formation::Line { files: 20 },
            ),
        ],
    )
    .unwrap();

    battle
        .issue_engagement_order("archers", "ranged-target")
        .unwrap();
    battle
        .issue_engagement_order("melee-attacker", "archers")
        .unwrap();
    battle.advance_ticks(TACTICAL_TICKS_PER_SECOND);

    assert_eq!(soldiers(&battle, "ranged-target"), 80);
    assert!(soldiers(&battle, "archers") < 80 || soldiers(&battle, "melee-attacker") < 80);
}
