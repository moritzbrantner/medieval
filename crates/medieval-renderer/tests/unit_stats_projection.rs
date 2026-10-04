use medieval_core::{
    BattlePoint, BattleSide, FlatBattlefield, Formation, TacticalBattle, TacticalUnit,
    UnitCombatProfile, UnitKind,
};
use medieval_renderer::{BattleRenderSnapshot, RenderViewState};

#[test]
fn stat_projection_copies_core_values_without_changing_simulation() {
    let field = FlatBattlefield::new(100_000, 100_000);
    let unit = TacticalUnit::new(
        "archers",
        BattleSide::Attacker,
        80,
        BattlePoint::new(20_000, 20_000),
        Formation::Line { files: 20 },
        100,
    )
    .with_combat_stats(UnitCombatProfile::v1(UnitKind::Archers));
    let battle = TacticalBattle::new(field, vec![unit]).unwrap();
    let before = battle.clone();
    let snapshot = BattleRenderSnapshot::capture(&battle, &RenderViewState::fit(field));
    assert_eq!(snapshot.units[0].combat_stats, battle.units()[0].stats());
    assert_eq!(snapshot.units[0].unit_kind, Some(UnitKind::Archers));
    assert_eq!(battle, before);
}
