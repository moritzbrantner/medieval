use medieval_core::{
    Army, BattleSide, BattlefieldLocation, CampaignError, CampaignSave, CampaignState,
    FlatBattlefield, TacticalBattle, TacticalBattleResult, TacticalBattleState, UnitKind,
};

fn campaign() -> CampaignState {
    let mut campaign = medieval_core::new_campaign();
    let paris = campaign
        .provinces
        .iter_mut()
        .find(|province| province.id == "paris")
        .unwrap();
    paris.battlefield.location = BattlefieldLocation::ForestClearing;
    paris.battlefield.fortified = false;
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
    campaign.armies.push(Army {
        id: "england-home".into(),
        owner: "england".into(),
        province: "wessex".into(),
        levy: 3,
        spearmen: 4,
        archers: 5,
        knights: 6,
        moved_this_turn: false,
    });
    campaign.move_army("england-main", "paris").unwrap();
    campaign
}

fn deployed(campaign: &CampaignState) -> TacticalBattle {
    TacticalBattle::from_campaign_seed(
        FlatBattlefield::new(120_000, 80_000),
        campaign.pending_tactical_battle_seed().unwrap(),
    )
    .unwrap()
}

fn finish(mut battle: TacticalBattle) -> TacticalBattleResult {
    battle.advance_ticks(5_000);
    assert!(matches!(
        battle.state(),
        TacticalBattleState::Finished { .. }
    ));
    battle.campaign_result().unwrap()
}

fn combat_result(campaign: &CampaignState) -> TacticalBattleResult {
    let mut document = serde_json::to_value(deployed(campaign)).unwrap();
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
    finish(battle)
}

fn destroyed_result(campaign: &CampaignState, army_id: &str) -> TacticalBattleResult {
    let mut document = serde_json::to_value(deployed(campaign)).unwrap();
    for unit in document["units"].as_array_mut().unwrap() {
        if unit["campaignProvenance"]["sourceArmyId"] == army_id {
            unit["soldiers"] = serde_json::json!(0);
            unit["state"] = serde_json::json!("destroyed");
        }
    }
    let mut battle: TacticalBattle = serde_json::from_value(document).unwrap();
    if army_id != "england-main" {
        battle.withdraw(BattleSide::Attacker).unwrap();
    }
    finish(battle)
}

fn count(army: &Army, kind: UnitKind) -> u64 {
    u64::from(match kind {
        UnitKind::Levy => army.levy,
        UnitKind::Spearmen => army.spearmen,
        UnitKind::Archers => army.archers,
        UnitKind::Knights => army.knights,
    })
}

#[test]
fn reconciliation_preserves_each_source_armys_survivors_and_conserves_every_loss() {
    let mut campaign = campaign();
    let before = campaign.clone();
    let result = combat_result(&campaign);
    campaign.reconcile_tactical_casualties(&result).unwrap();
    for source in &result.armies {
        for unit in &source.units {
            let original = before
                .armies
                .iter()
                .find(|army| army.id == source.source_army_id)
                .unwrap();
            let survivors = campaign
                .armies
                .iter()
                .find(|army| army.id == source.source_army_id)
                .map_or(0, |army| count(army, unit.kind));
            assert_eq!(count(original, unit.kind), survivors + unit.casualties);
            assert_eq!(survivors, unit.surviving_soldiers);
        }
    }
    assert_eq!(
        campaign
            .armies
            .iter()
            .find(|army| army.id == "england-home"),
        before.armies.iter().find(|army| army.id == "england-home")
    );
    assert_eq!(campaign.pending_battle, before.pending_battle);
    assert_eq!(campaign.provinces, before.provinces);
    assert_eq!(campaign.pending_tactical_result, Some(result));
}

#[test]
fn identical_retries_and_save_reload_cannot_apply_casualties_twice() {
    let mut campaign = campaign();
    let result = combat_result(&campaign);
    campaign.reconcile_tactical_casualties(&result).unwrap();
    let once = campaign.clone();
    campaign.reconcile_tactical_casualties(&result).unwrap();
    assert_eq!(campaign, once);
    let saved = CampaignSave::from_campaign(campaign, "england")
        .unwrap()
        .to_json()
        .unwrap();
    let mut restored = CampaignSave::from_json(&saved).unwrap().campaign;
    restored.reconcile_tactical_casualties(&result).unwrap();
    assert_eq!(restored, once);
}

#[test]
fn destroyed_attackers_are_removed_and_the_reconciled_pending_phase_is_saveable() {
    let mut campaign = campaign();
    let result = destroyed_result(&campaign, "england-main");
    campaign.reconcile_tactical_casualties(&result).unwrap();
    assert!(!campaign.armies.iter().any(|army| army.id == "england-main"));
    assert!(campaign.pending_battle.is_some());
    let saved = CampaignSave::from_campaign(campaign.clone(), "england")
        .unwrap()
        .to_json()
        .unwrap();
    assert_eq!(CampaignSave::from_json(&saved).unwrap().campaign, campaign);
    campaign.reconcile_tactical_casualties(&result).unwrap();
}

#[test]
fn removing_one_destroyed_defender_preserves_other_sources_and_unrelated_armies() {
    let mut campaign = campaign();
    let result = destroyed_result(&campaign, "france-main");
    let reserve = campaign
        .armies
        .iter()
        .find(|army| army.id == "france-reserve")
        .unwrap()
        .clone();
    campaign.reconcile_tactical_casualties(&result).unwrap();
    assert!(!campaign.armies.iter().any(|army| army.id == "france-main"));
    assert_eq!(
        campaign
            .armies
            .iter()
            .find(|army| army.id == "france-reserve"),
        Some(&reserve)
    );
    assert_eq!(campaign.armies.len(), 3);
}

#[test]
fn stale_or_mismatched_results_never_partially_mutate_the_campaign() {
    let campaign = campaign();
    let result = combat_result(&campaign);
    let mut candidates = Vec::new();
    let mut stale = result.clone();
    stale.seed.turn += 1;
    candidates.push(stale);
    let mut wrong_target = result.clone();
    wrong_target.seed.target_province = "flanders".into();
    candidates.push(wrong_target);
    let mut wrong_attacker = result.clone();
    wrong_attacker.seed.attacker_army_id = "england-home".into();
    candidates.push(wrong_attacker);
    let mut nonconserving = result;
    nonconserving.armies[1].units[0].surviving_soldiers += 1;
    candidates.push(nonconserving);
    for result in candidates {
        let mut state = campaign.clone();
        assert!(state.reconcile_tactical_casualties(&result).is_err());
        assert_eq!(state, campaign);
    }
}

#[test]
fn changed_source_rosters_ownership_or_location_reject_the_old_result_atomically() {
    let base = campaign();
    let result = combat_result(&base);
    for mutation in 0..3 {
        let mut state = base.clone();
        let source = state
            .armies
            .iter_mut()
            .find(|army| army.id == "france-reserve")
            .unwrap();
        match mutation {
            0 => source.levy += 1,
            1 => source.owner = "england".into(),
            _ => source.province = "flanders".into(),
        }
        let before = state.clone();
        assert!(state.reconcile_tactical_casualties(&result).is_err());
        assert_eq!(state, before);
    }
}

#[test]
fn a_different_completed_result_and_autoresolve_cannot_replace_applied_casualties() {
    let mut campaign = campaign();
    let result = combat_result(&campaign);
    campaign.reconcile_tactical_casualties(&result).unwrap();
    let before = campaign.clone();
    let mut different = result;
    different.finishing_tick += 1;
    assert_eq!(
        campaign.reconcile_tactical_casualties(&different),
        Err(CampaignError::TacticalCasualtiesAlreadyReconciled)
    );
    assert_eq!(
        campaign.resolve_pending_battle(42),
        Err(CampaignError::TacticalCasualtiesAlreadyReconciled)
    );
    assert_eq!(
        campaign.pending_tactical_battle_seed(),
        Err(CampaignError::TacticalCasualtiesAlreadyReconciled)
    );
    assert_eq!(
        campaign.select_player_faction("france"),
        Err(CampaignError::TacticalCasualtiesAlreadyReconciled)
    );
    assert_eq!(campaign, before);
}

#[test]
fn source_storage_order_does_not_change_reconciled_counts() {
    let first = campaign();
    let result = combat_result(&first);
    let mut forward = first.clone();
    let mut reverse = first;
    reverse.armies.reverse();
    forward.reconcile_tactical_casualties(&result).unwrap();
    reverse.reconcile_tactical_casualties(&result).unwrap();
    forward.armies.sort_by(|left, right| left.id.cmp(&right.id));
    reverse.armies.sort_by(|left, right| left.id.cmp(&right.id));
    assert_eq!(forward, reverse);
}

#[test]
fn historical_campaign_saves_default_to_the_unreconciled_phase() {
    let mut base = campaign();
    let mut document =
        serde_json::to_value(CampaignSave::from_campaign(base.clone(), "england").unwrap())
            .unwrap();
    document["schemaVersion"] = serde_json::json!(1);
    strip_settlement_levels(&mut document, &mut base);
    document["campaign"]
        .as_object_mut()
        .unwrap()
        .remove("pendingTacticalResult");
    let restored = CampaignSave::from_json(&serde_json::to_string(&document).unwrap())
        .unwrap()
        .campaign;
    assert_eq!(restored, base);
    assert!(restored.pending_tactical_result.is_none());
}

#[test]
fn save_validation_rejects_an_orphaned_or_inconsistent_reconciliation() {
    let mut campaign = campaign();
    let result = destroyed_result(&campaign, "england-main");
    campaign.reconcile_tactical_casualties(&result).unwrap();
    let mut orphaned = campaign.clone();
    orphaned.pending_battle = None;
    assert!(CampaignSave::from_campaign(orphaned, "england").is_err());
    campaign
        .armies
        .iter_mut()
        .find(|army| army.id == "france-main")
        .unwrap()
        .levy += 1;
    assert!(CampaignSave::from_campaign(campaign, "england").is_err());
}

#[test]
fn all_unit_kinds_reconcile_per_army_without_pooling_defender_losses() {
    let mut campaign = campaign();
    campaign.pending_battle = None;
    for army in &mut campaign.armies {
        if army.id == "england-home" {
            continue;
        }
        army.levy = 40;
        army.spearmen = 20;
        army.archers = 10;
        army.knights = 5;
        army.moved_this_turn = false;
    }
    campaign.move_army("england-main", "paris").unwrap();
    let before = campaign.clone();
    let mut document = serde_json::to_value(deployed(&campaign)).unwrap();
    for unit in document["units"].as_array_mut().unwrap() {
        unit["soldiers"] = serde_json::json!(unit["soldiers"].as_u64().unwrap() - 1);
        if unit["side"] == "defender" {
            unit["state"] = serde_json::json!("routed");
        }
    }
    let battle: TacticalBattle = serde_json::from_value(document).unwrap();
    let result = finish(battle);
    campaign.reconcile_tactical_casualties(&result).unwrap();
    for source in &result.armies {
        let original = before
            .armies
            .iter()
            .find(|army| army.id == source.source_army_id)
            .unwrap();
        let after = campaign
            .armies
            .iter()
            .find(|army| army.id == source.source_army_id)
            .unwrap();
        assert_eq!(source.units.len(), 4);
        for unit in &source.units {
            assert_eq!(unit.casualties, 1);
            assert_eq!(count(original, unit.kind), count(after, unit.kind) + 1);
        }
    }
}

#[test]
fn exact_legacy_single_source_results_reconcile_against_current_seed_metadata() {
    let mut campaign = campaign();
    campaign.armies.retain(|army| army.id != "france-reserve");
    let mut seed = campaign.pending_tactical_battle_seed().unwrap();
    seed.attacker.source_armies = None;
    seed.defender.source_armies = None;
    let mut battle =
        TacticalBattle::from_campaign_seed(FlatBattlefield::new(120_000, 80_000), seed).unwrap();
    battle.withdraw(BattleSide::Attacker).unwrap();
    campaign
        .reconcile_tactical_casualties(&finish(battle))
        .unwrap();
    CampaignSave::from_campaign(campaign, "england").unwrap();
}

#[test]
fn identical_retry_rejects_drift_instead_of_reapplying_the_recorded_counts() {
    let mut campaign = campaign();
    let result = combat_result(&campaign);
    campaign.reconcile_tactical_casualties(&result).unwrap();
    campaign
        .armies
        .iter_mut()
        .find(|army| army.id == "england-main")
        .unwrap()
        .levy += 1;
    let before = campaign.clone();
    assert_eq!(
        campaign.reconcile_tactical_casualties(&result),
        Err(CampaignError::TacticalResultMismatch)
    );
    assert_eq!(campaign, before);
}

#[test]
fn a_result_without_its_pending_battle_cannot_mutate_campaign_armies() {
    let mut campaign = campaign();
    let result = combat_result(&campaign);
    campaign.pending_battle = None;
    let before = campaign.clone();
    assert_eq!(
        campaign.reconcile_tactical_casualties(&result),
        Err(CampaignError::NoPendingBattle)
    );
    assert_eq!(campaign, before);
}

#[test]
fn reconciled_saves_use_the_current_version_and_cannot_be_disguised_as_legacy_documents() {
    let mut campaign = campaign();
    let result = combat_result(&campaign);
    campaign.reconcile_tactical_casualties(&result).unwrap();
    let save = CampaignSave::from_campaign(campaign, "england").unwrap();
    assert_eq!(
        save.schema_version,
        medieval_core::CAMPAIGN_SAVE_SCHEMA_VERSION
    );
    let mut document = serde_json::to_value(&save).unwrap();
    assert_eq!(
        document["schemaVersion"],
        medieval_core::CAMPAIGN_SAVE_SCHEMA_VERSION
    );
    document["schemaVersion"] = serde_json::json!(1);
    strip_settlement_levels(&mut document, &mut save.campaign.clone());
    assert!(
        CampaignSave::from_json(&serde_json::to_string(&document).unwrap())
            .unwrap_err()
            .to_string()
            .contains("tactical battle outcomes")
    );
}

#[test]
fn loading_version_one_preserves_state_and_upgrades_the_next_write_to_the_current_version() {
    let mut original = campaign();
    let mut document =
        serde_json::to_value(CampaignSave::from_campaign(original.clone(), "england").unwrap())
            .unwrap();
    document["schemaVersion"] = serde_json::json!(1);
    strip_settlement_levels(&mut document, &mut original);
    let upgraded = CampaignSave::from_json(&serde_json::to_string(&document).unwrap()).unwrap();
    assert_eq!(upgraded.campaign, original);
    assert_eq!(
        upgraded.schema_version,
        medieval_core::CAMPAIGN_SAVE_SCHEMA_VERSION
    );
    let written: serde_json::Value = serde_json::from_str(&upgraded.to_json().unwrap()).unwrap();
    assert_eq!(
        written["schemaVersion"],
        medieval_core::CAMPAIGN_SAVE_SCHEMA_VERSION
    );
}

/// Pre-settlement-level saves carry no levels: strip them and expect villages.
fn strip_settlement_levels(document: &mut serde_json::Value, expected: &mut CampaignState) {
    for province in document["campaign"]["provinces"].as_array_mut().unwrap() {
        let province = province.as_object_mut().unwrap();
        province.remove("settlementLevel");
        province.remove("buildings");
        province.remove("recruitmentPool");
        province.remove("buildings");
        province.remove("recruitmentPool");
    }
    for province in &mut expected.provinces {
        province.settlement_level = medieval_core::SettlementLevel::Village;
        province.recruitment_pool = province.full_recruitment_pool();
    }
}

#[test]
fn pre_profile_saves_keep_a_staged_field_result_in_a_walled_province() {
    let mut campaign = campaign();
    let result = combat_result(&campaign);
    campaign.reconcile_tactical_casualties(&result).unwrap();
    // Before schema 6 a palisade did not make a siege, so this staged result
    // was fought as a field battle.
    let paris = campaign
        .provinces
        .iter_mut()
        .find(|province| province.id == "paris")
        .unwrap();
    paris.buildings.push(medieval_core::ProvinceBuilding {
        building: medieval_core::BuildingId::Walls,
        level: 1,
    });
    let mut document = serde_json::to_value(CampaignSave {
        schema_version: medieval_core::CAMPAIGN_SAVE_SCHEMA_VERSION,
        player_faction: "england".into(),
        campaign: campaign.clone(),
    })
    .unwrap();
    document["schemaVersion"] = serde_json::json!(5);
    let migrated = CampaignSave::from_json(&document.to_string()).unwrap();
    let pending = migrated.campaign.pending_battle.as_ref().unwrap();
    assert_eq!(
        pending.legacy_battlefield_profile,
        Some(result.seed.battlefield_profile)
    );
    let reloaded = CampaignSave::from_json(&migrated.to_json().unwrap()).unwrap();
    assert_eq!(reloaded, migrated);
    let mut finished = migrated.campaign;
    finished.apply_tactical_battle_result(&result).unwrap();
    assert!(finished.pending_battle.is_none());

    // A current save cannot reinterpret the staged result.
    document["schemaVersion"] = serde_json::json!(medieval_core::CAMPAIGN_SAVE_SCHEMA_VERSION);
    assert!(CampaignSave::from_json(&document.to_string()).is_err());
}
