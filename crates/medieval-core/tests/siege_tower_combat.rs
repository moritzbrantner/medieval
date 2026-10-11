//! Acceptance for #124: staffed siege towers fire on attackers in range.
//!
//! Resolved design (inferred from existing core conventions, see the PR record):
//! - A tower is *staffed* (active) while at least one formed defender unit
//!   stands within its footprint circle (`SiegeTower::center`/`radius_mm`),
//!   exactly as the capture point counts formed units by position. Any formed
//!   defender staffs it; attackers never do.
//! - An active tower fires one bounded volley per authoritative combat pulse at
//!   the nearest formed attacker unit within its range of the tower center.
//!   Tower fire goes through the core ranged casualty rules (armor resistance,
//!   soldier cap, deterministic integer arithmetic). It never spends the
//!   staffing unit's ammunition and never hurts defenders.
//! - Tower activity (staffed + current target) is core state; presentation
//!   only projects it.
//!
//! Public seams these tests require (named here so the implementation stays
//! minimal):
//! - `SiegeProfile::tower_range_mm(self) -> u32`: reach of an active tower,
//!   measured from the tower center to the target unit position (inclusive),
//!   per fortification profile like `gate_damage_per_pulse`.
//! - `SiegeProfile::tower_max_casualties_per_pulse(self) -> u16`: upper bound
//!   on the soldiers one active tower removes in one combat pulse.
//! - `TacticalBattle::siege_tower_activity(&self) -> Option<Vec<SiegeTowerActivity>>`:
//!   `None` outside a siege, otherwise one entry per `layout.towers` entry in
//!   the same order. `SiegeTowerActivity` (re-exported from the crate root) has
//!   public fields `staffed: bool` and `target_unit_id: Option<String>` (the
//!   attacker unit the tower fires at; `None` while unstaffed or with no
//!   attacker in range).

use medieval_core::{
    BattlePoint, BattleSide, BattlefieldLocation, FlatBattlefield, Formation, MovementOrder,
    SiegeProfile, TACTICAL_TICKS_PER_SECOND, TacticalBattle, TacticalUnit, UnitCombatProfile,
    UnitKind,
};

/// Core `COMBAT_CONTACT_DISTANCE_MM` (documented melee reach; not re-exported).
const COMBAT_CONTACT_DISTANCE_MM: u32 = 1_500;
/// Liveness guard for reaching ordered positions, not a tuning budget.
const SETTLE_TICK_LIMIT: u32 = 4_000;
const PULSE: u32 = TACTICAL_TICKS_PER_SECOND;

/// 300 m wide, 200 m deep. Stone: wall x 147–153 m, tower radius 4 m;
/// palisade: wall x 148.5–151.5 m, tower radius 2.5 m. Towers stand on the
/// wall line (x 150 m) at y 30, 70, 130 and 170 m; the gate spans y 90–110 m.
const BATTLEFIELD: FlatBattlefield = FlatBattlefield::new(300_000, 200_000);
/// Deployment spots on the tower-0 row: attacker zone x <= 105 m, defender
/// zone x >= 195 m.
const ATTACKER_START_X: u32 = 100_000;
const DEFENDER_START_X: u32 = 200_000;

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

fn siege(profile: SiegeProfile, units: Vec<TacticalUnit>) -> TacticalBattle {
    TacticalBattle::deploy_siege_with_profile(
        BATTLEFIELD,
        units,
        BattlefieldLocation::ForestClearing,
        profile,
    )
    .unwrap()
}

fn find<'a>(battle: &'a TacticalBattle, id: &str) -> &'a TacticalUnit {
    battle.units().iter().find(|unit| unit.id() == id).unwrap()
}

fn soldiers(battle: &TacticalBattle, id: &str) -> u16 {
    find(battle, id).soldiers()
}

fn move_to(battle: &mut TacticalBattle, id: &str, destination: BattlePoint) {
    assert!(battle.siege_snapshot().unwrap().is_passable_at(destination));
    battle
        .issue_move_order(MovementOrder {
            unit_id: id.into(),
            destination,
        })
        .unwrap();
}

/// Advances until no unit still travels, then to the tick just after the next
/// combat pulse so activity reflects the settled positions.
fn settle(battle: &mut TacticalBattle) {
    for _ in 0..SETTLE_TICK_LIMIT {
        if battle
            .units()
            .iter()
            .all(|unit| unit.destination().is_none())
        {
            advance_to_next_pulse(battle);
            return;
        }
        battle.advance_ticks(1);
    }
    panic!("ordered units must reach their destinations within the liveness limit");
}

fn advance_to_next_pulse(battle: &mut TacticalBattle) {
    battle.advance_ticks(1);
    while !battle.tick().is_multiple_of(u64::from(PULSE)) {
        battle.advance_ticks(1);
    }
}

fn tower_zero(battle: &TacticalBattle) -> (BattlePoint, u32) {
    let tower = battle.siege_snapshot().unwrap().layout.towers[0];
    (tower.center, tower.radius_mm)
}

fn within(left: BattlePoint, right: BattlePoint, reach: u32) -> bool {
    let dx = u64::from(left.x_mm.abs_diff(right.x_mm));
    let dy = u64::from(left.y_mm.abs_diff(right.y_mm));
    let reach = u64::from(reach);
    dx * dx + dy * dy <= reach * reach
}

/// A defender spot inside tower 0's footprint on the defender side of the wall.
fn staffing_point(battle: &TacticalBattle) -> BattlePoint {
    let siege = battle.siege_snapshot().unwrap();
    let (center, radius) = tower_zero(battle);
    let wall_max_x = siege.layout.wall_segments[0].max_x_mm;
    let point = BattlePoint::new(
        wall_max_x + (center.x_mm + radius - wall_max_x) / 2,
        center.y_mm,
    );
    assert!(
        within(point, center, radius),
        "fixture: inside the footprint"
    );
    assert!(siege.is_passable_at(point), "fixture: outside the wall");
    point
}

/// An attacker spot `distance` in front of tower 0, outside the wall.
fn attacker_point(battle: &TacticalBattle, distance: u32) -> BattlePoint {
    let (center, _) = tower_zero(battle);
    let point = BattlePoint::new(center.x_mm - distance, center.y_mm);
    assert!(battle.siege_snapshot().unwrap().is_passable_at(point));
    point
}

/// A distance clearly inside tower range but outside the tower footprint.
fn in_range_distance(battle: &TacticalBattle, profile: SiegeProfile) -> u32 {
    let (_, radius) = tower_zero(battle);
    (radius + profile.tower_range_mm()) / 2
}

fn staffed(battle: &TacticalBattle) -> Vec<bool> {
    battle
        .siege_tower_activity()
        .unwrap()
        .iter()
        .map(|tower| tower.staffed)
        .collect()
}

fn target(battle: &TacticalBattle, tower: usize) -> Option<String> {
    battle.siege_tower_activity().unwrap()[tower]
        .target_unit_id
        .clone()
}

/// Tower 0 staffed by `staff`, attacker `near` placed `distance` in front of it.
fn staffed_tower(
    profile: SiegeProfile,
    staff: TacticalUnit,
    near: TacticalUnit,
    distance: impl Fn(&TacticalBattle) -> u32,
) -> TacticalBattle {
    let (staff_id, near_id) = (staff.id().to_owned(), near.id().to_owned());
    let mut battle = siege(profile, vec![near, staff]);
    let staffing = staffing_point(&battle);
    let attacker = attacker_point(&battle, distance(&battle));
    move_to(&mut battle, &staff_id, staffing);
    move_to(&mut battle, &near_id, attacker);
    settle(&mut battle);
    assert_eq!(find(&battle, &staff_id).position(), staffing);
    assert_eq!(find(&battle, &near_id).position(), attacker);
    battle
}

fn default_staffed_tower(profile: SiegeProfile) -> TacticalBattle {
    staffed_tower(
        profile,
        unit("staff", BattleSide::Defender, DEFENDER_START_X, 30_000),
        unit("near", BattleSide::Attacker, ATTACKER_START_X, 30_000),
        |battle| in_range_distance(battle, profile),
    )
}

#[test]
fn tower_combat_parameters_are_bounded_per_profile() {
    for profile in SiegeProfile::ALL {
        let battle = siege(
            profile,
            vec![unit("d", BattleSide::Defender, DEFENDER_START_X, 30_000)],
        );
        let (_, radius) = tower_zero(&battle);
        let range = profile.tower_range_mm();
        assert!(
            range >= radius + 2 * COMBAT_CONTACT_DISTANCE_MM,
            "{profile:?}: a tower's ranged effect reaches beyond melee distance of its footprint"
        );
        assert!(
            range < BATTLEFIELD.width_mm / 3,
            "{profile:?}: tower range is bounded, not battlefield-wide"
        );
        assert!(profile.tower_max_casualties_per_pulse() > 0);
    }
}

#[test]
fn siege_tower_activity_exists_only_in_sieges_and_starts_unstaffed() {
    let field = TacticalBattle::new(
        BATTLEFIELD,
        vec![
            unit("a", BattleSide::Attacker, 10_000, 10_000),
            unit("d", BattleSide::Defender, 290_000, 190_000),
        ],
    )
    .unwrap();
    assert!(field.siege_tower_activity().is_none());

    let battle = siege(
        SiegeProfile::StoneWallsV1,
        vec![unit("d", BattleSide::Defender, DEFENDER_START_X, 30_000)],
    );
    let activity = battle.siege_tower_activity().unwrap();
    assert_eq!(
        activity.len(),
        battle.siege_snapshot().unwrap().layout.towers.len()
    );
    assert!(
        activity
            .iter()
            .all(|tower| !tower.staffed && tower.target_unit_id.is_none())
    );
}

#[test]
fn an_unstaffed_tower_has_no_effect_on_attackers_in_range() {
    for profile in SiegeProfile::ALL {
        let mut battle = siege(
            profile,
            vec![
                unit("near", BattleSide::Attacker, ATTACKER_START_X, 30_000),
                unit(
                    "far-defender",
                    BattleSide::Defender,
                    DEFENDER_START_X,
                    190_000,
                ),
            ],
        );
        let attacker = attacker_point(&battle, in_range_distance(&battle, profile));
        move_to(&mut battle, "near", attacker);
        settle(&mut battle);
        battle.advance_ticks(10 * PULSE);
        assert_eq!(soldiers(&battle, "near"), 100, "{profile:?}");
        assert_eq!(staffed(&battle), vec![false; 4], "{profile:?}");
        assert_eq!(target(&battle, 0), None, "{profile:?}");
    }
}

#[test]
fn a_staffed_tower_fires_bounded_volleys_on_an_attacker_in_range() {
    for profile in SiegeProfile::ALL {
        let mut battle = default_staffed_tower(profile);
        assert_eq!(
            staffed(&battle),
            vec![true, false, false, false],
            "{profile:?}: a formed defender in the footprint staffs only that tower"
        );
        assert_eq!(target(&battle, 0).as_deref(), Some("near"), "{profile:?}");
        for tower in 1..4 {
            assert_eq!(target(&battle, tower), None, "{profile:?}");
        }

        let start = soldiers(&battle, "near");
        let cap = profile.tower_max_casualties_per_pulse();
        let mut previous = start;
        for _ in 0..(10 * PULSE) {
            battle.advance_ticks(1);
            let now = soldiers(&battle, "near");
            if now != previous {
                assert!(
                    battle.tick().is_multiple_of(u64::from(PULSE)),
                    "{profile:?}: tower fire resolves only on combat pulses"
                );
                assert!(
                    previous - now <= cap,
                    "{profile:?}: one pulse removed {} soldiers, cap {cap}",
                    previous - now
                );
            }
            previous = now;
        }
        assert!(
            soldiers(&battle, "near") < start,
            "{profile:?}: ten pulses of tower fire cause casualties"
        );
        assert_eq!(
            soldiers(&battle, "staff"),
            100,
            "{profile:?}: tower fire never hurts defenders"
        );
    }
}

#[test]
fn a_staffed_tower_ignores_attackers_out_of_range() {
    for profile in SiegeProfile::ALL {
        let mut battle = staffed_tower(
            profile,
            unit("staff", BattleSide::Defender, DEFENDER_START_X, 30_000),
            unit("distant", BattleSide::Attacker, ATTACKER_START_X, 30_000),
            |_| profile.tower_range_mm() + 1_000,
        );
        assert!(staffed(&battle)[0], "{profile:?}");
        battle.advance_ticks(10 * PULSE);
        assert_eq!(soldiers(&battle, "distant"), 100, "{profile:?}");
        assert_eq!(target(&battle, 0), None, "{profile:?}");
    }
}

#[test]
fn a_tower_fires_on_one_target_the_nearest_attacker_in_range() {
    let profile = SiegeProfile::StoneWallsV1;
    let mut battle = siege(
        profile,
        vec![
            unit("near", BattleSide::Attacker, ATTACKER_START_X, 30_000),
            unit("farther", BattleSide::Attacker, ATTACKER_START_X, 40_000),
            unit("staff", BattleSide::Defender, DEFENDER_START_X, 30_000),
        ],
    );
    let (_, radius) = tower_zero(&battle);
    let span = profile.tower_range_mm() - radius;
    let near = attacker_point(&battle, radius + span / 3);
    let farther = attacker_point(&battle, radius + 2 * span / 3);
    let staffing = staffing_point(&battle);
    move_to(&mut battle, "near", near);
    move_to(&mut battle, "farther", farther);
    move_to(&mut battle, "staff", staffing);
    settle(&mut battle);
    assert_eq!(target(&battle, 0).as_deref(), Some("near"));

    // Inspect the first pulse that removes soldiers: exactly the nearest
    // attacker loses them, and the tower reports it as its target.
    let mut before = (soldiers(&battle, "near"), soldiers(&battle, "farther"));
    for _ in 0..10 {
        advance_to_next_pulse(&mut battle);
        let after = (soldiers(&battle, "near"), soldiers(&battle, "farther"));
        if after != before {
            assert!(after.0 < before.0, "the nearest attacker is hit");
            assert_eq!(
                after.1, before.1,
                "one active tower fires at one target per pulse"
            );
            assert_eq!(target(&battle, 0).as_deref(), Some("near"));
            return;
        }
        before = after;
    }
    panic!("a staffed tower with attackers in range must cause casualties");
}

#[test]
fn attackers_in_the_tower_footprint_do_not_staff_it() {
    for profile in SiegeProfile::ALL {
        let mut battle = siege(
            profile,
            vec![
                unit("climber", BattleSide::Attacker, ATTACKER_START_X, 30_000),
                unit("defender", BattleSide::Defender, DEFENDER_START_X, 190_000),
            ],
        );
        let siege = battle.siege_snapshot().unwrap();
        let (center, radius) = tower_zero(&battle);
        let wall_min_x = siege.layout.wall_segments[0].min_x_mm;
        let footprint_edge = center.x_mm - radius;
        let point = BattlePoint::new(wall_min_x - (wall_min_x - footprint_edge) / 2, center.y_mm);
        assert!(within(point, center, radius));
        move_to(&mut battle, "climber", point);
        settle(&mut battle);
        battle.advance_ticks(5 * PULSE);
        assert_eq!(staffed(&battle), vec![false; 4], "{profile:?}");
        assert_eq!(soldiers(&battle, "climber"), 100, "{profile:?}");
        assert_eq!(soldiers(&battle, "defender"), 100, "{profile:?}");
    }
}

#[test]
fn tower_fire_does_not_spend_the_staffing_units_ammunition() {
    let profile = SiegeProfile::StoneWallsV1;
    let archers = unit("staff", BattleSide::Defender, DEFENDER_START_X, 30_000)
        .with_combat_stats(UnitCombatProfile::v1(UnitKind::Archers));
    let initial = archers.ammunition();
    assert!(initial.is_some());
    let mut battle = staffed_tower(
        profile,
        archers,
        unit("near", BattleSide::Attacker, ATTACKER_START_X, 30_000),
        |battle| in_range_distance(battle, profile),
    );
    let start = soldiers(&battle, "near");
    battle.advance_ticks(10 * PULSE);
    assert!(soldiers(&battle, "near") < start);
    assert_eq!(find(&battle, "staff").ammunition(), initial);
}

#[test]
fn tower_fire_respects_target_armor() {
    let profile = SiegeProfile::StoneWallsV1;
    let losses = |kind| {
        let target = TacticalUnit::new(
            "near",
            BattleSide::Attacker,
            500,
            BattlePoint::new(ATTACKER_START_X, 30_000),
            Formation::Line { files: 10 },
            1_000,
        )
        .with_combat_stats(UnitCombatProfile::v1(kind));
        let mut battle = staffed_tower(
            profile,
            unit("staff", BattleSide::Defender, DEFENDER_START_X, 30_000),
            target,
            |battle| in_range_distance(battle, profile),
        );
        let start = soldiers(&battle, "near");
        battle.advance_ticks(10 * PULSE);
        start - soldiers(&battle, "near")
    };
    let levy = losses(UnitKind::Levy);
    let knights = losses(UnitKind::Knights);
    assert!(levy > 0);
    assert!(
        knights < levy,
        "armor resists tower missiles like other ranged fire (knights {knights}, levy {levy})"
    );
}

#[test]
fn tower_combat_is_deterministic_and_replays_from_a_saved_battle() {
    let mut first = default_staffed_tower(SiegeProfile::StoneWallsV1);
    let mut second = first.clone();
    first.advance_ticks(20 * PULSE);
    second.advance_ticks(20 * PULSE);
    assert_eq!(first, second);
    assert_eq!(first.siege_tower_activity(), second.siege_tower_activity());

    // Save mid-battle after tower fire has caused casualties.
    let mut saved = default_staffed_tower(SiegeProfile::StoneWallsV1);
    let start = soldiers(&saved, "near");
    while soldiers(&saved, "near") == start {
        assert!(saved.tick() < 100 * u64::from(PULSE));
        saved.advance_ticks(1);
    }
    saved.advance_ticks(7);
    let encoded = serde_json::to_string(&saved).unwrap();
    let mut restored: TacticalBattle = serde_json::from_str(&encoded).unwrap();
    assert_eq!(restored, saved);
    assert_eq!(
        restored.siege_tower_activity(),
        saved.siege_tower_activity()
    );

    let mut continued = saved.clone();
    continued.advance_ticks(10 * PULSE);
    restored.advance_ticks(10 * PULSE);
    assert_eq!(restored, continued);
    assert_eq!(
        restored.siege_tower_activity(),
        continued.siege_tower_activity()
    );
}
