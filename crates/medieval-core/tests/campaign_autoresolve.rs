use medieval_core::{Army, BattleSide, CampaignSave, CampaignState, UnitKind};

fn campaign() -> CampaignState {
    let mut campaign = medieval_core::new_campaign();
    campaign.armies.push(Army {
        id: "france-reserve".into(),
        owner: "france".into(),
        province: "paris".into(),
        levy: 21,
        spearmen: 13,
        archers: 7,
        knights: 1,
        moved_this_turn: false,
    });
    campaign.move_army("england-main", "paris").unwrap();
    campaign
}

fn soldiers(army: &Army, kind: UnitKind) -> u64 {
    u64::from(match kind {
        UnitKind::Levy => army.levy,
        UnitKind::Spearmen => army.spearmen,
        UnitKind::Archers => army.archers,
        UnitKind::Knights => army.knights,
    })
}

#[test]
fn autoresolve_records_a_valid_source_aware_result_and_uses_the_shared_campaign_consequences() {
    let mut campaign = campaign();
    let before = campaign.clone();
    let legacy = campaign.resolve_pending_battle(42).unwrap();
    let report = campaign.tactical_battle_reports.last().unwrap();
    assert_eq!(report.result.auto_resolve_seed, Some(42));
    assert_eq!(report.result.finishing_tick, 0);
    report.result.validate().unwrap();
    report.validate().unwrap();
    for source in &report.result.armies {
        let original = before
            .armies
            .iter()
            .find(|army| army.id == source.source_army_id)
            .unwrap();
        for unit in &source.units {
            assert_eq!(soldiers(original, unit.kind), unit.initial_soldiers);
            assert_eq!(
                unit.initial_soldiers,
                unit.surviving_soldiers + unit.casualties
            );
            let survivors = campaign
                .armies
                .iter()
                .find(|army| army.id == source.source_army_id)
                .map_or(0, |army| soldiers(army, unit.kind));
            let surrendered = report
                .surrenders
                .iter()
                .find(|entry| entry.source_army_id == source.source_army_id)
                .and_then(|entry| entry.units.iter().find(|entry| entry.kind == unit.kind))
                .map_or(0, |entry| entry.soldiers);
            assert_eq!(
                unit.initial_soldiers,
                survivors + unit.casualties + surrendered
            );
        }
    }
    assert_eq!(legacy.captured, report.captured_province.is_some());
    let mut replay = before;
    assert_eq!(
        replay.apply_tactical_battle_result(&report.result).unwrap(),
        *report
    );
    assert_eq!(replay.armies, campaign.armies);
    assert_eq!(replay.provinces, campaign.provinces);
    assert_eq!(replay.recruitment_queue, campaign.recruitment_queue);
    CampaignSave::from_campaign(campaign, "england").unwrap();
}

#[test]
fn autoresolve_replays_from_the_same_seed_independently_of_source_storage_order() {
    let mut first = campaign();
    let mut reordered = first.clone();
    reordered.armies.reverse();
    assert_eq!(
        first.resolve_pending_battle(123).unwrap(),
        reordered.resolve_pending_battle(123).unwrap()
    );
    assert_eq!(
        first.tactical_battle_reports,
        reordered.tactical_battle_reports
    );
    first.armies.sort_by(|a, b| a.id.cmp(&b.id));
    reordered.armies.sort_by(|a, b| a.id.cmp(&b.id));
    assert_eq!(first, reordered);
}

#[test]
fn an_autoresolved_result_cannot_double_apply_after_a_save_reload() {
    let mut campaign = campaign();
    campaign.resolve_pending_battle(7).unwrap();
    let report = campaign.tactical_battle_reports.last().unwrap().clone();
    let saved = CampaignSave::from_campaign(campaign.clone(), "england")
        .unwrap()
        .to_json()
        .unwrap();
    let mut restored = CampaignSave::from_json(&saved).unwrap().campaign;
    assert_eq!(
        restored
            .apply_tactical_battle_result(&report.result)
            .unwrap(),
        report
    );
    assert_eq!(restored, campaign);
}

#[test]
fn empty_forces_resolve_as_a_draw_without_inventing_a_winning_formation() {
    let mut campaign = medieval_core::new_campaign();
    for army in &mut campaign.armies {
        army.levy = 0;
        army.spearmen = 0;
        army.archers = 0;
        army.knights = 0;
    }
    campaign.move_army("england-main", "paris").unwrap();
    let report = campaign.resolve_pending_battle(0).unwrap();
    assert_eq!(report.outcome, medieval_core::BattleOutcome::Draw);
    let result = &campaign.tactical_battle_reports[0].result;
    assert_eq!(result.winner, None);
    result.validate().unwrap();
    assert_eq!(
        campaign
            .provinces
            .iter()
            .find(|province| province.id == "paris")
            .unwrap()
            .owner,
        "france"
    );
    assert!(campaign.armies.is_empty());
}

#[test]
fn legacy_played_result_documents_default_to_no_autoresolve_seed() {
    let mut campaign = campaign();
    campaign.resolve_pending_battle(0).unwrap();
    let result = &campaign.tactical_battle_reports[0].result;
    assert_eq!(result.auto_resolve_seed, Some(0));
    let mut document = serde_json::to_value(result).unwrap();
    document.as_object_mut().unwrap().remove("autoResolveSeed");
    let decoded: medieval_core::TacticalBattleResult = serde_json::from_value(document).unwrap();
    assert_eq!(decoded.auto_resolve_seed, None);
    assert_eq!(decoded.winner, Some(BattleSide::Defender));
    decoded.validate().unwrap();
}
