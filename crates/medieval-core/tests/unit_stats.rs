use medieval_core::{
    BattlePoint, BattleSide, FlatBattlefield, Formation, TacticalBattle, TacticalUnit,
    UnitCombatProfile, UnitKind, UnitStatsVersion,
};

fn unit(id: &str, side: BattleSide, x: u32, kind: UnitKind) -> TacticalUnit {
    TacticalUnit::new(
        id,
        side,
        320,
        BattlePoint::new(x, 20_000),
        Formation::Line { files: 80 },
        100,
    )
    .with_combat_stats(UnitCombatProfile::v1(kind))
}

fn combat(attacker: UnitKind, defender: UnitKind, reverse: bool) -> TacticalBattle {
    let mut units = vec![
        unit("a", BattleSide::Attacker, 20_000, attacker),
        unit("d", BattleSide::Defender, 21_000, defender),
    ];
    if reverse {
        units.reverse();
    }
    let mut battle = TacticalBattle::new(FlatBattlefield::new(100_000, 100_000), units).unwrap();
    battle.issue_engagement_order("a", "d").unwrap();
    battle
}

#[test]
fn every_kind_has_explicit_bounded_v1_stats_and_applicable_missiles() {
    for kind in [
        UnitKind::Levy,
        UnitKind::Spearmen,
        UnitKind::Archers,
        UnitKind::Knights,
    ] {
        let profile = UnitCombatProfile::v1(kind);
        assert_eq!(profile.version, UnitStatsVersion::UnitStatsV1);
        let stats = profile.stats();
        assert!(stats.melee_attack_milli > 0);
        assert!(stats.defense_milli > 0);
        assert!(stats.initial_morale <= 1_000 && stats.initial_morale > 250);
        assert!(stats.movement_mm_per_tick > 0);
        assert!(stats.formation_resistance_milli > 0);
        assert_eq!(stats.missile.is_some(), kind == UnitKind::Archers);
        assert_eq!(stats.charge_impact_milli > 0, kind == UnitKind::Knights);
        if let Some(missile) = stats.missile {
            assert_eq!(missile.range_mm, 25_000);
            assert!(missile.damage_milli > 0 && missile.ammunition > 0);
        }
    }
}

#[test]
fn initialization_uses_core_morale_movement_and_missile_range() {
    for kind in [
        UnitKind::Levy,
        UnitKind::Spearmen,
        UnitKind::Archers,
        UnitKind::Knights,
    ] {
        let unit = unit("unit", BattleSide::Attacker, 20_000, kind);
        let stats = unit.stats().unwrap();
        assert_eq!(unit.morale(), stats.initial_morale);
        assert_eq!(unit.speed_mm_per_tick(), stats.movement_mm_per_tick);
        assert_eq!(
            unit.attack_range_mm(),
            stats.missile.map_or(1_500, |missile| missile.range_mm)
        );
    }
}

#[test]
fn melee_attack_and_defensive_stats_change_simultaneous_casualties() {
    let mut levy = combat(UnitKind::Levy, UnitKind::Levy, false);
    let mut knights = combat(UnitKind::Knights, UnitKind::Levy, false);
    levy.advance_ticks(20);
    knights.advance_ticks(20);
    let survivors = |battle: &TacticalBattle, id| {
        battle
            .units()
            .iter()
            .find(|unit| unit.id() == id)
            .unwrap()
            .soldiers()
    };
    assert!(survivors(&knights, "d") < survivors(&levy, "d"));
    assert!(survivors(&knights, "a") > survivors(&levy, "a"));
}

#[test]
fn ranged_armor_resistance_is_deterministic_without_melee_contact() {
    let make = |kind| {
        let mut battle = TacticalBattle::new(
            FlatBattlefield::new(100_000, 100_000),
            vec![
                unit("a", BattleSide::Attacker, 10_000, UnitKind::Archers),
                unit("d", BattleSide::Defender, 25_000, kind),
            ],
        )
        .unwrap();
        battle.issue_engagement_order("a", "d").unwrap();
        battle.advance_ticks(20);
        battle
    };
    let levy = make(UnitKind::Levy);
    let knights = make(UnitKind::Knights);
    let survivors = |battle: &TacticalBattle| {
        battle
            .units()
            .iter()
            .find(|unit| unit.id() == "d")
            .unwrap()
            .soldiers()
    };
    assert!(survivors(&knights) > survivors(&levy));
    assert!(survivors(&levy) < 320);
}

#[test]
fn profiles_replay_identically_across_storage_order_serialization_and_tick_partition() {
    let mut direct = combat(UnitKind::Knights, UnitKind::Spearmen, false);
    let document =
        serde_json::to_string(&combat(UnitKind::Knights, UnitKind::Spearmen, true)).unwrap();
    let mut replay: TacticalBattle = serde_json::from_str(&document).unwrap();
    direct.advance_ticks(30);
    for _ in 0..3 {
        replay.advance_ticks(10);
    }
    assert_eq!(direct, replay);
}

#[test]
fn historical_kind_metadata_retains_legacy_combat_without_a_profile() {
    let base = TacticalUnit::new(
        "a",
        BattleSide::Attacker,
        80,
        BattlePoint::new(20_000, 20_000),
        Formation::Line { files: 20 },
        100,
    );
    let legacy = base.clone().with_unit_kind(UnitKind::Archers);
    let document = serde_json::to_string(&legacy).unwrap();
    assert!(!document.contains("combatProfile"));
    let decoded: TacticalUnit = serde_json::from_str(&document).unwrap();
    assert_eq!(decoded, legacy);
    assert!(decoded.stats().is_none());
    assert_eq!(decoded.speed_mm_per_tick(), base.speed_mm_per_tick());
    assert_eq!(decoded.attack_range_mm(), base.attack_range_mm());
    let defenders = TacticalUnit::new(
        "d",
        BattleSide::Defender,
        80,
        BattlePoint::new(21_000, 20_000),
        Formation::Line { files: 20 },
        100,
    );
    let mut untyped = TacticalBattle::new(
        FlatBattlefield::new(100_000, 100_000),
        vec![base, defenders.clone()],
    )
    .unwrap();
    let mut typed_legacy = TacticalBattle::new(
        FlatBattlefield::new(100_000, 100_000),
        vec![decoded, defenders],
    )
    .unwrap();
    for battle in [&mut untyped, &mut typed_legacy] {
        battle.issue_engagement_order("a", "d").unwrap();
        battle.advance_ticks(20);
    }
    assert_eq!(
        untyped.units()[0].soldiers(),
        typed_legacy.units()[0].soldiers()
    );
    assert_eq!(
        untyped.units()[0].morale(),
        typed_legacy.units()[0].morale()
    );
    assert_eq!(untyped.units()[1], typed_legacy.units()[1]);
}

#[test]
fn unsupported_stat_versions_are_rejected_before_combat() {
    let mut document =
        serde_json::to_value(unit("a", BattleSide::Attacker, 20_000, UnitKind::Knights)).unwrap();
    document["combatProfile"]["version"] = serde_json::json!("unitStatsV99");
    assert!(serde_json::from_value::<TacticalUnit>(document).is_err());
}

#[test]
fn campaign_units_project_their_authoritative_stat_profile() {
    let mut campaign = medieval_core::new_campaign();
    campaign.move_army("england-main", "paris").unwrap();
    let battle = TacticalBattle::from_campaign_seed(
        FlatBattlefield::new(120_000, 80_000),
        campaign.pending_tactical_battle_seed().unwrap(),
    )
    .unwrap();
    for unit in battle.units() {
        assert_eq!(
            unit.combat_profile(),
            Some(UnitCombatProfile::v1(unit.unit_kind().unwrap()))
        );
        assert_eq!(unit.stats(), Some(unit.combat_profile().unwrap().stats()));
    }
}

fn small_ranged_battle(defender_kind: UnitKind) -> TacticalBattle {
    let attacker = TacticalUnit::new(
        "a",
        BattleSide::Attacker,
        40,
        BattlePoint::new(10_000, 20_000),
        Formation::Line { files: 10 },
        100,
    )
    .with_combat_stats(UnitCombatProfile::v1(UnitKind::Archers));
    let defender = TacticalUnit::new(
        "d",
        BattleSide::Defender,
        80,
        BattlePoint::new(25_000, 20_000),
        Formation::Line { files: 10 },
        100,
    )
    .with_combat_stats(UnitCombatProfile::v1(defender_kind));
    let mut battle = TacticalBattle::new(
        FlatBattlefield::new(100_000, 100_000),
        vec![attacker, defender],
    )
    .unwrap();
    battle.issue_engagement_order("a", "d").unwrap();
    battle
}

#[test]
fn default_campaign_frontage_exposes_armor_over_successive_volleys() {
    let mut levy = small_ranged_battle(UnitKind::Levy);
    let mut knights = small_ranged_battle(UnitKind::Knights);
    levy.advance_ticks(120);
    knights.advance_ticks(120);
    let survivors = |battle: &TacticalBattle| {
        battle
            .units()
            .iter()
            .find(|unit| unit.id() == "d")
            .unwrap()
            .soldiers()
    };
    assert!(survivors(&knights) > survivors(&levy));
    assert!(survivors(&levy) < 80);
}

#[test]
fn fractional_ranged_damage_survives_serialization_and_tick_partition() {
    let mut direct = small_ranged_battle(UnitKind::Knights);
    direct.advance_ticks(20);
    let document = serde_json::to_value(&direct).unwrap();
    assert!(document["rangedDamageCredit"]["a"]["d"].as_u64().unwrap() > 0);
    let mut replay: TacticalBattle = serde_json::from_value(document.clone()).unwrap();
    direct.advance_ticks(100);
    for _ in 0..5 {
        replay.advance_ticks(20);
    }
    assert_eq!(direct, replay);
    let mut corrupt = document;
    corrupt["rangedDamageCredit"]["a"]["d"] = serde_json::json!(1_000);
    assert!(serde_json::from_value::<TacticalBattle>(corrupt).is_err());
}
