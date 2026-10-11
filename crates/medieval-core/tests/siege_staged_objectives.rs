//! Acceptance for #127: staged siege objectives and multiple capture points.
//!
//! Resolved contract (inferred from the #123–#126 conventions and the
//! "progressive defense" goal; see the PR record for the rationale):
//! - **Ordered objectives.** A siege profile owns an ordered list of capture
//!   objectives, outer to inner. Every existing single-point profile
//!   (`PalisadeV1`, `StoneWallsV1`) is a one-objective list whose objective is
//!   exactly `layout.capture_point` with the legacy `capture` state. A new
//!   versioned profile (profiles are never changed in place) defines at least
//!   two objectives on the defender side of the wall, each farther from the
//!   wall than the one before (outer works before the keep).
//! - **Staged unlock.** Only the first objective starts unlocked. A later
//!   objective is locked, and accrues nothing for anyone, until its
//!   predecessor has been captured by the attacker; it then unlocks.
//! - **Per-objective capture rule is the existing rule.** On each combat
//!   pulse an unlocked, uncaptured objective held by formed units of exactly
//!   one side gains progress for that side; contested objectives are left
//!   exactly unchanged (fail closed); a captured objective stays captured.
//! - **Victory: the final objective.** Capturing an earlier objective never
//!   finishes the battle. A started battle finishes with
//!   `TacticalFinishReason::SiegeCapture` on the pulse the final objective is
//!   captured, won by its captor. With one objective this is exactly the
//!   existing rule.
//! - **Persistence.** Objective state serializes with the battle and resumes
//!   exactly; single-point sieges keep the exact pre-#127 document and capture
//!   timeline (`tests/fixtures/siege-single-point-v1.json`, recorded on the
//!   baseline), so older saves load unchanged.
//!
//! Public seams these tests require (minimal, named here):
//! - `SiegeProfile::StagedKeepV1`, a new versioned multi-objective profile
//!   (serialized `stagedKeepV1`).
//! - `SiegeObjectiveStatus { center: BattlePoint, radius_mm: u32, progress:
//!   u16, capturing_side: Option<BattleSide>, captured_by: Option<BattleSide>,
//!   unlocked: bool }` (`Clone + Debug + PartialEq`), re-exported from the
//!   crate root.
//! - `TacticalBattle::siege_objectives(&self) -> Option<Vec<SiegeObjectiveStatus>>`:
//!   `None` outside a siege, otherwise objectives in order — the read model,
//!   like `siege_tower_activity`, `siege_ladders` and `siege_engines`.
//!
//! Deliberately left open: exact objective geometry and count beyond two,
//! whether a defender capture of an earlier objective unlocks the next one,
//! opponent-AI objective planning, campaign fortification-level mapping to the
//! new profile, the legacy `capture`/`capturePoint` fields' meaning for a
//! multi-objective profile, and the browser profile selector.

use medieval_core::{
    BattlePoint, BattleSide, BattlefieldLocation, FlatBattlefield, Formation, MovementOrder,
    SIEGE_CAPTURE_MAX_PROGRESS, SiegeObjectiveStatus, SiegeProfile, TACTICAL_TICKS_PER_SECOND,
    TacticalBattle, TacticalBattleState, TacticalFinishReason, TacticalUnit,
};
use serde_json::Value;

const PULSE: u64 = TACTICAL_TICKS_PER_SECOND as u64;
const FIELD: FlatBattlefield = FlatBattlefield::new(100_000, 100_000);
/// Liveness guard in combat pulses (10 pulses capture one objective).
const PULSE_LIMIT: u32 = 200;

fn unit(id: &str, side: BattleSide, x: u32, y: u32) -> TacticalUnit {
    TacticalUnit::new(
        id,
        side,
        100,
        BattlePoint::new(x, y),
        Formation::Line { files: 10 },
        1_000,
    )
}

fn objectives(battle: &TacticalBattle) -> Vec<SiegeObjectiveStatus> {
    battle
        .siege_objectives()
        .expect("a siege exposes its objectives")
}

fn distance(a: BattlePoint, b: BattlePoint) -> u64 {
    let dx = i64::from(a.x_mm) - i64::from(b.x_mm);
    let dy = i64::from(a.y_mm) - i64::from(b.y_mm);
    ((dx * dx + dy * dy) as f64).sqrt() as u64
}

/// Defender-side point at least 10 m outside every objective.
fn idle_defender_point(objectives: &[SiegeObjectiveStatus]) -> BattlePoint {
    [
        BattlePoint::new(99_000, 1_000),
        BattlePoint::new(99_000, 99_000),
        BattlePoint::new(70_000, 1_000),
        BattlePoint::new(70_000, 99_000),
    ]
    .into_iter()
    .find(|point| {
        objectives.iter().all(|objective| {
            distance(*point, objective.center) > u64::from(objective.radius_mm) + 10_000
        })
    })
    .expect("fixture: a defender corner lies away from every objective")
}

/// Staged siege with one attacker per objective (`a0`, `a1`, ...) waiting in
/// their zone and one idle defender (`guard`) away from every objective,
/// tower and wall. Units are later placed with [`place`] so no movement path
/// crosses an objective.
fn staged_siege() -> TacticalBattle {
    let deploy = |guard: BattlePoint, attackers: usize| {
        let mut units: Vec<TacticalUnit> = (0..attackers)
            .map(|index| {
                let y = 10_000
                    + 80_000 * u32::try_from(index).unwrap()
                        / u32::try_from(attackers.max(2) - 1).unwrap();
                unit(&format!("a{index}"), BattleSide::Attacker, 10_000, y)
            })
            .collect();
        units.push(unit("guard", BattleSide::Defender, guard.x_mm, guard.y_mm));
        TacticalBattle::deploy_siege_with_profile(
            FIELD,
            units,
            BattlefieldLocation::MountainPass,
            SiegeProfile::StagedKeepV1,
        )
        .unwrap()
    };
    let probe = objectives(&deploy(BattlePoint::new(99_000, 1_000), 2));
    deploy(idle_defender_point(&probe), probe.len())
}

/// Places a unit at `point` with no pending movement, through the saved
/// document (the public persistence path), so no route crosses an objective.
fn place(battle: &mut TacticalBattle, id: &str, point: BattlePoint) {
    let mut document = serde_json::to_value(&*battle).unwrap();
    let unit = document["units"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|unit| unit["id"] == id)
        .expect("fixture unit exists");
    unit["position"] = serde_json::to_value(point).unwrap();
    unit["destination"] = Value::Null;
    *battle = serde_json::from_value(document).unwrap();
}

/// Advances to and through the next combat pulse.
fn next_pulse(battle: &mut TacticalBattle) {
    let ticks = PULSE - battle.tick() % PULSE;
    battle.advance_ticks(u32::try_from(ticks).unwrap());
}

fn find<'a>(battle: &'a TacticalBattle, id: &str) -> &'a TacticalUnit {
    battle.units().iter().find(|unit| unit.id() == id).unwrap()
}

fn formed(unit: &TacticalUnit) -> bool {
    !(unit.is_routed() || unit.is_destroyed() || unit.is_escaped() || unit.is_withdrawing())
}

fn pulse_until(battle: &mut TacticalBattle, done: impl Fn(&TacticalBattle) -> bool) {
    for _ in 0..PULSE_LIMIT {
        if done(battle) {
            return;
        }
        next_pulse(battle);
    }
    panic!("condition not reached within the liveness limit");
}

fn captured_by_attacker(objective: &SiegeObjectiveStatus) -> bool {
    objective.captured_by == Some(BattleSide::Attacker)
}

fn untouched(objective: &SiegeObjectiveStatus) -> bool {
    objective.progress == 0 && objective.capturing_side.is_none() && objective.captured_by.is_none()
}

#[test]
fn staged_profile_exposes_ordered_core_owned_objectives() {
    let battle = staged_siege();
    let siege = battle.siege_snapshot().unwrap();
    assert_eq!(siege.profile, SiegeProfile::StagedKeepV1);
    assert_eq!(
        serde_json::to_value(SiegeProfile::StagedKeepV1).unwrap(),
        "stagedKeepV1"
    );
    let wall_max_x = siege.layout.wall_segments[0].max_x_mm;
    let all = objectives(&battle);
    assert!(all.len() >= 2, "a staged profile has several objectives");
    assert!(all.iter().all(untouched), "a fresh siege has no progress");
    assert!(all[0].unlocked, "the first objective starts unlocked");
    assert!(
        all[1..].iter().all(|objective| !objective.unlocked),
        "later objectives start locked"
    );
    for objective in &all {
        assert!(objective.radius_mm > 0);
        assert!(
            objective.center.x_mm > wall_max_x + objective.radius_mm,
            "objectives lie entirely on the defender side of the wall"
        );
        assert!(
            objective.center.x_mm + objective.radius_mm <= FIELD.width_mm
                && objective.center.y_mm >= objective.radius_mm
                && objective.center.y_mm + objective.radius_mm <= FIELD.depth_mm,
            "objectives lie inside the battlefield"
        );
    }
    for (index, left) in all.iter().enumerate() {
        for right in &all[index + 1..] {
            assert!(
                distance(left.center, right.center)
                    > u64::from(left.radius_mm) + u64::from(right.radius_mm),
                "objective areas are disjoint"
            );
        }
    }
    for pair in all.windows(2) {
        assert!(
            pair[1].center.x_mm > pair[0].center.x_mm,
            "objectives are ordered outer to inner, away from the wall"
        );
    }
    assert_eq!(
        objectives(&staged_siege()),
        all,
        "the layout is deterministic"
    );
}

#[test]
fn field_battles_have_no_objectives() {
    let battle = TacticalBattle::new(
        FIELD,
        vec![
            unit("a", BattleSide::Attacker, 10_000, 10_000),
            unit("d", BattleSide::Defender, 90_000, 90_000),
        ],
    )
    .unwrap();
    assert_eq!(battle.siege_objectives(), None);
}

#[test]
fn single_point_profiles_are_one_objective_mirroring_the_legacy_capture_state() {
    for profile in [SiegeProfile::PalisadeV1, SiegeProfile::StoneWallsV1] {
        let mut battle = TacticalBattle::deploy_siege_with_profile(
            FIELD,
            vec![
                unit("a", BattleSide::Attacker, 30_000, 50_000),
                unit("d", BattleSide::Defender, 100_000, 100_000),
            ],
            BattlefieldLocation::MountainPass,
            profile,
        )
        .unwrap()
        .start();
        battle.open_siege_gate().unwrap();
        battle
            .issue_move_order(MovementOrder {
                unit_id: "a".into(),
                destination: BattlePoint::new(80_000, 50_000),
            })
            .unwrap();
        for _ in 0..PULSE_LIMIT {
            let siege = battle.siege_snapshot().unwrap();
            let expected = SiegeObjectiveStatus {
                center: siege.layout.capture_point.center,
                radius_mm: siege.layout.capture_point.radius_mm,
                progress: siege.capture.progress,
                capturing_side: siege.capture.capturing_side,
                captured_by: siege.capture.captured_by,
                unlocked: true,
            };
            assert_eq!(objectives(&battle), vec![expected], "{profile:?}");
            if battle.state() != TacticalBattleState::Running {
                break;
            }
            next_pulse(&mut battle);
        }
        assert!(matches!(
            battle.state(),
            TacticalBattleState::Finished {
                winner: Some(BattleSide::Attacker),
                reason: TacticalFinishReason::SiegeCapture,
                ..
            }
        ));
    }
}

#[test]
fn a_locked_objective_accrues_nothing_until_its_predecessor_is_captured() {
    let mut battle = staged_siege();
    let inner = objectives(&battle)[1].center;
    place(&mut battle, "a1", inner);
    for _ in 0..30 {
        next_pulse(&mut battle);
    }
    let all = objectives(&battle);
    assert!(
        all.iter().all(untouched),
        "an attacker alone on a locked objective captures nothing: {all:?}"
    );
    assert!(!all[1].unlocked);

    // A defender alone on a locked objective accrues nothing either.
    let mut defended = staged_siege();
    place(&mut defended, "guard", inner);
    for _ in 0..30 {
        next_pulse(&mut defended);
    }
    assert!(objectives(&defended)[1..].iter().all(untouched));
}

#[test]
fn objectives_are_captured_in_order_and_each_capture_unlocks_the_next() {
    let mut battle = staged_siege();
    let all = objectives(&battle);
    place(&mut battle, "a0", all[0].center);
    place(&mut battle, "a1", all[1].center);

    pulse_until(&mut battle, |battle| {
        captured_by_attacker(&objectives(battle)[0])
    });
    let at_capture = objectives(&battle);
    assert_eq!(at_capture[0].progress, SIEGE_CAPTURE_MAX_PROGRESS);
    assert!(
        at_capture[1].unlocked,
        "capturing an objective unlocks the next"
    );
    assert!(
        at_capture[2..].iter().all(|objective| !objective.unlocked),
        "only the next objective unlocks"
    );

    // From here the next attacker progresses monotonically to capture.
    let mut previous = at_capture;
    loop {
        next_pulse(&mut battle);
        let now = objectives(&battle);
        assert!(captured_by_attacker(&now[0]), "a capture is permanent");
        assert!(now[1].progress >= previous[1].progress);
        previous = now;
        if captured_by_attacker(&previous[1]) {
            break;
        }
        assert!(battle.tick() < PULSE * u64::from(PULSE_LIMIT));
    }
    assert_eq!(previous[1].progress, SIEGE_CAPTURE_MAX_PROGRESS);
}

#[test]
fn a_captured_objective_stays_captured_when_the_enemy_reoccupies_it() {
    let mut battle = staged_siege();
    let outer = objectives(&battle)[0].center;
    place(&mut battle, "a0", outer);
    pulse_until(&mut battle, |battle| {
        captured_by_attacker(&objectives(battle)[0])
    });
    let captured = objectives(&battle)[0].clone();
    place(&mut battle, "a0", BattlePoint::new(10_000, 30_000));
    place(&mut battle, "guard", outer);
    for _ in 0..20 {
        next_pulse(&mut battle);
    }
    assert_eq!(objectives(&battle)[0], captured);
}

#[test]
fn contested_objective_capture_fails_closed() {
    let mut battle = staged_siege();
    let outer = objectives(&battle)[0].center;
    place(&mut battle, "a0", outer);
    for _ in 0..3 {
        next_pulse(&mut battle);
    }
    let before = objectives(&battle);
    assert!(before[0].progress > 0, "fixture: partial attacker progress");
    assert_eq!(before[0].capturing_side, Some(BattleSide::Attacker));

    place(&mut battle, "guard", outer);
    for _ in 0..3 {
        next_pulse(&mut battle);
        assert!(
            formed(find(&battle, "a0")) && formed(find(&battle, "guard")),
            "fixture: both sides stay formed while contesting"
        );
        assert_eq!(
            objectives(&battle),
            before,
            "contested capture changes no objective"
        );
    }
}

#[test]
fn only_the_final_objective_wins_a_started_siege() {
    let mut battle = staged_siege().start();
    let all = objectives(&battle);
    place(&mut battle, "a0", all[0].center);
    pulse_until(&mut battle, |battle| {
        captured_by_attacker(&objectives(battle)[0])
    });
    assert_eq!(
        battle.state(),
        TacticalBattleState::Running,
        "capturing an earlier objective never finishes the battle"
    );

    // Walk the remaining objectives one by one.
    for (index, objective) in all.iter().enumerate().skip(1) {
        place(&mut battle, &format!("a{index}"), objective.center);
        let mut finished_at = None;
        for _ in 0..PULSE_LIMIT {
            next_pulse(&mut battle);
            let now = objectives(&battle);
            if captured_by_attacker(&now[index]) {
                finished_at = Some(battle.tick());
                break;
            }
            assert_eq!(battle.state(), TacticalBattleState::Running);
        }
        let tick = finished_at.expect("objective captured within the liveness limit");
        if index + 1 < all.len() {
            assert_eq!(battle.state(), TacticalBattleState::Running);
        } else {
            assert_eq!(
                battle.state(),
                TacticalBattleState::Finished {
                    winner: Some(BattleSide::Attacker),
                    finishing_tick: tick,
                    reason: TacticalFinishReason::SiegeCapture,
                },
                "the final objective's capture finishes the battle on that pulse"
            );
        }
    }
    assert!(objectives(&battle).iter().all(captured_by_attacker));
}

#[test]
fn a_defender_holding_the_first_objective_does_not_finish_the_battle() {
    let mut battle = staged_siege().start();
    let outer = objectives(&battle)[0].center;
    place(&mut battle, "guard", outer);
    for _ in 0..20 {
        next_pulse(&mut battle);
    }
    assert_eq!(battle.state(), TacticalBattleState::Running);
    let last = objectives(&battle).last().cloned().unwrap();
    assert_eq!(
        last.captured_by, None,
        "the final objective is not captured"
    );
}

/// Full staged assault from a fresh siege; returns per-pulse objective states.
fn staged_timeline(
    mut battle: TacticalBattle,
    stop: Option<u64>,
) -> (TacticalBattle, Vec<(u64, Vec<SiegeObjectiveStatus>)>) {
    let mut timeline = Vec::new();
    while battle.state() == TacticalBattleState::Running {
        if stop.is_some_and(|tick| battle.tick() >= tick) {
            break;
        }
        assert!(battle.tick() < PULSE * u64::from(PULSE_LIMIT));
        next_pulse(&mut battle);
        timeline.push((battle.tick(), objectives(&battle)));
    }
    (battle, timeline)
}

fn staged_assault() -> TacticalBattle {
    let mut battle = staged_siege().start();
    for (index, objective) in objectives(&battle).into_iter().enumerate() {
        place(&mut battle, &format!("a{index}"), objective.center);
    }
    battle
}

#[test]
fn staged_capture_is_deterministic_and_resumes_exactly_from_a_mid_capture_save() {
    let (finished, timeline) = staged_timeline(staged_assault(), None);
    let (again, repeat) = staged_timeline(staged_assault(), None);
    assert_eq!(
        timeline, repeat,
        "identical inputs give identical timelines"
    );
    assert_eq!(finished, again);

    // Save after the first objective fell and the next is partly taken.
    let (save_tick, first) = timeline
        .iter()
        .find(|(_, states)| captured_by_attacker(&states[0]) && states[1].progress > 0)
        .cloned()
        .expect("fixture: a mid-capture moment exists");
    assert!(states_partial(&first[1]));
    let (paused, head) = staged_timeline(staged_assault(), Some(save_tick));
    assert_eq!(paused.tick(), save_tick);
    let document = serde_json::to_string(&paused).unwrap();
    let restored: TacticalBattle = serde_json::from_str(&document).unwrap();
    assert_eq!(restored, paused, "a mid-capture save round-trips exactly");
    assert_eq!(objectives(&restored), first);
    let (resumed, tail) = staged_timeline(restored, None);
    let mut joined = head;
    joined.extend(tail);
    assert_eq!(joined, timeline, "a restored siege continues identically");
    assert_eq!(resumed, finished);
}

fn states_partial(objective: &SiegeObjectiveStatus) -> bool {
    objective.progress < SIEGE_CAPTURE_MAX_PROGRESS && objective.captured_by.is_none()
}

fn single_point_fixture() -> Value {
    serde_json::from_str(include_str!("fixtures/siege-single-point-v1.json")).unwrap()
}

/// The pre-#127 sandbox capture scenario recorded in the fixture.
fn single_point_run(profile: SiegeProfile) -> (TacticalBattle, Option<Value>, Vec<Value>) {
    let mut battle = TacticalBattle::deploy_siege_with_profile(
        FIELD,
        vec![
            TacticalUnit::new(
                "a",
                BattleSide::Attacker,
                40,
                BattlePoint::new(30_000, 50_000),
                Formation::Line { files: 10 },
                1_000,
            ),
            TacticalUnit::new(
                "d",
                BattleSide::Defender,
                40,
                BattlePoint::new(100_000, 100_000),
                Formation::Line { files: 10 },
                1_000,
            ),
        ],
        BattlefieldLocation::MountainPass,
        profile,
    )
    .unwrap()
    .start();
    battle.open_siege_gate().unwrap();
    battle
        .issue_move_order(MovementOrder {
            unit_id: "a".into(),
            destination: BattlePoint::new(80_000, 50_000),
        })
        .unwrap();
    let mut timeline = Vec::new();
    let mut mid = None;
    while battle.state() == TacticalBattleState::Running && battle.tick() < 20_000 {
        battle.advance_ticks(1);
        if battle.tick().is_multiple_of(PULSE) {
            let capture = battle.siege_snapshot().unwrap().capture;
            timeline.push(serde_json::json!([
                battle.tick(),
                capture.progress,
                capture.capturing_side,
                capture.captured_by
            ]));
            if capture.progress == 500 && mid.is_none() {
                mid = Some(serde_json::to_value(&battle).unwrap());
            }
        }
    }
    (battle, mid, timeline)
}

#[test]
fn single_point_sieges_keep_their_recorded_timeline_and_document() {
    let fixture = single_point_fixture();
    for (name, profile) in [
        ("stoneWallsV1", SiegeProfile::StoneWallsV1),
        ("palisadeV1", SiegeProfile::PalisadeV1),
    ] {
        let recorded = &fixture[name];
        let (battle, mid, timeline) = single_point_run(profile);
        assert_eq!(
            Value::Array(timeline),
            recorded["captureTimeline"],
            "{name}: identical capture timeline"
        );
        let TacticalBattleState::Finished { finishing_tick, .. } = battle.state() else {
            panic!("{name}: the single-point siege finishes on capture");
        };
        assert_eq!(finishing_tick, recorded["finishingTick"].as_u64().unwrap());
        assert_eq!(
            mid.unwrap(),
            recorded["midCapture"],
            "{name}: identical mid-capture save document"
        );
        assert_eq!(
            serde_json::to_value(&battle).unwrap(),
            recorded["final"],
            "{name}: identical finished document"
        );
    }
}

#[test]
fn older_single_point_saves_load_unchanged_and_resume_identically() {
    let fixture = single_point_fixture();
    for (name, profile) in [
        ("stoneWallsV1", SiegeProfile::StoneWallsV1),
        ("palisadeV1", SiegeProfile::PalisadeV1),
    ] {
        let recorded = &fixture[name];
        let mut loaded: TacticalBattle =
            serde_json::from_value(recorded["midCapture"].clone()).unwrap();
        assert_eq!(
            serde_json::to_value(&loaded).unwrap(),
            recorded["midCapture"],
            "{name}: an older save re-serializes to the same document"
        );
        let siege = loaded.siege_snapshot().unwrap();
        assert_eq!(siege.profile, profile);
        assert_eq!(
            objectives(&loaded),
            vec![SiegeObjectiveStatus {
                center: siege.layout.capture_point.center,
                radius_mm: siege.layout.capture_point.radius_mm,
                progress: 500,
                capturing_side: Some(BattleSide::Attacker),
                captured_by: None,
                unlocked: true,
            }],
            "{name}: an older save is one partly captured objective"
        );
        while loaded.state() == TacticalBattleState::Running && loaded.tick() < 20_000 {
            loaded.advance_ticks(1);
        }
        assert_eq!(
            serde_json::to_value(&loaded).unwrap(),
            recorded["final"],
            "{name}: an older save resumes to the recorded finish"
        );
    }
}
