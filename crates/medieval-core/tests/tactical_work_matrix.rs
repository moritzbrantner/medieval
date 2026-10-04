mod support {
    pub mod tactical_scenarios;
}
use medieval_core::{BattleSide, TacticalBattleState, TacticalFinishReason, TacticalWorkCounters};
use support::tactical_scenarios::{SCENARIOS, Scenario};

#[test]
fn scenarios_preserve_state_and_counters_across_replay_chunking_and_serialization() {
    for scenario in SCENARIOS {
        let seed = scenario.build();
        let mut plain = seed.clone();
        plain.advance_ticks(scenario.ticks());
        let mut measured = seed.clone();
        let work = measured.advance_ticks_measured(scenario.ticks());
        assert_eq!(plain, measured, "{}", scenario.name());
        let restored: medieval_core::TacticalBattle =
            serde_json::from_str(&serde_json::to_string(&measured).unwrap()).unwrap();
        assert_eq!(restored, measured);
        let mut replay = seed.clone();
        let replay_work = replay.advance_ticks_measured(scenario.ticks());
        assert_eq!(replay, measured);
        assert_eq!(replay_work, work);

        let mut chunked = seed;
        let mut totals = serde_json::to_value(TacticalWorkCounters::default()).unwrap();
        for _ in 0..scenario.ticks() {
            let next = serde_json::to_value(chunked.advance_ticks_measured(1)).unwrap();
            for (key, value) in next.as_object().unwrap() {
                totals[key] =
                    serde_json::json!(totals[key].as_u64().unwrap() + value.as_u64().unwrap());
            }
        }
        assert_eq!(chunked, measured);
        assert_eq!(totals, serde_json::to_value(work).unwrap());
        assert_eq!(work.ticks, measured.tick());
        assert_eq!(
            work.snapshot_unit_copies,
            work.snapshot_clones * measured.units().len() as u64
        );
        assert_eq!(
            work.movement_unit_visits,
            work.ticks * measured.units().len() as u64
        );
        assert!(work.physics_contact_queries <= work.proximity_queries);
        assert!(work.ticks <= u64::from(scenario.ticks()));
    }
}

#[test]
fn gameplay_invariants_are_checked_separately_from_work_budgets() {
    for scenario in SCENARIOS {
        let mut battle = scenario.build();
        let initial_soldiers: u64 = battle
            .units()
            .iter()
            .map(|unit| u64::from(unit.soldiers()))
            .sum();
        let initial_units = battle.units().to_vec();
        battle.advance_ticks(scenario.ticks());
        let final_soldiers: u64 = battle
            .units()
            .iter()
            .map(|unit| u64::from(unit.soldiers()))
            .sum();
        assert!(final_soldiers <= initial_soldiers);
        for (before, unit) in initial_units.iter().zip(battle.units()) {
            assert_eq!(unit.id(), before.id());
            assert!(unit.soldiers() <= before.soldiers());
            assert!(unit.morale() <= 1_000);
            assert!(unit.fatigue() <= 1_000);
            assert!(unit.position().x_mm <= battle.battlefield().width_mm);
            assert!(unit.position().y_mm <= battle.battlefield().depth_mm);
            assert!(
                battle
                    .terrain()
                    .is_passable_at(battle.battlefield(), unit.position())
            );
            assert!(
                unit.ammunition()
                    .zip(before.ammunition())
                    .is_none_or(|(after, before)| after <= before)
            );
        }
        match scenario {
            Scenario::Small => {
                assert!(matches!(
                    battle.state(),
                    TacticalBattleState::Finished {
                        winner: Some(BattleSide::Attacker),
                        reason: TacticalFinishReason::ForceDefeated,
                        ..
                    }
                ));
                let before = battle.clone();
                assert_eq!(
                    battle.advance_ticks_measured(100),
                    TacticalWorkCounters::default()
                );
                assert_eq!(battle, before);
            }
            Scenario::Idle => assert_eq!(battle.units(), initial_units),
            Scenario::RangedHeavy => {
                assert!(final_soldiers < initial_soldiers);
                assert!(
                    battle
                        .units()
                        .iter()
                        .all(|unit| unit.ammunition().unwrap() < 30)
                );
            }
            Scenario::Siege => {
                let siege = battle.siege_snapshot().unwrap();
                assert!(siege.gate_state.is_traversable());
                assert!(final_soldiers < initial_soldiers);
            }
            _ => assert!(final_soldiers < initial_soldiers),
        }
    }
}

#[test]
fn subsystem_work_stays_within_the_versioned_fixture_budgets() {
    let budgets: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/tactical-work-budgets-v1.json")).unwrap();
    assert_eq!(budgets["schemaVersion"], 1);
    assert_eq!(budgets["fixtureRevision"], 1);
    for scenario in SCENARIOS {
        let mut battle = scenario.build();
        let actual = serde_json::to_value(battle.advance_ticks_measured(scenario.ticks())).unwrap();
        let budget = &budgets["scenarios"][scenario.name()];
        assert_eq!(budget["units"], battle.units().len());
        assert_eq!(budget["requestedTicks"], scenario.ticks());
        for (metric, limit) in budget["maximumWork"].as_object().unwrap() {
            let actual = actual[metric].as_u64().unwrap();
            assert!(
                actual <= limit.as_u64().unwrap(),
                "{}: {metric} work {actual} exceeds {limit}",
                scenario.name()
            );
        }
    }
}
