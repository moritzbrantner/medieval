use medieval_core::{
    Army, BattleSide, BattlefieldLocation, CampaignSave, CampaignState, FlatBattlefield,
    TacticalBattle, TacticalBattleResult, TacticalBattleState, TacticalFinishReason,
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

fn victory(campaign: &CampaignState, winner: BattleSide) -> TacticalBattleResult {
    let mut document = serde_json::to_value(deployed(campaign)).unwrap();
    for unit in document["units"].as_array_mut().unwrap() {
        let side = if unit["side"] == "attacker" {
            BattleSide::Attacker
        } else {
            BattleSide::Defender
        };
        if side != winner {
            unit["state"] = serde_json::json!("routed");
        }
    }
    finish(serde_json::from_value(document).unwrap())
}

#[test]
fn attacker_victory_captures_the_province_and_retreats_each_defending_source() {
    let mut campaign = campaign();
    let result = victory(&campaign, BattleSide::Attacker);
    let report = campaign.apply_tactical_battle_result(&result).unwrap();
    assert_eq!(report.captured_province.as_deref(), Some("paris"));
    assert_eq!(
        campaign
            .provinces
            .iter()
            .find(|province| province.id == "paris")
            .unwrap()
            .owner,
        "england"
    );
    assert_eq!(
        campaign
            .armies
            .iter()
            .find(|army| army.id == "england-main")
            .unwrap()
            .province,
        "paris"
    );
    assert_eq!(report.retreats.len(), 2);
    for retreat in &report.retreats {
        assert_eq!(retreat.from_province, "paris");
        assert_eq!(retreat.to_province, "anjou");
        let army = campaign
            .armies
            .iter()
            .find(|army| army.id == retreat.source_army_id)
            .unwrap();
        assert_eq!(army.province, "anjou");
        assert!(army.moved_this_turn);
    }
    assert!(report.surrenders.is_empty());
    assert!(campaign.pending_battle.is_none());
    assert!(campaign.pending_tactical_result.is_none());
    assert_eq!(campaign.tactical_battle_reports, [report]);
    CampaignSave::from_campaign(campaign, "england").unwrap();
}

#[test]
fn defender_victory_preserves_ownership_and_returns_attacking_survivors_to_their_origin() {
    let mut campaign = campaign();
    let result = victory(&campaign, BattleSide::Defender);
    let report = campaign.apply_tactical_battle_result(&result).unwrap();
    assert!(report.captured_province.is_none());
    assert_eq!(
        campaign
            .provinces
            .iter()
            .find(|province| province.id == "paris")
            .unwrap()
            .owner,
        "france"
    );
    assert_eq!(report.retreats.len(), 1);
    assert_eq!(report.retreats[0].to_province, "normandy");
    assert_eq!(
        campaign
            .armies
            .iter()
            .find(|army| army.id == "england-main")
            .unwrap()
            .province,
        "normandy"
    );
    assert!(
        campaign
            .armies
            .iter()
            .filter(|army| army.owner == "france")
            .all(|army| army.province == "paris")
    );
}

#[test]
fn no_legal_defender_retreat_records_surrender_without_inventing_tactical_casualties() {
    let mut campaign = campaign();
    for province in &mut campaign.provinces {
        if ["anjou", "flanders"].contains(&province.id.as_str()) {
            province.owner = "england".into();
        }
    }
    let result = victory(&campaign, BattleSide::Attacker);
    let report = campaign.apply_tactical_battle_result(&result).unwrap();
    assert_eq!(report.surrenders.len(), 2);
    assert!(report.retreats.is_empty());
    assert!(!campaign.armies.iter().any(|army| army.owner == "france"));
    for surrender in &report.surrenders {
        let source = result
            .armies
            .iter()
            .find(|army| army.source_army_id == surrender.source_army_id)
            .unwrap();
        for unit in &source.units {
            let surrendered = surrender
                .units
                .iter()
                .find(|entry| entry.kind == unit.kind)
                .unwrap()
                .soldiers;
            assert_eq!(unit.initial_soldiers, unit.casualties + surrendered);
            assert_eq!(unit.casualties, 0);
        }
    }
    CampaignSave::from_campaign(campaign, "england").unwrap();
}

#[test]
fn an_attacker_without_a_friendly_neighbor_surrenders_after_defeat() {
    let mut campaign = campaign();
    campaign
        .provinces
        .iter_mut()
        .find(|province| province.id == "normandy")
        .unwrap()
        .owner = "france".into();
    let result = victory(&campaign, BattleSide::Defender);
    let report = campaign.apply_tactical_battle_result(&result).unwrap();
    assert_eq!(report.surrenders.len(), 1);
    assert_eq!(report.surrenders[0].source_army_id, "england-main");
    assert!(!campaign.armies.iter().any(|army| army.id == "england-main"));
    assert!(report.captured_province.is_none());
}

#[test]
fn retreat_is_independent_of_province_army_and_neighbor_storage_order() {
    let mut first = campaign();
    let result = victory(&first, BattleSide::Attacker);
    let mut reordered = first.clone();
    reordered.provinces.reverse();
    reordered.armies.reverse();
    for province in &mut reordered.provinces {
        province.neighbors.reverse();
    }
    assert_eq!(
        first.apply_tactical_battle_result(&result).unwrap(),
        reordered.apply_tactical_battle_result(&result).unwrap()
    );
    first.provinces.sort_by(|a, b| a.id.cmp(&b.id));
    reordered.provinces.sort_by(|a, b| a.id.cmp(&b.id));
    for state in [&mut first, &mut reordered] {
        state.armies.sort_by(|a, b| a.id.cmp(&b.id));
        for province in &mut state.provinces {
            province.neighbors.sort();
        }
    }
    assert_eq!(first, reordered);
}

#[test]
fn staged_reconciliation_and_direct_resolution_produce_identical_campaigns() {
    let mut direct = campaign();
    let result = victory(&direct, BattleSide::Attacker);
    let mut staged = direct.clone();
    staged.reconcile_tactical_casualties(&result).unwrap();
    assert_eq!(
        direct.apply_tactical_battle_result(&result).unwrap(),
        staged.apply_tactical_battle_result(&result).unwrap()
    );
    assert_eq!(direct, staged);
}

#[test]
fn repeated_resolution_after_save_reload_is_an_exact_no_op() {
    let mut campaign = campaign();
    let result = victory(&campaign, BattleSide::Attacker);
    let report = campaign.apply_tactical_battle_result(&result).unwrap();
    let once = campaign.clone();
    assert_eq!(
        campaign.apply_tactical_battle_result(&result).unwrap(),
        report
    );
    assert_eq!(campaign, once);
    let document = CampaignSave::from_campaign(campaign, "england")
        .unwrap()
        .to_json()
        .unwrap();
    let mut restored = CampaignSave::from_json(&document).unwrap().campaign;
    assert_eq!(
        restored.apply_tactical_battle_result(&result).unwrap(),
        report
    );
    assert_eq!(restored, once);
}

#[test]
fn a_mismatched_result_does_not_change_ownership_armies_pending_state_or_reports() {
    let mut campaign = campaign();
    let mut result = victory(&campaign, BattleSide::Attacker);
    result.seed.turn += 1;
    let before = campaign.clone();
    assert!(campaign.apply_tactical_battle_result(&result).is_err());
    assert_eq!(campaign, before);
}

#[test]
fn draws_preserve_the_province_and_return_attackers_without_capturing() {
    let mut campaign = campaign();
    let mut battle = deployed(&campaign);
    battle.withdraw(BattleSide::Attacker).unwrap();
    battle.withdraw(BattleSide::Defender).unwrap();
    let result = finish(battle);
    let report = campaign.apply_tactical_battle_result(&result).unwrap();
    assert_eq!(report.result.winner, None);
    assert!(report.captured_province.is_none());
    assert_eq!(report.retreats.len(), 1);
    assert_eq!(report.retreats[0].source_army_id, "england-main");
    assert_eq!(
        campaign
            .provinces
            .iter()
            .find(|province| province.id == "paris")
            .unwrap()
            .owner,
        "france"
    );
}

#[test]
fn capturing_the_last_enemy_province_checks_campaign_victory_after_surrender() {
    let mut campaign = campaign();
    for province in &mut campaign.provinces {
        if province.id != "paris" {
            province.owner = "england".into();
        }
    }
    let result = victory(&campaign, BattleSide::Attacker);
    let report = campaign.apply_tactical_battle_result(&result).unwrap();
    assert_eq!(report.campaign_winner.as_deref(), Some("england"));
    assert_eq!(campaign.winner().as_deref(), Some("england"));
    assert!(
        campaign
            .log
            .last()
            .unwrap()
            .contains("has won the campaign")
    );
}

#[test]
fn siege_objective_capture_reaches_the_campaign_report_and_province_owner() {
    let mut campaign = campaign();
    campaign
        .provinces
        .iter_mut()
        .find(|province| province.id == "paris")
        .unwrap()
        .battlefield
        .fortified = true;
    let mut document = serde_json::to_value(deployed(&campaign)).unwrap();
    for unit in document["units"].as_array_mut().unwrap() {
        unit["position"] = if unit["side"] == "attacker" {
            serde_json::json!({"xMm":96_000,"yMm":40_000})
        } else {
            serde_json::json!({"xMm":110_000,"yMm":5_000})
        };
    }
    let result = finish(serde_json::from_value(document).unwrap());
    assert_eq!(result.reason, TacticalFinishReason::SiegeCapture);
    let report = campaign.apply_tactical_battle_result(&result).unwrap();
    assert_eq!(report.captured_province.as_deref(), Some("paris"));
    assert_eq!(
        report
            .result
            .settlement_capture
            .as_ref()
            .unwrap()
            .faction_id,
        "england"
    );
    assert_eq!(
        campaign
            .provinces
            .iter()
            .find(|province| province.id == "paris")
            .unwrap()
            .owner,
        "england"
    );
}

#[test]
fn capture_cancels_the_former_owners_queued_recruitment_and_preserves_other_orders() {
    let mut campaign = medieval_core::new_campaign();
    campaign.select_player_faction("france").unwrap();
    campaign
        .queue_recruitment("paris", medieval_core::UnitKind::Levy)
        .unwrap();
    campaign
        .queue_recruitment("anjou", medieval_core::UnitKind::Levy)
        .unwrap();
    campaign.select_player_faction("england").unwrap();
    campaign.move_army("england-main", "paris").unwrap();
    let result = victory(&campaign, BattleSide::Attacker);
    campaign.apply_tactical_battle_result(&result).unwrap();
    assert_eq!(campaign.recruitment_queue.len(), 1);
    assert_eq!(campaign.recruitment_queue[0].province_id, "anjou");
    CampaignSave::from_campaign(campaign, "england").unwrap();
}

#[test]
fn saves_preserve_reports_and_reject_unsupported_legacy_claims_or_corrupt_surrender_counts() {
    let mut campaign = campaign();
    for province in &mut campaign.provinces {
        if ["anjou", "flanders"].contains(&province.id.as_str()) {
            province.owner = "england".into();
        }
    }
    let result = victory(&campaign, BattleSide::Attacker);
    campaign.apply_tactical_battle_result(&result).unwrap();
    let save = CampaignSave::from_campaign(campaign, "england").unwrap();
    let mut document = serde_json::to_value(&save).unwrap();
    document["schemaVersion"] = serde_json::json!(1);
    assert!(CampaignSave::from_json(&serde_json::to_string(&document).unwrap()).is_err());
    let mut corrupted = save.campaign;
    corrupted.tactical_battle_reports[0].surrenders[0].units[0].soldiers += 1;
    let before = corrupted.clone();
    assert!(corrupted.apply_tactical_battle_result(&result).is_err());
    assert_eq!(corrupted, before);
    assert!(CampaignSave::from_campaign(corrupted, "england").is_err());
}

#[test]
fn a_serialized_winner_flip_cannot_capture_the_province_or_move_the_wrong_force() {
    for original_winner in [BattleSide::Attacker, BattleSide::Defender] {
        let mut campaign = campaign();
        let result = victory(&campaign, original_winner);
        let mut document = serde_json::to_value(result).unwrap();
        document["winner"] = serde_json::json!(if original_winner == BattleSide::Attacker {
            "defender"
        } else {
            "attacker"
        });
        let inconsistent: TacticalBattleResult = serde_json::from_value(document).unwrap();
        let before = campaign.clone();
        assert!(inconsistent.validate().is_err());
        assert!(
            campaign
                .apply_tactical_battle_result(&inconsistent)
                .is_err()
        );
        assert_eq!(campaign, before);
    }
}

#[test]
fn withdrawal_and_draw_claims_must_match_escaped_and_routed_survivors() {
    let mut campaign = campaign();
    let mut battle = deployed(&campaign);
    battle.withdraw(BattleSide::Attacker).unwrap();
    let result = finish(battle);
    let before = campaign.clone();
    let mut flipped = result.clone();
    flipped.winner = Some(BattleSide::Attacker);
    let mut false_draw = result;
    false_draw.winner = None;
    false_draw.reason = TacticalFinishReason::MutualDefeat;
    for inconsistent in [flipped, false_draw] {
        assert!(inconsistent.validate().is_err());
        assert!(
            campaign
                .apply_tactical_battle_result(&inconsistent)
                .is_err()
        );
        assert_eq!(campaign, before);
    }
}

#[test]
fn existing_nonadjacent_retreat_destinations_are_rejected_on_load_and_completed_replay() {
    let mut campaign = campaign();
    let result = victory(&campaign, BattleSide::Attacker);
    campaign.apply_tactical_battle_result(&result).unwrap();
    let save = CampaignSave::from_campaign(campaign.clone(), "england").unwrap();
    let mut document = serde_json::to_value(save).unwrap();
    document["campaign"]["tacticalBattleReports"][0]["retreats"][0]["toProvince"] =
        serde_json::json!("wessex");
    assert!(CampaignSave::from_json(&serde_json::to_string(&document).unwrap()).is_err());
    campaign.tactical_battle_reports[0].retreats[0].to_province = "wessex".into();
    let before = campaign.clone();
    assert!(campaign.apply_tactical_battle_result(&result).is_err());
    assert_eq!(campaign, before);
}

#[test]
fn completed_reports_cannot_overlap_the_same_pending_battle_or_be_recorded_twice() {
    let base = campaign();
    let result = victory(&base, BattleSide::Attacker);
    let mut completed = base.clone();
    let report = completed.apply_tactical_battle_result(&result).unwrap();
    for staged in [false, true] {
        let mut conflicting = base.clone();
        if staged {
            conflicting.reconcile_tactical_casualties(&result).unwrap();
        }
        conflicting.tactical_battle_reports.push(report.clone());
        assert!(CampaignSave::from_campaign(conflicting.clone(), "england").is_err());
        let before = conflicting.clone();
        assert!(conflicting.apply_tactical_battle_result(&result).is_err());
        assert_eq!(conflicting, before);
    }
    completed.tactical_battle_reports.push(report);
    assert!(CampaignSave::from_campaign(completed, "england").is_err());
}

#[test]
fn completed_report_attacker_identity_must_match_its_retained_source_roster() {
    let mut campaign = campaign();
    let result = victory(&campaign, BattleSide::Attacker);
    campaign.apply_tactical_battle_result(&result).unwrap();
    campaign.tactical_battle_reports[0]
        .result
        .seed
        .attacker_army_id = "unrelated-army".into();
    assert!(CampaignSave::from_campaign(campaign, "england").is_err());
}
