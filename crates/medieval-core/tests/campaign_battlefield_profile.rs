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
                TacticalBattlefieldProfile::Siege {
                    location,
                    fortification: medieval_core::SiegeProfile::StoneWallsV1,
                }
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
        province.remove("buildings");
        province.remove("recruitmentPool");
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

fn set_walls(campaign: &mut medieval_core::CampaignState, level: u8) {
    let province = campaign
        .provinces
        .iter_mut()
        .find(|province| province.id == "paris")
        .unwrap();
    province.battlefield.fortified = false;
    province
        .buildings
        .retain(|standing| standing.building != medieval_core::BuildingId::Walls);
    if level > 0 {
        province.buildings.push(medieval_core::ProvinceBuilding {
            building: medieval_core::BuildingId::Walls,
            level,
        });
        province.buildings.sort_by_key(|standing| standing.building);
    }
}

#[test]
fn walls_select_versioned_siege_profiles_with_retained_provenance() {
    use medieval_core::SiegeProfile;
    let location = BattlefieldLocation::ForestClearing;
    for (walls, expected) in [
        (0, None),
        (1, Some(SiegeProfile::PalisadeV1)),
        (2, Some(SiegeProfile::StoneWallsV1)),
    ] {
        let mut campaign = pending_campaign();
        campaign.provinces[4].battlefield.location = location;
        set_walls(&mut campaign, walls);
        let seed = campaign.pending_tactical_battle_seed().unwrap();
        assert_eq!(
            seed.battlefield_profile,
            expected.map_or(
                TacticalBattlefieldProfile::Field { location },
                |fortification| {
                    TacticalBattlefieldProfile::Siege {
                        location,
                        fortification,
                    }
                }
            ),
            "walls level {walls}"
        );
        let restored: medieval_core::TacticalBattleSeed =
            serde_json::from_str(&serde_json::to_string(&seed).unwrap()).unwrap();
        let battle =
            TacticalBattle::from_campaign_seed(FlatBattlefield::new(120_000, 80_000), restored)
                .unwrap();
        assert_eq!(battle.siege_snapshot().map(|siege| siege.profile), expected);
        assert_eq!(battle.campaign_seed().unwrap(), &seed);
        let encoded = serde_json::to_string(&battle).unwrap();
        assert_eq!(
            serde_json::from_str::<TacticalBattle>(&encoded).unwrap(),
            battle
        );
    }
}

#[test]
fn palisade_and_stone_walls_produce_different_core_geometry() {
    use medieval_core::SiegeProfile;
    let field = FlatBattlefield::new(120_000, 80_000);
    let mut layouts = Vec::new();
    for walls in [1, 2] {
        let mut campaign = pending_campaign();
        set_walls(&mut campaign, walls);
        let battle = TacticalBattle::from_campaign_seed(
            field,
            campaign.pending_tactical_battle_seed().unwrap(),
        )
        .unwrap();
        layouts.push(battle.siege_snapshot().unwrap());
    }
    assert_eq!(layouts[0].profile, SiegeProfile::PalisadeV1);
    assert_eq!(layouts[1].profile, SiegeProfile::StoneWallsV1);
    assert_ne!(layouts[0].layout, layouts[1].layout);
    // The defined stone fortification of Paris keeps the original layout.
    let defined = TacticalBattle::from_campaign_seed(
        field,
        pending_campaign().pending_tactical_battle_seed().unwrap(),
    )
    .unwrap();
    assert_eq!(defined.siege_snapshot().unwrap().layout, layouts[1].layout);
}

#[test]
fn siege_seeds_recorded_before_profiles_load_as_stone_walls() {
    let seed = pending_campaign().pending_tactical_battle_seed().unwrap();
    let mut document = serde_json::to_value(&seed).unwrap();
    assert_eq!(
        document["battlefieldProfile"]["fortification"],
        "stoneWallsV1"
    );
    document["battlefieldProfile"]
        .as_object_mut()
        .unwrap()
        .remove("fortification");
    let legacy: medieval_core::TacticalBattleSeed = serde_json::from_value(document).unwrap();
    assert_eq!(legacy, seed);
}

#[test]
fn campaign_saves_with_walls_round_trip_and_reselect_the_profile() {
    let mut campaign = pending_campaign();
    set_walls(&mut campaign, 1);
    let save = CampaignSave::from_campaign(campaign, "england").unwrap();
    let loaded = CampaignSave::from_json(&save.to_json().unwrap()).unwrap();
    assert_eq!(loaded, save);
    assert_eq!(
        loaded.campaign.pending_tactical_battle_seed().unwrap(),
        save.campaign.pending_tactical_battle_seed().unwrap()
    );
}
