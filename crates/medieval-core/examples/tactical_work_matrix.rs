use std::{hint::black_box, time::Instant};

#[path = "../tests/support/tactical_scenarios.rs"]
mod tactical_scenarios;
use tactical_scenarios::SCENARIOS;

fn main() {
    // Timing includes construction and orders as well as measured tick advancement.
    // Work counters cover tick advancement only. Shared-runner timing is advisory.
    for scenario in SCENARIOS {
        let mut samples = Vec::new();
        let mut reference = None;
        let mut reference_work = None;
        for _ in 0..3 {
            let start = Instant::now();
            let mut battle = scenario.build();
            let work = battle.advance_ticks_measured(scenario.ticks());
            black_box(&battle);
            samples.push(start.elapsed().as_nanos());
            if let Some(expected) = &reference {
                assert_eq!(&battle, expected);
                assert_eq!(Some(work), reference_work);
            } else {
                reference = Some(battle);
                reference_work = Some(work);
            }
        }
        samples.sort_unstable();
        let battle = reference.unwrap();
        println!(
            "{}",
            serde_json::json!({
                "schemaVersion": 1,
                "fixtureRevision": 1,
                "scenario": scenario.name(),
                "units": battle.units().len(),
                "requestedTicks": scenario.ticks(),
                "finalState": battle.state(),
                "survivors": battle.units().iter().map(|unit| u64::from(unit.soldiers())).sum::<u64>(),
                "work": reference_work.unwrap(),
                "runs": samples.len(),
                "medianElapsedNs": samples[samples.len()/2],
                "timing": "advisory-shared-runner",
            })
        );
    }
}
