use medieval_core::{
    BattlefieldLocation, CampaignSave, FlatBattlefield, TacticalBattle, TacticalBattlefieldProfile,
};

fn pending_campaign() -> medieval_core::CampaignState {
    let mut campaign = medieval_core::new_campaign();
    campaign.move_army("england-main", "paris").unwrap();
    campaign
}

#[test]
fn campaign_context_selects_and_serializes_the_profile() {
    for location in BattlefieldLocation::ALL {
        for fortified in [false, true] {
            let mut campaign = pending_campaign();
            let province = campaign
                .provinces
                .iter_mut()
                .find(|province| province.id == "paris")
                .unwrap();
            province.battlefield.location = location;
            province.battlefield.fortified = fortified;
            let seed = campaign.pending_tactical_battle_seed().unwrap();
            let expected = if fortified {
                TacticalBattlefieldProfile::Siege { location }
            } else {
                TacticalBattlefieldProfile::Field { location }
            };
            assert_eq!(seed.battlefield_profile, expected);
            let restored: medieval_core::TacticalBattleSeed =
                serde_json::from_str(&serde_json::to_string(&seed).unwrap()).unwrap();
            let battle =
                TacticalBattle::from_campaign_seed(FlatBattlefield::new(120_000, 80_000), restored)
                    .unwrap();
            assert_eq!(battle.terrain().location(), location);
            assert_eq!(battle.siege_snapshot().is_some(), fortified);
            assert_eq!(
                battle.campaign_seed().unwrap().battlefield_profile,
                expected
            );
        }
    }
}

#[test]
fn profile_uses_stable_province_identity_and_survives_storage_reordering() {
    let mut campaign = pending_campaign();
    let first = campaign.pending_tactical_battle_seed().unwrap();
    campaign.provinces.reverse();
    for province in &mut campaign.provinces {
        province.name = "renamed presentation".into();
    }
    assert_eq!(campaign.pending_tactical_battle_seed().unwrap(), first);
}

#[test]
fn legacy_campaign_saves_and_seeds_keep_the_original_field_profile() {
    let save = CampaignSave::from_campaign(pending_campaign(), "england").unwrap();
    let mut document = serde_json::to_value(save).unwrap();
    document["schemaVersion"] = serde_json::json!(1);
    for province in document["campaign"]["provinces"].as_array_mut().unwrap() {
        let province = province.as_object_mut().unwrap();
        province.remove("battlefield");
        province.remove("settlementLevel");
    }
    let restored = CampaignSave::from_json(&serde_json::to_string(&document).unwrap()).unwrap();
    let seed = restored.campaign.pending_tactical_battle_seed().unwrap();
    assert_eq!(
        seed.battlefield_profile,
        TacticalBattlefieldProfile::Field {
            location: BattlefieldLocation::MountainPass
        }
    );
    let mut document = serde_json::to_value(&seed).unwrap();
    document
        .as_object_mut()
        .unwrap()
        .remove("battlefieldProfile");
    let legacy_seed: medieval_core::TacticalBattleSeed = serde_json::from_value(document).unwrap();
    assert_eq!(legacy_seed, seed);
    assert!(
        TacticalBattle::from_campaign_seed(FlatBattlefield::new(120_000, 80_000), legacy_seed)
            .unwrap()
            .siege_snapshot()
            .is_none()
    );
}
