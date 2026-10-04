use medieval_core::{FlatBattlefield, TacticalBattle, UnitKind};

fn seed() -> medieval_core::TacticalBattleSeed {
    let mut campaign = medieval_core::new_campaign();
    campaign.move_army("england-main", "paris").unwrap();
    campaign.pending_tactical_battle_seed().unwrap()
}

#[test]
fn campaign_composition_and_provenance_survive_battle_round_trip() {
    let seed = seed();
    let battle =
        TacticalBattle::from_campaign_seed(FlatBattlefield::new(120_000, 80_000), seed.clone())
            .unwrap();
    assert_eq!(battle.campaign_seed(), Some(&seed));
    assert_eq!(battle.units().len(), 8);
    for unit in battle.units() {
        let force = match unit.side() {
            medieval_core::BattleSide::Attacker => &seed.attacker,
            medieval_core::BattleSide::Defender => &seed.defender,
        };
        let expected = force
            .units
            .iter()
            .find(|entry| Some(entry.kind) == unit.unit_kind())
            .unwrap();
        assert_eq!(u64::from(unit.soldiers()), expected.soldiers);
    }
    let encoded = serde_json::to_string(&battle).unwrap();
    assert_eq!(
        serde_json::from_str::<TacticalBattle>(&encoded).unwrap(),
        battle
    );
}

#[test]
fn force_storage_order_does_not_change_battle_identity() {
    let mut first = seed();
    first.defender.source_army_ids.push("france-reserve".into());
    let mut reordered = first.clone();
    reordered.attacker.units.reverse();
    reordered.defender.units.reverse();
    reordered.defender.source_army_ids.reverse();
    let field = FlatBattlefield::new(120_000, 80_000);
    assert_eq!(
        TacticalBattle::from_campaign_seed(field, first).unwrap(),
        TacticalBattle::from_campaign_seed(field, reordered).unwrap()
    );
}

#[test]
fn large_force_counts_are_split_without_losing_soldiers() {
    let mut seed = seed();
    seed.attacker.units[0].soldiers = u64::from(u16::MAX) + 42;
    let battle =
        TacticalBattle::from_campaign_seed(FlatBattlefield::new(120_000, 80_000), seed).unwrap();
    let levy: Vec<_> = battle
        .units()
        .iter()
        .filter(|unit| {
            unit.side() == medieval_core::BattleSide::Attacker
                && unit.unit_kind() == Some(UnitKind::Levy)
        })
        .collect();
    assert_eq!(levy.len(), 2);
    assert_eq!(
        levy.iter()
            .map(|unit| u64::from(unit.soldiers()))
            .sum::<u64>(),
        u64::from(u16::MAX) + 42
    );
    assert_ne!(levy[0].id(), levy[1].id());
}

#[test]
fn sandbox_battles_still_round_trip_without_campaign_metadata() {
    let battle = TacticalBattle::new(FlatBattlefield::new(1_000, 1_000), vec![]).unwrap();
    assert!(battle.campaign_seed().is_none());
    let encoded = serde_json::to_string(&battle).unwrap();
    assert!(!encoded.contains("campaignSeed"));
    assert_eq!(
        serde_json::from_str::<TacticalBattle>(&encoded).unwrap(),
        battle
    );
}
