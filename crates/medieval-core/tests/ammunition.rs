use medieval_core::{
    BattlePoint, BattleSide, FlatBattlefield, Formation, TacticalBattle, TacticalUnit,
    UnitCombatProfile, UnitKind,
};

fn battle() -> TacticalBattle {
    let units = vec![
        TacticalUnit::new(
            "a",
            BattleSide::Attacker,
            80,
            BattlePoint::new(10_000, 20_000),
            Formation::Line { files: 10 },
            100,
        )
        .with_combat_stats(UnitCombatProfile::v1(UnitKind::Archers)),
        TacticalUnit::new(
            "d",
            BattleSide::Defender,
            320,
            BattlePoint::new(25_000, 20_000),
            Formation::Line { files: 10 },
            100,
        )
        .with_combat_stats(UnitCombatProfile::v1(UnitKind::Knights)),
    ];
    let mut battle = TacticalBattle::new(FlatBattlefield::new(100_000, 100_000), units).unwrap();
    battle.issue_engagement_order("a", "d").unwrap();
    battle
}

fn with_ammunition(battle: &TacticalBattle, remaining: u16) -> TacticalBattle {
    let mut value = serde_json::to_value(battle).unwrap();
    value["units"][0]["ammunition"] = serde_json::json!(remaining);
    serde_json::from_value(value).unwrap()
}

#[test]
fn core_missile_profile_owns_the_initial_volley_budget() {
    for kind in [
        UnitKind::Levy,
        UnitKind::Spearmen,
        UnitKind::Archers,
        UnitKind::Knights,
    ] {
        let unit = TacticalUnit::new(
            "unit",
            BattleSide::Attacker,
            80,
            BattlePoint::new(10_000, 20_000),
            Formation::Line { files: 10 },
            100,
        )
        .with_combat_stats(UnitCombatProfile::v1(kind));
        assert_eq!(
            unit.ammunition(),
            unit.stats()
                .unwrap()
                .missile
                .map(|missile| missile.ammunition)
        );
        assert_eq!(unit.ammunition().is_some(), kind == UnitKind::Archers);
    }
}

#[test]
fn firing_consumes_once_per_pulse_even_without_whole_casualties() {
    let mut firing = battle();
    let mut value = serde_json::to_value(&firing).unwrap();
    value["units"][0]["formation"] = serde_json::json!({"line":{"files":1}});
    firing = serde_json::from_value(value).unwrap();
    firing.advance_ticks(19);
    assert_eq!(firing.units()[0].ammunition(), Some(30));
    firing.advance_ticks(1);
    assert_eq!(firing.units()[0].ammunition(), Some(29));
    assert_eq!(firing.units()[1].soldiers(), 320);
    firing.issue_engagement_order("a", "d").unwrap();
    assert_eq!(firing.units()[0].ammunition(), Some(29));
    firing.advance_ticks(19);
    assert_eq!(firing.units()[0].ammunition(), Some(29));
    firing.advance_ticks(1);
    assert_eq!(firing.units()[0].ammunition(), Some(28));
}

#[test]
fn out_of_range_and_routed_units_do_not_spend_ammunition() {
    for routed in [false, true] {
        let mut value = serde_json::to_value(battle()).unwrap();
        if routed {
            value["units"][0]["state"] = serde_json::json!("routed");
        } else {
            value["units"][1]["position"]["xMm"] = serde_json::json!(50_000);
        }
        let mut inactive: TacticalBattle = serde_json::from_value(value).unwrap();
        inactive.advance_ticks(20);
        assert_eq!(inactive.units()[0].ammunition(), Some(30));
        assert_eq!(inactive.units()[1].soldiers(), 320);
    }
}

#[test]
fn last_volley_exhausts_the_budget_without_repeating_fractional_fire() {
    let mut last = with_ammunition(&battle(), 1);
    last.advance_ticks(20);
    assert_eq!(last.units()[0].ammunition(), Some(0));
    assert_eq!(last.units()[0].attack_range_mm(), 1_500);
    let checkpoint = serde_json::to_value(&last).unwrap();
    last.advance_ticks(20);
    assert_eq!(last.units()[0].ammunition(), Some(0));
    let after = serde_json::to_value(&last).unwrap();
    assert_eq!(
        checkpoint["rangedDamageCredit"],
        after["rangedDamageCredit"]
    );
    assert_eq!(
        checkpoint["units"][1]["soldiers"],
        after["units"][1]["soldiers"]
    );
}

#[test]
fn exhausted_archers_close_for_melee_instead_of_firing_from_range() {
    let mut exhausted = with_ammunition(&battle(), 0);
    exhausted.advance_ticks(20);
    assert_eq!(exhausted.units()[1].soldiers(), 320);
    assert!(exhausted.units()[0].position().x_mm > 10_000);
    assert_eq!(exhausted.units()[0].ammunition(), Some(0));
    exhausted.advance_ticks(300);
    assert!(exhausted.units()[1].soldiers() < 320);
    assert_eq!(exhausted.units()[0].ammunition(), Some(0));
}

#[test]
fn ammunition_and_fractional_damage_replay_across_serialization_and_order() {
    let mut direct = battle();
    direct.advance_ticks(20);
    let mut value = serde_json::to_value(&direct).unwrap();
    value["units"].as_array_mut().unwrap().reverse();
    let mut replay: TacticalBattle = serde_json::from_value(value).unwrap();
    direct.advance_ticks(240);
    for _ in 0..12 {
        replay.advance_ticks(20);
    }
    assert_eq!(direct, replay);
    assert_eq!(direct.units()[0].ammunition(), Some(17));
}

#[test]
fn historical_missing_ammunition_keeps_previous_ranged_behavior_and_invalid_budgets_fail() {
    let mut value = serde_json::to_value(battle()).unwrap();
    value["units"][0]
        .as_object_mut()
        .unwrap()
        .remove("ammunition");
    let mut historical: TacticalBattle = serde_json::from_value(value.clone()).unwrap();
    historical.advance_ticks(620);
    assert_eq!(historical.units()[0].ammunition(), None);
    assert_eq!(historical.units()[0].attack_range_mm(), 25_000);
    assert!(historical.units()[1].soldiers() < 320);
    value["units"][0]["ammunition"] = serde_json::json!(31);
    assert!(serde_json::from_value::<TacticalBattle>(value).is_err());
}

fn finished_campaign() -> (
    medieval_core::CampaignState,
    medieval_core::TacticalBattleResult,
) {
    let mut campaign = medieval_core::new_campaign();
    let paris = campaign
        .provinces
        .iter_mut()
        .find(|province| province.id == "paris")
        .unwrap();
    paris.battlefield.fortified = false;
    paris.battlefield.location = medieval_core::BattlefieldLocation::ForestClearing;
    campaign.move_army("england-main", "paris").unwrap();
    let battle = TacticalBattle::from_campaign_seed(
        FlatBattlefield::new(120_000, 80_000),
        campaign.pending_tactical_battle_seed().unwrap(),
    )
    .unwrap();
    let mut value = serde_json::to_value(battle).unwrap();
    for unit in value["units"].as_array_mut().unwrap() {
        if unit["side"] == "attacker" && unit["unitKind"] == "archers" {
            unit["ammunition"] = serde_json::json!(27);
        }
    }
    let mut checkpoint: TacticalBattle = serde_json::from_value(value).unwrap();
    checkpoint.withdraw(BattleSide::Attacker).unwrap();
    checkpoint.advance_ticks(2_000);
    (campaign, checkpoint.campaign_result().unwrap())
}

#[test]
fn result_and_campaign_boundary_save_preserve_source_ammunition() {
    let (mut campaign, result) = finished_campaign();
    let archers = result
        .armies
        .iter()
        .find(|army| army.source_army_id == "england-main")
        .unwrap()
        .units
        .iter()
        .find(|unit| unit.kind == UnitKind::Archers)
        .unwrap();
    let ammunition = archers.ammunition.as_ref().unwrap();
    assert_eq!(
        (ammunition.initial_volleys, ammunition.remaining_volleys),
        (30, 27)
    );
    let decoded =
        medieval_core::TacticalBattleResult::from_json(&result.to_json().unwrap()).unwrap();
    assert_eq!(decoded, result);
    campaign.reconcile_tactical_casualties(&result).unwrap();
    let save = medieval_core::CampaignSave::from_campaign(campaign, "england").unwrap();
    let loaded = medieval_core::CampaignSave::from_json(&save.to_json().unwrap()).unwrap();
    assert_eq!(loaded, save);
    assert_eq!(
        loaded.campaign.pending_tactical_result.as_ref(),
        Some(&result)
    );
}

#[test]
fn result_rejects_invalid_volley_conservation_and_accepts_historical_missing_resources() {
    let (_, result) = finished_campaign();
    for (initial, remaining) in [(30, 31), (29, 27)] {
        let mut corrupt = result.clone();
        let archers = corrupt.armies[0]
            .units
            .iter_mut()
            .find(|unit| unit.kind == UnitKind::Archers)
            .unwrap();
        archers.ammunition = Some(medieval_core::TacticalAmmunitionResult {
            initial_volleys: initial,
            remaining_volleys: remaining,
        });
        assert!(corrupt.validate().is_err());
    }
    let mut value = serde_json::to_value(result).unwrap();
    for army in value["armies"].as_array_mut().unwrap() {
        for unit in army["units"].as_array_mut().unwrap() {
            unit.as_object_mut().unwrap().remove("ammunition");
        }
    }
    let historical: medieval_core::TacticalBattleResult = serde_json::from_value(value).unwrap();
    historical.validate().unwrap();
}

#[test]
fn thirty_authoritative_volleys_exhaust_without_a_thirty_first_shot() {
    let mut finite = battle();
    finite.advance_ticks(600);
    assert_eq!(finite.units()[0].ammunition(), Some(0));
    assert!(finite.units()[1].soldiers() > 0);
    let before = serde_json::to_value(&finite).unwrap();
    finite.advance_ticks(20);
    let after = serde_json::to_value(&finite).unwrap();
    assert_eq!(before["rangedDamageCredit"], after["rangedDamageCredit"]);
    assert_eq!(
        before["units"][1]["soldiers"],
        after["units"][1]["soldiers"]
    );
    assert_eq!(finite.units()[0].ammunition(), Some(0));
}
