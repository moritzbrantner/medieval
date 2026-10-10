//! Operation-level work contracts: recurring tactical actions must scale with
//! the units they actually touch, not with the unrelated population.
//!
//! Every fixture keeps the same active set — one melee engagement already in
//! contact and one marching unit — and adds idle, order-free units far from
//! it. Movement, combat-pulse and order-change work must then be identical at
//! every population; only the per-tick movement visit scan is declared linear
//! in total units.
use medieval_core::{
    BattlePoint, BattleSide, BattlefieldLocation, FlatBattlefield, Formation, MovementOrder,
    TacticalBattle, TacticalUnit, TacticalWorkCounters, UnitCombatProfile, UnitKind,
};

const POPULATIONS: [u32; 3] = [4, 32, 128];

fn unit(id: String, side: BattleSide, position: BattlePoint) -> TacticalUnit {
    TacticalUnit::new(id, side, 120, position, Formation::Line { files: 24 }, 100)
        .with_combat_stats(UnitCombatProfile::v1(UnitKind::Levy))
}

/// The fixed active set plus `idle_pairs` idle units per side.
fn battle(idle_pairs: u32) -> TacticalBattle {
    let mut units = vec![
        unit(
            "a-active-melee".into(),
            BattleSide::Attacker,
            BattlePoint::new(49_500, 5_000),
        ),
        unit(
            "d-active-melee".into(),
            BattleSide::Defender,
            BattlePoint::new(50_500, 5_000),
        ),
        unit(
            "a-active-march".into(),
            BattleSide::Attacker,
            BattlePoint::new(20_000, 30_000),
        ),
    ];
    for index in 0..idle_pairs {
        let y = 120_000 + index * 2_000;
        units.push(unit(
            format!("a-idle-{index:03}"),
            BattleSide::Attacker,
            BattlePoint::new(10_000, y),
        ));
        units.push(unit(
            format!("d-idle-{index:03}"),
            BattleSide::Defender,
            BattlePoint::new(90_000, y),
        ));
    }
    let mut battle = TacticalBattle::new_at_location(
        FlatBattlefield::new(100_000, 400_000),
        units,
        BattlefieldLocation::ForestClearing,
    )
    .unwrap()
    .start();
    battle
        .issue_engagement_order("a-active-melee", "d-active-melee")
        .unwrap();
    battle
        .issue_engagement_order("d-active-melee", "a-active-melee")
        .unwrap();
    battle
        .issue_move_order(MovementOrder {
            unit_id: "a-active-march".into(),
            destination: BattlePoint::new(20_000, 90_000),
        })
        .unwrap();
    battle
}

/// Movement, combat pulse, and the tick after one retarget, measured
/// separately. Ticks 1–19 have no combat pulse; tick 20 does.
fn operations(idle_pairs: u32) -> [(&'static str, TacticalWorkCounters); 3] {
    let mut battle = battle(idle_pairs);
    let movement = battle.advance_ticks_measured(19);
    let combat = battle.advance_ticks_measured(1);
    battle
        .issue_engagement_order("a-active-march", "d-active-melee")
        .unwrap();
    let retarget = battle.advance_ticks_measured(1);
    [
        ("movement", movement),
        ("combat", combat),
        ("retarget", retarget),
    ]
}

fn without_linear_visit_scan(mut counters: TacticalWorkCounters) -> TacticalWorkCounters {
    counters.movement_unit_visits = 0;
    counters
}

#[test]
fn fixtures_vary_only_the_unrelated_population() {
    for idle_pairs in POPULATIONS {
        let battle = battle(idle_pairs);
        assert_eq!(battle.units().len() as u32, 3 + 2 * idle_pairs);
        let active: Vec<_> = battle
            .units()
            .iter()
            .filter(|unit| unit.id().contains("active"))
            .map(TacticalUnit::id)
            .collect();
        assert_eq!(
            active,
            ["a-active-march", "a-active-melee", "d-active-melee"]
        );
    }
}

#[test]
fn recurring_tactical_work_is_independent_of_unrelated_units() {
    let reference = operations(POPULATIONS[0]);
    for idle_pairs in &POPULATIONS[1..] {
        for ((name, expected), (_, actual)) in reference.iter().zip(operations(*idle_pairs)) {
            assert_eq!(
                without_linear_visit_scan(actual),
                without_linear_visit_scan(*expected),
                "{name} work changed with {idle_pairs} idle unit pairs"
            );
        }
    }
}

#[test]
fn recurring_actions_never_materialize_the_whole_unit_vector() {
    for idle_pairs in POPULATIONS {
        for (name, work) in operations(idle_pairs) {
            assert_eq!(work.snapshot_clones, 0, "{name}");
            // Only the three active units are copied before their own step.
            assert_eq!(work.snapshot_unit_copies, 3 * work.ticks, "{name}");
        }
    }
    let [(_, movement), (_, combat), _] = operations(POPULATIONS[0]);
    assert!(movement.combat_pulses == 0 && combat.combat_pulses == 1);
    assert!(combat.melee_contacts > 0);
}

#[test]
fn the_movement_visit_scan_is_the_declared_linear_cost() {
    for idle_pairs in POPULATIONS {
        let units = u64::from(3 + 2 * idle_pairs);
        for (name, work) in operations(idle_pairs) {
            assert_eq!(work.movement_unit_visits, work.ticks * units, "{name}");
        }
    }
}

#[test]
fn idle_population_does_not_change_the_active_outcome() {
    let active_state = |idle_pairs| {
        let mut battle = battle(idle_pairs);
        battle.advance_ticks(200);
        battle
            .units()
            .iter()
            .filter(|unit| unit.id().contains("active"))
            .cloned()
            .collect::<Vec<_>>()
    };
    let reference = active_state(POPULATIONS[0]);
    for idle_pairs in &POPULATIONS[1..] {
        assert_eq!(active_state(*idle_pairs), reference);
    }
}
