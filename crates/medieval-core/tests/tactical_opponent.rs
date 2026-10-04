use medieval_core::{
    BattlePoint, BattleSide, FlatBattlefield, Formation, TacticalBattle, TacticalUnit,
};

fn battle(reverse: bool) -> TacticalBattle {
    let mut units = vec![
        TacticalUnit::new(
            "attacker-a",
            BattleSide::Attacker,
            100,
            BattlePoint::new(10_000, 20_000),
            Formation::Line { files: 10 },
            1_000,
        ),
        TacticalUnit::new(
            "attacker-z",
            BattleSide::Attacker,
            100,
            BattlePoint::new(30_000, 20_000),
            Formation::Line { files: 10 },
            1_000,
        ),
        TacticalUnit::new(
            "defender",
            BattleSide::Defender,
            100,
            BattlePoint::new(20_000, 20_000),
            Formation::Line { files: 10 },
            1_000,
        )
        .with_attack_range_mm(25_000),
    ];
    if reverse {
        units.reverse();
    }
    TacticalBattle::new(FlatBattlefield::new(100_000, 100_000), units).unwrap()
}

#[test]
fn opponent_orders_initiate_ranged_combat_without_player_input() {
    let mut battle = battle(false);
    battle.plan_opponent_orders(BattleSide::Defender).unwrap();
    assert_eq!(
        battle
            .units()
            .iter()
            .find(|unit| unit.id() == "defender")
            .unwrap()
            .engagement_target(),
        Some("attacker-a")
    );
    battle.advance_ticks(20);
    assert!(
        battle
            .units()
            .iter()
            .find(|unit| unit.id() == "attacker-a")
            .unwrap()
            .soldiers()
            < 100
    );
}

#[test]
fn equal_distance_target_selection_is_stable_across_storage_order() {
    for reverse in [false, true] {
        let mut battle = battle(reverse);
        battle.plan_opponent_orders(BattleSide::Defender).unwrap();
        assert_eq!(
            battle
                .units()
                .iter()
                .find(|unit| unit.id() == "defender")
                .unwrap()
                .engagement_target(),
            Some("attacker-a")
        );
    }
}

#[test]
fn replanning_replaces_an_escaped_target_and_preserves_withdrawal() {
    let mut battle = battle(false);
    battle.plan_opponent_orders(BattleSide::Defender).unwrap();
    let mut document = serde_json::to_value(battle).unwrap();
    document["units"][0]["state"] = serde_json::json!({"escaped": {"routed": true}});
    let mut battle: TacticalBattle = serde_json::from_value(document).unwrap();
    battle.plan_opponent_orders(BattleSide::Defender).unwrap();
    assert_eq!(
        battle
            .units()
            .iter()
            .find(|unit| unit.id() == "defender")
            .unwrap()
            .engagement_target(),
        Some("attacker-z")
    );
    battle = battle.start();
    battle.withdraw(BattleSide::Defender).unwrap();
    let before = battle.clone();
    battle.plan_opponent_orders(BattleSide::Defender).unwrap();
    assert_eq!(battle, before);
}
