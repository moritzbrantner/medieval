use medieval_core::{
    Army, BattleSide, BattlefieldLocation, FlatBattlefield, TacticalBattle, TacticalBattleResult,
    TacticalBattleSeed, TacticalBattleState, TacticalBattlefieldProfile, TacticalFinishReason,
    TacticalResultError, UnitKind,
};

fn seed() -> TacticalBattleSeed {
    let mut campaign = medieval_core::new_campaign();
    for army in &mut campaign.armies {
        army.levy = if army.owner == "england" { 80 } else { 40 };
        army.spearmen = 0;
        army.archers = 0;
        army.knights = 0;
    }
    campaign.armies.push(Army {
        id: "france-reserve".into(),
        owner: "france".into(),
        province: "paris".into(),
        levy: 20,
        spearmen: 0,
        archers: 0,
        knights: 0,
        moved_this_turn: false,
    });
    campaign.move_army("england-main", "paris").unwrap();
    let mut seed = campaign.pending_tactical_battle_seed().unwrap();
    seed.battlefield_profile = TacticalBattlefieldProfile::Field {
        location: BattlefieldLocation::ForestClearing,
    };
    seed
}

fn battle(seed: TacticalBattleSeed) -> TacticalBattle {
    TacticalBattle::from_campaign_seed(FlatBattlefield::new(120_000, 80_000), seed).unwrap()
}

fn combat_fixture(seed: TacticalBattleSeed) -> TacticalBattle {
    let mut document = serde_json::to_value(battle(seed)).unwrap();
    for unit in document["units"].as_array_mut().unwrap() {
        let x = if unit["side"] == "attacker" {
            10_000
        } else {
            10_500
        };
        unit["position"] = serde_json::json!({"xMm":x, "yMm":10_000});
    }
    let mut battle: TacticalBattle = serde_json::from_value(document).unwrap();
    let attacker = battle
        .units()
        .iter()
        .find(|unit| unit.side() == BattleSide::Attacker)
        .unwrap()
        .id()
        .to_owned();
    let defenders: Vec<_> = battle
        .units()
        .iter()
        .filter(|unit| unit.side() == BattleSide::Defender)
        .map(|unit| unit.id().to_owned())
        .collect();
    for defender in defenders {
        battle.issue_engagement_order(&defender, &attacker).unwrap();
    }
    battle
}

fn finish(mut battle: TacticalBattle) -> TacticalBattle {
    battle.advance_ticks(5_000);
    assert!(matches!(
        battle.state(),
        TacticalBattleState::Finished { .. }
    ));
    battle
}

#[test]
fn results_attribute_each_defending_armys_actual_survivors_and_casualties() {
    let battle = finish(combat_fixture(seed()));
    let result = battle.campaign_result().unwrap();
    assert_eq!(
        result
            .armies
            .iter()
            .map(|army| army.source_army_id.as_str())
            .collect::<Vec<_>>(),
        ["england-main", "france-main", "france-reserve"]
    );
    assert!(
        result
            .armies
            .iter()
            .flat_map(|army| &army.units)
            .any(|unit| unit.casualties > 0)
    );
    for army in &result.armies {
        for unit in &army.units {
            let tactical: Vec<_> = battle
                .units()
                .iter()
                .filter(|deployed| {
                    deployed.campaign_provenance().unwrap().source_army_id == army.source_army_id
                        && deployed.unit_kind() == Some(unit.kind)
                })
                .collect();
            assert_eq!(
                unit.surviving_soldiers,
                tactical
                    .iter()
                    .map(|deployed| u64::from(deployed.soldiers()))
                    .sum::<u64>()
            );
            assert_eq!(
                unit.initial_soldiers,
                unit.surviving_soldiers + unit.casualties
            );
            assert!(unit.routed_soldiers <= unit.surviving_soldiers);
            assert!(unit.escaped_soldiers <= unit.surviving_soldiers);
            assert!(unit.pursuit_casualties <= unit.casualties);
        }
    }
    assert_eq!(result.armies[1].units[0].initial_soldiers, 40);
    assert_eq!(result.armies[2].units[0].initial_soldiers, 20);
    assert!(result.settlement_capture.is_none());
    result.validate().unwrap();
}

#[test]
fn result_replay_is_stable_across_source_order_tick_partition_and_serialization() {
    let first_seed = seed();
    let mut reordered = first_seed.clone();
    reordered.defender.source_army_ids.reverse();
    reordered.defender.source_armies.as_mut().unwrap().reverse();
    for army in reordered.defender.source_armies.as_mut().unwrap() {
        army.units.reverse();
    }
    let first = finish(combat_fixture(first_seed));
    let mut replay = combat_fixture(reordered);
    replay.advance_ticks(3);
    let mut replay: TacticalBattle =
        serde_json::from_str(&serde_json::to_string(&replay).unwrap()).unwrap();
    for _ in 0..250 {
        replay.advance_ticks(20);
    }
    assert_eq!(first, replay);
    assert_eq!(
        first.campaign_result().unwrap(),
        replay.campaign_result().unwrap()
    );
    let result = replay.campaign_result().unwrap();
    assert_eq!(
        TacticalBattleResult::from_json(&result.to_json().unwrap()).unwrap(),
        result
    );
    assert_eq!(
        serde_json::from_value::<TacticalBattleResult>(serde_json::to_value(&result).unwrap())
            .unwrap(),
        result
    );
}

#[test]
fn withdrawal_results_keep_escaped_survivors_by_source_army() {
    let mut battle = battle(seed());
    battle.withdraw(BattleSide::Defender).unwrap();
    let result = finish(battle).campaign_result().unwrap();
    assert_eq!(result.winner, Some(BattleSide::Attacker));
    assert_eq!(result.reason, TacticalFinishReason::Withdrawal);
    for army in result
        .armies
        .iter()
        .filter(|army| army.side == BattleSide::Defender)
    {
        assert_eq!(
            army.units[0].escaped_soldiers,
            army.units[0].initial_soldiers
        );
        assert_eq!(army.units[0].casualties, 0);
    }
    assert!(result.settlement_capture.is_none());
}

#[test]
fn pursuit_losses_remain_part_of_each_source_armys_total_casualties() {
    let mut document = serde_json::to_value(battle(seed())).unwrap();
    for unit in document["units"].as_array_mut().unwrap() {
        let x = if unit["side"] == "attacker" {
            30_000
        } else {
            33_000
        };
        unit["position"] = serde_json::json!({"xMm":x,"yMm":10_000});
        unit["speedMmPerTick"] = serde_json::json!(50);
    }
    let mut battle: TacticalBattle = serde_json::from_value(document).unwrap();
    let attacker = battle
        .units()
        .iter()
        .find(|unit| unit.side() == BattleSide::Attacker)
        .unwrap()
        .id()
        .to_owned();
    let defenders: Vec<_> = battle
        .units()
        .iter()
        .filter(|unit| unit.side() == BattleSide::Defender)
        .map(|unit| unit.id().to_owned())
        .collect();
    for defender in defenders {
        battle.issue_engagement_order(&defender, &attacker).unwrap();
    }
    battle.withdraw(BattleSide::Attacker).unwrap();
    let result = finish(battle).campaign_result().unwrap();
    let attacker = &result.armies[0].units[0];
    assert!(attacker.pursuit_casualties > 0);
    assert!(attacker.pursuit_casualties <= attacker.casualties);
    assert_eq!(
        attacker.initial_soldiers,
        attacker.surviving_soldiers + attacker.casualties
    );
}

#[test]
fn result_generation_requires_a_finished_campaign_battle() {
    assert_eq!(
        battle(seed()).campaign_result(),
        Err(TacticalResultError::BattleNotFinished)
    );
    let empty = TacticalBattle::new(FlatBattlefield::new(1_000, 1_000), Vec::new())
        .unwrap()
        .start();
    assert_eq!(
        empty.campaign_result(),
        Err(TacticalResultError::MissingCampaignSeed)
    );
}

#[test]
fn incomplete_legacy_multi_army_provenance_stays_playable_but_cannot_be_reconciled() {
    let mut seed = seed();
    seed.attacker.source_armies = None;
    seed.defender.source_armies = None;
    let mut battle = battle(seed);
    battle.withdraw(BattleSide::Attacker).unwrap();
    assert_eq!(
        finish(battle).campaign_result(),
        Err(TacticalResultError::MissingArmyProvenance)
    );
}

#[test]
fn legacy_single_army_seed_can_still_produce_an_exact_result_when_newly_deployed() {
    let mut seed = seed();
    seed.defender.source_armies = None;
    seed.defender.source_army_ids = vec!["france-main".into()];
    seed.attacker.source_armies = None;
    let mut battle = battle(seed);
    battle.withdraw(BattleSide::Attacker).unwrap();
    let result = finish(battle).campaign_result().unwrap();
    assert_eq!(result.armies[0].units[0].escaped_soldiers, 80);
    assert_eq!(result.armies[1].units[0].initial_soldiers, 60);
}

#[test]
fn inconsistent_source_rosters_are_rejected_before_tactical_deployment() {
    let mut seed = seed();
    seed.defender.source_armies.as_mut().unwrap()[0].units[0].soldiers += 1;
    assert_eq!(
        TacticalBattle::from_campaign_seed(FlatBattlefield::new(120_000, 80_000), seed),
        Err(medieval_core::TacticalError::InvalidCampaignProvenance(
            BattleSide::Defender
        ))
    );
}

#[test]
fn result_validation_rejects_unsupported_versions_and_nonconserving_documents() {
    let result = finish(combat_fixture(seed())).campaign_result().unwrap();
    let mut version = result.clone();
    version.schema_version += 1;
    assert_eq!(
        version.validate(),
        Err(TacticalResultError::UnsupportedVersion(2))
    );
    assert_eq!(
        TacticalBattleResult::from_json(&serde_json::to_string(&version).unwrap()),
        Err(TacticalResultError::UnsupportedVersion(2))
    );
    let mut invalid = result.clone();
    invalid.armies[0].units[0].surviving_soldiers += 1;
    assert!(matches!(
        TacticalBattleResult::from_json(&serde_json::to_string(&invalid).unwrap()),
        Err(TacticalResultError::ConservationViolation(_))
    ));
    let mut wrong_origin = result;
    wrong_origin.armies[1].source_army_id = "unrelated-army".into();
    assert!(matches!(
        wrong_origin.to_json(),
        Err(TacticalResultError::InvalidProvenance(_))
    ));
}

#[test]
fn missing_or_duplicate_deployed_units_cannot_silently_become_campaign_losses() {
    let battle = finish(combat_fixture(seed()));
    for duplicate in [false, true] {
        let mut document = serde_json::to_value(&battle).unwrap();
        let units = document["units"].as_array_mut().unwrap();
        if duplicate {
            units.push(units[0].clone());
        } else {
            units.remove(0);
        }
        let malformed: TacticalBattle = serde_json::from_value(document).unwrap();
        assert!(matches!(
            malformed.campaign_result(),
            Err(TacticalResultError::ConservationViolation(_))
        ));
    }
}

#[test]
fn source_aware_large_force_chunks_conserve_their_armys_full_strength() {
    let mut seed = seed();
    seed.attacker.units[0].soldiers = u64::from(u16::MAX) + 42;
    seed.attacker.source_armies.as_mut().unwrap()[0].units[0].soldiers = u64::from(u16::MAX) + 42;
    let mut battle =
        TacticalBattle::from_campaign_seed(FlatBattlefield::new(1_000_000, 10_000_000), seed)
            .unwrap();
    battle.withdraw(BattleSide::Attacker).unwrap();
    let result = finish(battle).campaign_result().unwrap();
    let levy = &result.armies[0].units[0];
    assert_eq!(levy.kind, UnitKind::Levy);
    assert_eq!(levy.initial_soldiers, u64::from(u16::MAX) + 42);
    assert_eq!(levy.escaped_soldiers, levy.initial_soldiers);
}

#[test]
fn siege_capture_records_the_target_province_and_winning_faction() {
    let mut seed = seed();
    seed.battlefield_profile = TacticalBattlefieldProfile::Siege {
        location: BattlefieldLocation::ForestClearing,
    };
    let mut document = serde_json::to_value(battle(seed)).unwrap();
    for unit in document["units"].as_array_mut().unwrap() {
        unit["position"] = if unit["side"] == "attacker" {
            serde_json::json!({"xMm":96_000,"yMm":40_000})
        } else {
            serde_json::json!({"xMm":110_000,"yMm":5_000})
        };
    }
    let battle: TacticalBattle = serde_json::from_value(document).unwrap();
    let result = finish(battle).campaign_result().unwrap();
    assert_eq!(result.reason, TacticalFinishReason::SiegeCapture);
    let capture = result.settlement_capture.as_ref().unwrap();
    assert_eq!(capture.province_id, "paris");
    assert_eq!(capture.faction_id, "england");
    assert_eq!(capture.side, BattleSide::Attacker);
    assert!(
        result
            .armies
            .iter()
            .flat_map(|army| &army.units)
            .all(|unit| unit.casualties == 0)
    );
    let mut inconsistent = result;
    inconsistent.settlement_capture.as_mut().unwrap().faction_id = "france".into();
    assert!(matches!(
        inconsistent.validate(),
        Err(TacticalResultError::InvalidProvenance(_))
    ));
}

#[test]
fn mutual_withdrawal_draw_retains_both_sides_escaped_survivors() {
    let mut battle = battle(seed());
    battle.withdraw(BattleSide::Attacker).unwrap();
    battle.withdraw(BattleSide::Defender).unwrap();
    let result = finish(battle).campaign_result().unwrap();
    assert_eq!(result.winner, None);
    assert_eq!(result.reason, TacticalFinishReason::MutualWithdrawal);
    assert!(
        result
            .armies
            .iter()
            .flat_map(|army| &army.units)
            .all(|unit| unit.escaped_soldiers == unit.initial_soldiers && unit.casualties == 0)
    );
}

#[test]
fn old_battle_records_without_unit_provenance_cannot_guess_campaign_losses() {
    let battle = finish(combat_fixture(seed()));
    let mut document = serde_json::to_value(battle).unwrap();
    for unit in document["units"].as_array_mut().unwrap() {
        unit.as_object_mut().unwrap().remove("campaignProvenance");
    }
    let legacy: TacticalBattle = serde_json::from_value(document).unwrap();
    assert_eq!(
        legacy.campaign_result(),
        Err(TacticalResultError::MissingArmyProvenance)
    );
}

#[test]
fn escaped_routed_soldiers_remain_overlapping_survivor_subsets() {
    let mut document = serde_json::to_value(battle(seed())).unwrap();
    let reserve = document["units"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|unit| unit["campaignProvenance"]["sourceArmyId"] == "france-reserve")
        .unwrap();
    reserve["state"] = serde_json::json!("routed");
    let mut battle: TacticalBattle = serde_json::from_value(document).unwrap();
    battle.withdraw(BattleSide::Defender).unwrap();
    let result = finish(battle).campaign_result().unwrap();
    let reserve = &result.armies[2].units[0];
    assert_eq!(reserve.surviving_soldiers, 20);
    assert_eq!(reserve.routed_soldiers, 20);
    assert_eq!(reserve.escaped_soldiers, 20);
    result.validate().unwrap();
}

#[test]
fn result_validation_rejects_winners_that_contradict_the_terminal_reason() {
    let mut result = finish(combat_fixture(seed())).campaign_result().unwrap();
    result.winner = None;
    assert!(matches!(
        result.validate(),
        Err(TacticalResultError::InvalidProvenance(_))
    ));
}
