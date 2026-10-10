use medieval_core::{
    BattleSide, BattlefieldLocation, FlatBattlefield, SiegeProfile, TacticalBattle,
    TacticalBattlefieldProfile, TacticalError,
};

fn seed() -> medieval_core::TacticalBattleSeed {
    let mut campaign = medieval_core::new_campaign();
    campaign.move_army("england-main", "paris").unwrap();
    campaign.pending_tactical_battle_seed().unwrap()
}

#[test]
fn every_field_and_siege_profile_deploys_complete_nonoverlapping_formations() {
    for location in BattlefieldLocation::ALL {
        let sieges = SiegeProfile::ALL.map(|fortification| TacticalBattlefieldProfile::Siege {
            location,
            fortification,
        });
        for profile in std::iter::once(TacticalBattlefieldProfile::Field { location }).chain(sieges)
        {
            let field = FlatBattlefield::new(120_000, 80_000);
            let battle =
                TacticalBattle::from_campaign_seed_with_profile(field, seed(), profile).unwrap();
            assert_eq!(battle.terrain().location(), location);
            assert_eq!(
                battle.siege_snapshot().is_some(),
                matches!(profile, TacticalBattlefieldProfile::Siege { .. })
            );
            for unit in battle.units() {
                let footprint = unit.formation_footprint();
                let zone = battle
                    .deployment_zones()
                    .into_iter()
                    .find(|zone| zone.side == unit.side())
                    .unwrap();
                assert!(footprint.is_inside(zone));
                assert!(battle.terrain().is_passable_at(field, unit.position()));
                if let Some(siege) = battle.siege_snapshot() {
                    assert!(siege.is_passable_at(unit.position()));
                }
                assert_eq!(
                    unit.initial_facing_millidegrees(),
                    match unit.side() {
                        BattleSide::Attacker => 0,
                        BattleSide::Defender => 180_000,
                    }
                );
            }
            for (index, left) in battle.units().iter().enumerate() {
                for right in &battle.units()[index + 1..] {
                    assert!(
                        !left
                            .formation_footprint()
                            .overlaps(right.formation_footprint())
                    );
                }
            }
        }
    }
}

#[test]
fn deployment_and_overflow_are_deterministic_under_seed_reordering() {
    let mut reordered = seed();
    reordered.attacker.units.reverse();
    reordered.defender.units.reverse();
    let profile = TacticalBattlefieldProfile::Field {
        location: BattlefieldLocation::RiverFord,
    };
    for field in [
        FlatBattlefield::new(120_000, 80_000),
        FlatBattlefield::new(120_000, 1_000),
    ] {
        let first = TacticalBattle::from_campaign_seed_with_profile(field, seed(), profile);
        let second =
            TacticalBattle::from_campaign_seed_with_profile(field, reordered.clone(), profile);
        assert_eq!(first, second);
        if field.depth_mm == 1_000 {
            assert!(matches!(first, Err(TacticalError::DeploymentFull { .. })));
        } else {
            assert!(first.is_ok());
        }
    }
}

#[test]
fn formation_footprint_uses_every_rank_instead_of_only_its_center() {
    let battle =
        TacticalBattle::from_campaign_seed(FlatBattlefield::new(120_000, 80_000), seed()).unwrap();
    let levy = battle
        .units()
        .iter()
        .find(|unit| {
            unit.side() == BattleSide::Attacker
                && unit.unit_kind() == Some(medieval_core::UnitKind::Levy)
        })
        .unwrap();
    let footprint = levy.formation_footprint();
    assert_eq!(footprint.max_x_mm - footprint.min_x_mm, 10_000);
    assert_eq!(footprint.max_y_mm - footprint.min_y_mm, 12_000);
}
