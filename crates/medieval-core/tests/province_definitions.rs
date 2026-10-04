use medieval_core::{
    CampaignSave, ProvinceDefinitionDocument, ProvinceDefinitionError, ProvinceDefinitions,
    default_province_definitions, new_campaign, new_campaign_with_province_definitions,
};

fn document() -> ProvinceDefinitionDocument {
    default_province_definitions().document().clone()
}
#[test]
fn packaged_data_preserves_the_complete_six_province_bootstrap() {
    let legacy =
        CampaignSave::from_json(include_str!("fixtures/six-provinces-save-v2.json")).unwrap();
    assert_eq!(new_campaign(), legacy.campaign);
    assert_eq!(
        CampaignSave::from_campaign(new_campaign(), "england").unwrap(),
        legacy
    );
}
#[test]
fn save_schema_one_migration_keeps_the_same_campaign_semantics() {
    let mut value: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/six-provinces-save-v2.json")).unwrap();
    value["schemaVersion"] = serde_json::json!(1);
    let migrated = CampaignSave::from_json(&serde_json::to_string(&value).unwrap()).unwrap();
    assert_eq!(migrated.schema_version, 2);
    assert_eq!(migrated.campaign, new_campaign());
}
#[test]
fn loading_historical_metadata_does_not_replace_it_with_new_definitions() {
    let mut value: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/six-provinces-save-v2.json")).unwrap();
    value["schemaVersion"] = serde_json::json!(1);
    for province in value["campaign"]["provinces"].as_array_mut().unwrap() {
        province.as_object_mut().unwrap().remove("battlefield");
    }
    let migrated = CampaignSave::from_json(&serde_json::to_string(&value).unwrap()).unwrap();
    assert!(migrated.campaign.provinces.iter().all(
        |province| province.battlefield == medieval_core::ProvinceBattlefieldContext::default()
    ));
    assert_eq!(
        CampaignSave::from_json(&migrated.to_json().unwrap()).unwrap(),
        migrated
    );
}
#[test]
fn stable_ids_survive_localized_display_names() {
    let mut data = document();
    data.provinces[1].name = "Normandie".into();
    let definitions = ProvinceDefinitions::validate(data).unwrap();
    let mut campaign = new_campaign_with_province_definitions(&definitions).unwrap();
    assert!(
        campaign
            .legal_destinations("england-main")
            .unwrap()
            .contains(&"wessex".to_owned())
    );
    campaign.move_army("england-main", "wessex").unwrap();
    assert_eq!(campaign.armies[0].province, "wessex");
    assert!(campaign.log.last().unwrap().contains("Normandie"));
}
#[test]
fn definitions_reject_duplicate_ids_and_empty_names() {
    let mut data = document();
    data.provinces[1].id = data.provinces[0].id.clone();
    assert!(ProvinceDefinitions::validate(data).is_err());
    let mut data = document();
    data.battlefield_profiles
        .push(data.battlefield_profiles[0].clone());
    assert!(ProvinceDefinitions::validate(data).is_err());
    let mut data = document();
    data.provinces[0].name = "  ".into();
    assert!(ProvinceDefinitions::validate(data).is_err());
    let mut data = document();
    data.provinces[0].id = "Wessex province".into();
    assert!(ProvinceDefinitions::validate(data).is_err());
}
#[test]
fn definitions_reject_dangling_repeated_self_and_one_way_adjacency() {
    let mut data = document();
    data.provinces[0].neighbors.push("missing".into());
    assert!(ProvinceDefinitions::validate(data).is_err());
    let mut data = document();
    data.provinces[0].neighbors.push("normandy".into());
    assert!(ProvinceDefinitions::validate(data).is_err());
    let mut data = document();
    data.provinces[0].neighbors.push("wessex".into());
    assert!(ProvinceDefinitions::validate(data).is_err());
    let mut data = document();
    data.provinces[1].neighbors.retain(|id| id != "wessex");
    assert!(ProvinceDefinitions::validate(data).is_err());
}

#[test]
fn definitions_reject_disconnected_components_even_with_reciprocal_borders() {
    let mut data = document();
    data.provinces[0].neighbors.clear();
    data.provinces[1].neighbors.retain(|id| id != "wessex");
    assert!(ProvinceDefinitions::validate(data).is_err());
}
#[test]
fn definitions_reject_invalid_battlefield_references_and_context() {
    let mut data = document();
    data.provinces[0].battlefield_profile = "unknown".into();
    assert!(ProvinceDefinitions::validate(data).is_err());
    let mut value = serde_json::to_value(document()).unwrap();
    value["battlefieldProfiles"][0]["context"]["location"] = serde_json::json!("unknown");
    assert!(ProvinceDefinitions::from_json(&value.to_string()).is_err());
    let mut value = serde_json::to_value(document()).unwrap();
    value["battlefieldProfiles"][0]["context"]["fortifyd"] = serde_json::json!(true);
    assert!(ProvinceDefinitions::from_json(&value.to_string()).is_err());
}
#[test]
fn settlement_fortification_must_agree_with_its_battlefield_profile() {
    let mut data = document();
    data.provinces
        .iter_mut()
        .find(|province| province.id == "paris")
        .unwrap()
        .settlement
        .fortified = false;
    assert!(ProvinceDefinitions::validate(data).is_err());
}
#[test]
fn starting_owners_and_army_positions_are_validated() {
    let mut data = document();
    data.provinces[1].starting_owner = "unknown-faction".into();
    let definitions = ProvinceDefinitions::validate(data).unwrap();
    assert!(new_campaign_with_province_definitions(&definitions).is_err());
    let mut data = document();
    data.provinces[1].starting_owner = "france".into();
    let definitions = ProvinceDefinitions::validate(data).unwrap();
    assert!(new_campaign_with_province_definitions(&definitions).is_err());
    let campaign = new_campaign();
    let mut armies = campaign.armies.clone();
    armies[0].province = "missing".into();
    assert!(
        default_province_definitions()
            .build_provinces(&campaign.factions, &armies)
            .is_err()
    );
}
#[test]
fn expanded_data_adds_a_province_without_hard_coded_movement_changes() {
    let mut data = document();
    let mut maine = data.provinces[2].clone();
    maine.id = "maine".into();
    maine.name = "Maine".into();
    maine.neighbors = vec!["anjou".into()];
    maine.base_economy.wealth = 2;
    data.provinces[3].neighbors.push("maine".into());
    data.provinces.push(maine);
    let definitions = ProvinceDefinitions::validate(data).unwrap();
    let mut campaign = new_campaign_with_province_definitions(&definitions).unwrap();
    assert_eq!(campaign.provinces.len(), 7);
    campaign.active_faction = "france".into();
    campaign.move_army("france-main", "anjou").unwrap();
    campaign.end_turn().unwrap();
    campaign.end_turn().unwrap();
    assert!(
        campaign
            .legal_destinations("france-main")
            .unwrap()
            .contains(&"maine".to_owned())
    );
    let save = CampaignSave::from_campaign(campaign, "france").unwrap();
    assert_eq!(
        CampaignSave::from_json(&save.to_json().unwrap()).unwrap(),
        save
    );
}
#[test]
fn extension_metadata_round_trips_and_income_is_bounded() {
    let mut data = document();
    data.provinces[0]
        .extensions
        .insert("agriculture".into(), serde_json::json!({"level":2}));
    let definitions =
        ProvinceDefinitions::from_json(&serde_json::to_string(&data).unwrap()).unwrap();
    assert_eq!(definitions.document(), &data);
    data.provinces[0].base_economy.wealth = u32::MAX;
    assert!(ProvinceDefinitions::validate(data).is_err());
}
#[test]
fn definition_versions_and_unknown_fields_are_explicitly_rejected() {
    assert_eq!(
        ProvinceDefinitions::from_json(r#"{"schemaVersion":99,"futureShape":true}"#).unwrap_err(),
        ProvinceDefinitionError::UnsupportedVersion(99)
    );
    let mut value = serde_json::to_value(document()).unwrap();
    value["typo"] = serde_json::json!(true);
    assert!(ProvinceDefinitions::from_json(&value.to_string()).is_err());
}
