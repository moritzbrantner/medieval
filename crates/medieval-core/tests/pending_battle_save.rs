use medieval_core::{
    BattleSide, CampaignSave, CampaignState, FlatBattlefield, TacticalBattle, TacticalBattleResult,
    TacticalBattleState,
};

fn pending() -> CampaignState {
    let mut campaign = medieval_core::new_campaign();
    campaign.move_army("england-main", "paris").unwrap();
    campaign
}

fn reload(campaign: &CampaignState) -> CampaignState {
    let document = CampaignSave::from_campaign(campaign.clone(), "england")
        .unwrap()
        .to_json()
        .unwrap();
    CampaignSave::from_json(&document).unwrap().campaign
}

fn withdrawal(campaign: &CampaignState) -> TacticalBattleResult {
    let mut battle = TacticalBattle::from_campaign_seed(
        FlatBattlefield::new(120_000, 80_000),
        campaign.pending_tactical_battle_seed().unwrap(),
    )
    .unwrap();
    battle.withdraw(BattleSide::Attacker).unwrap();
    battle.advance_ticks(5_000);
    assert!(matches!(
        battle.state(),
        TacticalBattleState::Finished { .. }
    ));
    battle.campaign_result().unwrap()
}

#[test]
fn pre_battle_round_trip_retains_identical_rosters_profile_and_identity() {
    let before = pending();
    let after = reload(&before);
    assert_eq!(before, after);
    assert_eq!(
        before.pending_tactical_battle_seed().unwrap(),
        after.pending_tactical_battle_seed().unwrap()
    );
}

#[test]
fn reloaded_pending_battles_resume_both_resolution_choices_identically() {
    let original = pending();
    let mut direct_auto = original.clone();
    let mut loaded_auto = reload(&original);
    assert_eq!(
        direct_auto.resolve_pending_battle(42).unwrap(),
        loaded_auto.resolve_pending_battle(42).unwrap()
    );
    assert_eq!(direct_auto, loaded_auto);
    let result = withdrawal(&original);
    let mut direct_fight = original.clone();
    let mut loaded_fight = reload(&original);
    assert_eq!(
        direct_fight.apply_tactical_battle_result(&result).unwrap(),
        loaded_fight.apply_tactical_battle_result(&result).unwrap()
    );
    assert_eq!(direct_fight, loaded_fight);
}

#[test]
fn version_one_pending_battles_migrate_to_the_current_handoff_schema() {
    let before = pending();
    let mut document =
        serde_json::to_value(CampaignSave::from_campaign(before.clone(), "england").unwrap())
            .unwrap();
    document["schemaVersion"] = serde_json::json!(1);
    let migrated = CampaignSave::from_json(&serde_json::to_string(&document).unwrap()).unwrap();
    assert_eq!(
        migrated.schema_version,
        medieval_core::CAMPAIGN_SAVE_SCHEMA_VERSION
    );
    assert_eq!(migrated.campaign, before);
    assert_eq!(
        migrated.campaign.pending_tactical_battle_seed().unwrap(),
        before.pending_tactical_battle_seed().unwrap()
    );
}

#[test]
fn illegal_pending_movement_provenance_is_rejected_clearly() {
    let original =
        serde_json::to_value(CampaignSave::from_campaign(pending(), "england").unwrap()).unwrap();
    let variants = [
        ("fromProvince", "wessex"),
        ("targetProvince", "wessex"),
        ("attackerFaction", "france"),
        ("defenderFaction", "england"),
        ("attackerArmyId", "missing-army"),
    ];
    for (field, value) in variants {
        let mut document = original.clone();
        document["campaign"]["pendingBattle"][field] = serde_json::json!(value);
        let error =
            CampaignSave::from_json(&serde_json::to_string(&document).unwrap()).unwrap_err();
        assert!(
            error.to_string().contains("pending battle"),
            "{field}: {error}"
        );
    }
    for field in ["activeFaction", "movement"] {
        let mut document = original.clone();
        if field == "activeFaction" {
            document["campaign"][field] = serde_json::json!("france");
        } else {
            document["campaign"]["armies"][0]["movedThisTurn"] = serde_json::json!(false);
        }
        assert!(CampaignSave::from_json(&serde_json::to_string(&document).unwrap()).is_err());
    }
}

#[test]
fn a_saved_reconciled_result_finishes_once_without_repeating_casualties() {
    let mut campaign = pending();
    let result = withdrawal(&campaign);
    campaign.reconcile_tactical_casualties(&result).unwrap();
    let mut loaded = reload(&campaign);
    let report = loaded.finish_reconciled_tactical_battle().unwrap();
    assert_eq!(report.result, result);
    assert!(loaded.pending_battle.is_none());
    assert!(loaded.pending_tactical_result.is_none());
    let after = loaded.clone();
    assert_eq!(
        loaded.apply_tactical_battle_result(&result).unwrap(),
        report
    );
    assert_eq!(loaded, after);
}

#[test]
fn corrupt_staged_handoff_turn_and_profile_are_rejected() {
    let mut campaign = pending();
    campaign
        .reconcile_tactical_casualties(&withdrawal(&campaign))
        .unwrap();
    let original =
        serde_json::to_value(CampaignSave::from_campaign(campaign, "england").unwrap()).unwrap();
    for field in ["turn", "battlefieldProfile"] {
        let mut document = original.clone();
        document["campaign"]["pendingTacticalResult"]["seed"][field] = if field == "turn" {
            serde_json::json!(99)
        } else {
            serde_json::json!({"kind": "field", "location": "riverFord"})
        };
        assert!(CampaignSave::from_json(&serde_json::to_string(&document).unwrap()).is_err());
    }
}
