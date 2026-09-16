use std::time::Instant;

use medieval_core::{
    BattlePoint, BattleSide, FlatBattlefield, Formation, TacticalBattle, TacticalUnit,
};

const RUNS: usize = 3;
const TICKS: u32 = 2_400;
const UNITS_PER_SIDE: u32 = 6;

fn unit(id: String, side: BattleSide, x: u32, y: u32) -> TacticalUnit {
    TacticalUnit::new(
        id,
        side,
        120,
        BattlePoint::new(x, y),
        Formation::Line { files: 24 },
        1_200,
    )
}

fn run_battle() -> TacticalBattle {
    let battlefield = FlatBattlefield::new(300_000, 200_000);
    let mut units = Vec::new();
    for index in 0..UNITS_PER_SIDE {
        let y = 25_000 + index * 25_000;
        units.push(unit(
            format!("attacker-{index}"),
            BattleSide::Attacker,
            20_000,
            y,
        ));
        units.push(unit(
            format!("defender-{index}"),
            BattleSide::Defender,
            280_000,
            y,
        ));
    }

    let mut battle = TacticalBattle::deploy(battlefield, units)
        .expect("performance fixture must remain a valid deployed battle");
    for index in 0..UNITS_PER_SIDE {
        battle
            .issue_engagement_order(
                &format!("attacker-{index}"),
                &format!("defender-{index}"),
            )
            .expect("attacker engagement must remain valid");
        battle
            .issue_engagement_order(
                &format!("defender-{index}"),
                &format!("attacker-{index}"),
            )
            .expect("defender engagement must remain valid");
    }
    battle.advance_ticks(TICKS);
    battle
}

fn main() {
    let mut elapsed_ns = Vec::with_capacity(RUNS);
    let mut expected = None;

    for _ in 0..RUNS {
        let started = Instant::now();
        let battle = run_battle();
        elapsed_ns.push(started.elapsed().as_nanos());
        if let Some(reference) = &expected {
            assert_eq!(&battle, reference, "medieval tactical benchmark became nondeterministic");
        } else {
            expected = Some(battle);
        }
    }

    elapsed_ns.sort_unstable();
    let battle = expected.expect("at least one run");
    let surviving = battle.units().iter().filter(|unit| !unit.is_destroyed()).count();
    println!(
        "scenario=tactical-engagement units={} ticks={TICKS} runs={RUNS} median_elapsed_ns={} surviving_units={} deterministic=true timing=advisory-shared-runner",
        UNITS_PER_SIDE * 2,
        elapsed_ns[RUNS / 2],
        surviving,
    );
}
