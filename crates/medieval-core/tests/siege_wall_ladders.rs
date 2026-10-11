//! Acceptance for #125: ladders, the one wall-access mechanic for assaulting
//! units.
//!
//! Resolved contract (inferred from existing core conventions; see the PR
//! record for the rationale):
//! - **Method:** scaling ladders. No new engine entity: a ladder is placed by
//!   an existing unit and then belongs to the siege. (`SiegeTower` already
//!   names the defenders' wall towers, and engines are #126.)
//! - **Availability:** every formed attacker *foot* unit carries one ladder.
//!   Mounted units (`Knights`) never place or climb ladders; defenders never
//!   do either. A unit that has placed its ladder cannot place another.
//! - **Placement order:** `issue_ladder_order(unit, at)` is legal only in a
//!   running siege, for an eligible formed attacker, when `at` lies inside a
//!   wall segment (never the gate, never open ground). Like the gate-attack
//!   order the unit approaches from any distance to the foot of the wall on
//!   its own side; the ladder exists only once the unit has arrived there.
//!   Rejected orders change nothing.
//! - **Geometry:** a placed ladder makes one bounded wall cell (`area`) span
//!   the full wall thickness around the ordered point, inside that wall
//!   segment and narrower than the gate. Only eligible attackers may stand in
//!   or move through that cell; every other wall position still blocks
//!   movement and melee exactly as #124 established, and a wall position is
//!   still never a legal move destination.
//! - **Pathing:** an eligible attacker's ordinary move order to the far side
//!   of a closed wall routes through a placed ladder.
//! - **Climbing takes time:** a climbing unit stands on the ladder (inside the
//!   wall footprint) for at least `SiegeProfile::ladder_climb_ticks()` ticks,
//!   longer than walking the same distance would take.
//! - **Contest:** melee is possible across the ladder (the line between the
//!   two units crosses the wall line inside the ladder cell), and a formed
//!   defender standing at the top of the ladder (within melee contact of the
//!   inner foot) *holds* it: no attacker leaves the ladder onto the defenders'
//!   side while it is held. A held ladder is not destroyed; it is released
//!   when no formed defender holds the top.
//!
//! Public seams these tests require (named here so the implementation stays
//! minimal):
//! - `TacticalBattle::issue_ladder_order(&mut self, unit_id: &str, at: BattlePoint)
//!   -> Result<(), TacticalError>`.
//! - `TacticalBattle::siege_ladders(&self) -> Option<Vec<SiegeLadder>>`: `None`
//!   outside a siege, otherwise the placed ladders in placement order. It is
//!   the presentation read model, like `siege_tower_activity`.
//! - `SiegeLadder` (re-exported from the crate root) with public fields
//!   `area: SiegeArea` (the traversable wall cell) and `held_by_defender: bool`
//!   (derived from authoritative unit state, like a staffed tower).
//! - `SiegeProfile::ladder_climb_ticks(self) -> u32`, per fortification
//!   profile like `gate_damage_per_pulse`.
//!
//! The tests deliberately leave open: how many units may climb one ladder at
//! once, the exact ladder width, opponent-AI use of ladders (#128), engines
//! (#126), and how the renderer draws ladders.

use medieval_core::{
    BattlePoint, BattleSide, BattlefieldLocation, FlatBattlefield, Formation, MovementOrder,
    SiegeProfile, TacticalBattle, TacticalError, TacticalGroundCover, TacticalUnit,
    UnitCombatProfile, UnitKind,
};

/// Core `COMBAT_CONTACT_DISTANCE_MM` (documented melee reach; not re-exported).
const COMBAT_CONTACT_DISTANCE_MM: u32 = 1_500;
/// Liveness guard for reaching ordered positions, not a tuning budget.
const TICK_LIMIT: u32 = 4_000;
/// 100 m square. Palisade: wall x 49.5–50.5 m, gate y 43–57 m, towers on the
/// wall line at y 15, 35, 65 and 85 m. Stone: wall x 49–51 m, gate y 45–55 m.
const BATTLEFIELD: FlatBattlefield = FlatBattlefield::new(100_000, 100_000);
/// The ladder row, on the northern wall segment between two towers.
const LADDER_Y: u32 = 25_000;
/// A destination on the defenders' side, off the ladder row.
const INSIDE: BattlePoint = BattlePoint::new(65_000, 40_000);

fn unit(id: &str, side: BattleSide, kind: UnitKind, x: u32, y: u32) -> TacticalUnit {
    TacticalUnit::new(
        id,
        side,
        100,
        BattlePoint::new(x, y),
        Formation::Line { files: 10 },
        1_000,
    )
    .with_combat_stats(UnitCombatProfile::v1(kind))
}

/// The ladder party: an eligible placer and climber, an ineligible mounted
/// attacker and one defender, all in their deployment zones.
fn assault(profile: SiegeProfile) -> TacticalBattle {
    TacticalBattle::deploy_siege_with_profile(
        BATTLEFIELD,
        vec![
            unit(
                "placer",
                BattleSide::Attacker,
                UnitKind::Spearmen,
                30_000,
                LADDER_Y,
            ),
            unit(
                "climber",
                BattleSide::Attacker,
                UnitKind::Levy,
                30_000,
                5_000,
            ),
            unit(
                "knights",
                BattleSide::Attacker,
                UnitKind::Knights,
                20_000,
                70_000,
            ),
            unit(
                "wall-guard",
                BattleSide::Defender,
                UnitKind::Spearmen,
                70_000,
                LADDER_Y,
            ),
        ],
        BattlefieldLocation::ForestClearing,
        profile,
    )
    .unwrap()
}

fn find<'a>(battle: &'a TacticalBattle, id: &str) -> &'a TacticalUnit {
    battle.units().iter().find(|unit| unit.id() == id).unwrap()
}

fn position(battle: &TacticalBattle, id: &str) -> BattlePoint {
    find(battle, id).position()
}

/// (min_x, max_x, min_y, max_y) of the wall segment holding the ladder row.
fn wall(battle: &TacticalBattle) -> (u32, u32, u32, u32) {
    let segment = battle.siege_snapshot().unwrap().layout.wall_segments[0];
    assert!(segment.contains(BattlePoint::new(segment.min_x_mm, LADDER_Y)));
    (
        segment.min_x_mm,
        segment.max_x_mm,
        segment.min_y_mm,
        segment.max_y_mm,
    )
}

fn in_wall_band(battle: &TacticalBattle, point: BattlePoint) -> bool {
    let (min_x, max_x, ..) = wall(battle);
    (min_x..=max_x).contains(&point.x_mm)
}

fn ladder_point(battle: &TacticalBattle) -> BattlePoint {
    let (min_x, max_x, ..) = wall(battle);
    BattlePoint::new(min_x + (max_x - min_x) / 2, LADDER_Y)
}

fn outer_foot(battle: &TacticalBattle) -> BattlePoint {
    BattlePoint::new(wall(battle).0 - 1, LADDER_Y)
}

fn inner_foot(battle: &TacticalBattle) -> BattlePoint {
    BattlePoint::new(wall(battle).1 + 1, LADDER_Y)
}

fn within(left: BattlePoint, right: BattlePoint, reach: u32) -> bool {
    let dx = u64::from(left.x_mm.abs_diff(right.x_mm));
    let dy = u64::from(left.y_mm.abs_diff(right.y_mm));
    let reach = u64::from(reach);
    dx * dx + dy * dy <= reach * reach
}

fn ladder_count(battle: &TacticalBattle) -> usize {
    battle.siege_ladders().unwrap().len()
}

fn held(battle: &TacticalBattle) -> bool {
    battle.siege_ladders().unwrap()[0].held_by_defender
}

fn move_to(battle: &mut TacticalBattle, id: &str, destination: BattlePoint) {
    battle
        .issue_move_order(MovementOrder {
            unit_id: id.into(),
            destination,
        })
        .unwrap();
}

fn settle(battle: &mut TacticalBattle) {
    for _ in 0..TICK_LIMIT {
        if battle
            .units()
            .iter()
            .all(|unit| unit.destination().is_none())
        {
            return;
        }
        battle.advance_ticks(1);
    }
    panic!("ordered units must reach their destinations within the liveness limit");
}

/// Orders `placer` to put a ladder at the ladder row and advances until it
/// exists. Returns the ticks it took.
fn place_ladder(battle: &mut TacticalBattle) -> u32 {
    let at = ladder_point(battle);
    assert_eq!(battle.issue_ladder_order("placer", at), Ok(()));
    for elapsed in 1..=TICK_LIMIT {
        battle.advance_ticks(1);
        assert!(
            !in_wall_band(battle, position(battle, "placer")),
            "placing a ladder does not climb it"
        );
        if ladder_count(battle) == 1 {
            return elapsed;
        }
    }
    panic!("an ordered ladder must be placed within the liveness limit");
}

/// Advances until `id` arrives at `destination`, returning the per-tick
/// positions of every unit.
fn advance_until_arrived(
    battle: &mut TacticalBattle,
    id: &str,
    destination: BattlePoint,
) -> Vec<Vec<BattlePoint>> {
    let mut timeline = Vec::new();
    for _ in 0..TICK_LIMIT {
        battle.advance_ticks(1);
        timeline.push(battle.units().iter().map(TacticalUnit::position).collect());
        if position(battle, id) == destination && find(battle, id).destination().is_none() {
            return timeline;
        }
    }
    panic!("{id} must reach {destination:?} within the liveness limit");
}

#[test]
fn ladders_exist_only_in_sieges_and_climbing_takes_time_in_every_profile() {
    let field = TacticalBattle::new(
        BATTLEFIELD,
        vec![
            unit(
                "a",
                BattleSide::Attacker,
                UnitKind::Spearmen,
                10_000,
                10_000,
            ),
            unit(
                "d",
                BattleSide::Defender,
                UnitKind::Spearmen,
                90_000,
                90_000,
            ),
        ],
    )
    .unwrap();
    assert!(field.siege_ladders().is_none());
    for profile in SiegeProfile::ALL {
        assert!(profile.ladder_climb_ticks() > 0);
        assert_eq!(
            assault(profile).siege_ladders(),
            Some(Vec::new()),
            "a fresh siege has no ladders"
        );
    }
}

#[test]
fn an_eligible_attacker_places_a_bounded_ladder_at_the_ordered_wall_point() {
    for profile in SiegeProfile::ALL {
        let mut battle = assault(profile);
        let at = ladder_point(&battle);
        battle.issue_ladder_order("placer", at).unwrap();
        assert_eq!(
            ladder_count(&battle),
            0,
            "the ladder exists only once the unit reaches the wall"
        );
        battle.advance_ticks(1);
        assert_eq!(ladder_count(&battle), 0);
        let mut battle = assault(profile);
        place_ladder(&mut battle);

        let siege = battle.siege_snapshot().unwrap();
        let gate = siege.layout.gate;
        let (min_x, max_x, min_y, max_y) = wall(&battle);
        let before = battle.clone();
        let ladders = battle.siege_ladders().unwrap();
        assert_eq!(battle, before, "reading ladders never mutates the battle");
        let ladder = &ladders[0];
        assert!(ladder.area.contains(at), "the ladder stands where ordered");
        assert_eq!(
            (ladder.area.min_x_mm, ladder.area.max_x_mm),
            (min_x, max_x),
            "a ladder spans the full wall thickness"
        );
        assert!(
            ladder.area.min_y_mm >= min_y && ladder.area.max_y_mm <= max_y,
            "a ladder is bounded to its wall segment and never reaches the gate"
        );
        assert!(
            ladder.area.max_y_mm - ladder.area.min_y_mm < gate.max_y_mm - gate.min_y_mm,
            "a ladder is a narrower access point than the gate"
        );
        assert!(!ladder.held_by_defender);

        let placer = position(&battle, "placer");
        assert!(placer.x_mm < min_x, "the placer stays on its own side");
        assert!(within(
            placer,
            outer_foot(&battle),
            COMBAT_CONTACT_DISTANCE_MM
        ));
        assert!(
            !siege.gate_state.is_traversable(),
            "placing a ladder changes no gate truth"
        );
    }
}

#[test]
fn illegal_ladder_orders_are_rejected_without_changing_the_battle() {
    let mut battle = assault(SiegeProfile::PalisadeV1);
    let at = ladder_point(&battle);
    let gate = battle.siege_snapshot().unwrap().layout.gate;
    let reject = |battle: &mut TacticalBattle, id: &str, at: BattlePoint| {
        let before = battle.clone();
        let result = battle.issue_ladder_order(id, at);
        assert!(result.is_err(), "{id} at {at:?} must be rejected");
        assert_eq!(*battle, before, "a rejected ladder order changes nothing");
        result.unwrap_err()
    };

    // Defenders and mounted units never use ladders.
    reject(&mut battle, "wall-guard", at);
    reject(&mut battle, "knights", at);
    // Only a wall segment takes a ladder: not the gate, not open ground on
    // either side, not outside the battlefield.
    reject(&mut battle, "placer", gate.center());
    reject(&mut battle, "placer", BattlePoint::new(40_000, LADDER_Y));
    reject(&mut battle, "placer", BattlePoint::new(60_000, LADDER_Y));
    reject(&mut battle, "placer", BattlePoint::new(200_000, LADDER_Y));
    assert_eq!(
        reject(&mut battle, "missing", at),
        TacticalError::UnitNotFound("missing".into())
    );

    // A routed unit cannot receive orders.
    let mut document = serde_json::to_value(&battle).unwrap();
    for unit in document["units"].as_array_mut().unwrap() {
        if unit["id"] == "placer" {
            unit["state"] = serde_json::json!("routed");
        }
    }
    let mut routed: TacticalBattle = serde_json::from_value(document).unwrap();
    assert_eq!(
        reject(&mut routed, "placer", at),
        TacticalError::UnitCannotReceiveOrders {
            unit_id: "placer".into()
        }
    );

    // A field battle has no wall.
    let mut field = TacticalBattle::new(
        BATTLEFIELD,
        vec![
            unit(
                "a",
                BattleSide::Attacker,
                UnitKind::Spearmen,
                10_000,
                10_000,
            ),
            unit(
                "d",
                BattleSide::Defender,
                UnitKind::Spearmen,
                90_000,
                90_000,
            ),
        ],
    )
    .unwrap();
    assert_eq!(reject(&mut field, "a", at), TacticalError::NotSiegeBattle);

    // A finished battle rejects every gameplay command.
    let mut finished = TacticalBattle::deploy_siege_with_profile(
        BATTLEFIELD,
        vec![unit(
            "placer",
            BattleSide::Attacker,
            UnitKind::Spearmen,
            30_000,
            LADDER_Y,
        )],
        BattlefieldLocation::ForestClearing,
        SiegeProfile::PalisadeV1,
    )
    .unwrap()
    .start();
    assert_eq!(
        reject(&mut finished, "placer", at),
        TacticalError::BattleFinished
    );

    // Each unit carries one ladder.
    place_ladder(&mut battle);
    let other_segment = battle.siege_snapshot().unwrap().layout.wall_segments[1].center();
    reject(&mut battle, "placer", other_segment);
    reject(&mut battle, "placer", at);
    assert_eq!(ladder_count(&battle), 1);
}

#[test]
fn eligible_attackers_cross_a_closed_wall_only_through_the_placed_ladder() {
    for profile in SiegeProfile::ALL {
        let mut battle = assault(profile);
        place_ladder(&mut battle);
        let area = battle.siege_ladders().unwrap()[0].area;
        assert!(
            battle.siege_snapshot().unwrap().is_passable_at(INSIDE),
            "fixture: the destination is open ground inside the wall"
        );

        // The climber did not place the ladder; any eligible attacker uses it.
        move_to(&mut battle, "climber", INSIDE);
        for positions in advance_until_arrived(&mut battle, "climber", INSIDE) {
            for point in positions {
                if in_wall_band(&battle, point) {
                    assert!(
                        area.contains(point),
                        "units stand inside the wall only on the ladder, not at {point:?}"
                    );
                }
            }
        }
        assert!(
            !battle.siege_snapshot().unwrap().gate_state.is_traversable(),
            "the climber crossed while the gate stayed closed"
        );

        // Wall positions, including the ladder cell, are still never
        // destinations.
        let before = battle.clone();
        let ladder = ladder_point(&battle);
        assert_eq!(
            battle.issue_move_order(MovementOrder {
                unit_id: "placer".into(),
                destination: ladder,
            }),
            Err(TacticalError::DestinationImpassable {
                unit_id: "placer".into(),
                destination: ladder,
            })
        );
        assert_eq!(battle, before);
    }
}

#[test]
fn climbing_a_ladder_takes_longer_than_walking_the_wall_thickness() {
    for profile in SiegeProfile::ALL {
        let mut battle = assault(profile);
        place_ladder(&mut battle);
        let (min_x, max_x, ..) = wall(&battle);
        assert_eq!(
            battle
                .terrain()
                .ground_cover_at(BATTLEFIELD, ladder_point(&battle)),
            TacticalGroundCover::Open,
            "fixture: no forest slows the wall crossing"
        );
        let speed = find(&battle, "climber").speed_mm_per_tick();
        let walking_ticks = (max_x - min_x) / speed + 1;

        move_to(&mut battle, "climber", INSIDE);
        let on_ladder = advance_until_arrived(&mut battle, "climber", INSIDE)
            .iter()
            .filter(|positions| {
                let climber = battle
                    .units()
                    .iter()
                    .position(|unit| unit.id() == "climber")
                    .unwrap();
                in_wall_band(&battle, positions[climber])
            })
            .count();
        let on_ladder = u32::try_from(on_ladder).unwrap();
        assert!(
            on_ladder >= profile.ladder_climb_ticks(),
            "{profile:?}: climbed in {on_ladder} ticks"
        );
        assert!(
            on_ladder > walking_ticks,
            "{profile:?}: climbing ({on_ladder} ticks) is slower than walking ({walking_ticks})"
        );
    }
}

#[test]
fn ladders_do_not_open_the_wall_for_mounted_units_or_defenders() {
    let mut battle = assault(SiegeProfile::PalisadeV1);
    place_ladder(&mut battle);
    let (min_x, max_x, ..) = wall(&battle);
    // Both orders are legal; neither unit may use the attackers' ladder.
    move_to(&mut battle, "knights", BattlePoint::new(65_000, LADDER_Y));
    move_to(
        &mut battle,
        "wall-guard",
        BattlePoint::new(35_000, LADDER_Y),
    );
    for _ in 0..1_000 {
        battle.advance_ticks(1);
        assert!(
            position(&battle, "knights").x_mm < min_x,
            "mounted units cannot climb ladders"
        );
        assert!(
            position(&battle, "wall-guard").x_mm > max_x,
            "defenders cannot use the attackers' ladder"
        );
    }
}

#[test]
fn a_defender_holding_the_ladder_top_blocks_the_climb_until_it_leaves() {
    let mut battle = assault(SiegeProfile::PalisadeV1);
    place_ladder(&mut battle);
    let (_, max_x, ..) = wall(&battle);
    let top = inner_foot(&battle);
    move_to(&mut battle, "wall-guard", top);
    settle(&mut battle);
    assert!(
        held(&battle),
        "a formed defender at the top holds the ladder"
    );

    let destination = BattlePoint::new(65_000, LADDER_Y);
    move_to(&mut battle, "climber", destination);
    for _ in 0..1_000 {
        battle.advance_ticks(1);
        assert!(held(&battle));
        assert!(
            position(&battle, "climber").x_mm <= max_x,
            "no attacker leaves a held ladder onto the defenders' side"
        );
    }

    move_to(&mut battle, "wall-guard", BattlePoint::new(90_000, 90_000));
    advance_until_arrived(&mut battle, "climber", destination);
    assert!(
        !held(&battle),
        "the ladder is released once the top is clear"
    );
    assert_eq!(ladder_count(&battle), 1, "a released ladder still stands");
}

#[test]
fn defenders_and_attackers_fight_in_melee_across_the_ladder() {
    // Palisade: the feet of the ladder are within melee contact distance.
    let mut battle = assault(SiegeProfile::PalisadeV1);
    place_ladder(&mut battle);
    let (outer, inner) = (outer_foot(&battle), inner_foot(&battle));
    assert!(within(outer, inner, COMBAT_CONTACT_DISTANCE_MM));
    move_to(&mut battle, "placer", outer);
    move_to(&mut battle, "wall-guard", inner);
    settle(&mut battle);

    battle
        .issue_engagement_order("placer", "wall-guard")
        .unwrap();
    battle
        .issue_engagement_order("wall-guard", "placer")
        .unwrap();
    // Ten combat pulses.
    for _ in 0..200 {
        battle.advance_ticks(1);
        assert!(position(&battle, "wall-guard").x_mm > wall(&battle).1);
    }
    assert!(
        find(&battle, "placer").soldiers() < 100,
        "a defender at the top contests the ladder in melee"
    );
    assert!(
        find(&battle, "wall-guard").soldiers() < 100,
        "attackers at the ladder fight the defender holding it"
    );
}

#[test]
fn the_wall_away_from_the_ladder_still_separates_melee() {
    // Palisade: units pressed against the wall 15 m from the ladder are within
    // melee contact distance but on opposite sides of an unladdered wall. The
    // row lies between tower 1 (y 35 m) and the gate, so the defender's siege
    // path toward the gate never staffs a tower.
    let mut battle = assault(SiegeProfile::PalisadeV1);
    place_ladder(&mut battle);
    let (min_x, max_x, ..) = wall(&battle);
    let row = 40_000;
    move_to(&mut battle, "climber", BattlePoint::new(min_x - 1, row));
    move_to(&mut battle, "wall-guard", BattlePoint::new(max_x + 1, row));
    settle(&mut battle);
    assert!(
        !battle.siege_ladders().unwrap()[0]
            .area
            .contains(BattlePoint::new(min_x, row)),
        "fixture: the row is off the ladder"
    );

    battle
        .issue_engagement_order("wall-guard", "climber")
        .unwrap();
    for _ in 0..200 {
        battle.advance_ticks(1);
        assert!(position(&battle, "wall-guard").x_mm > max_x);
    }
    assert_eq!(
        find(&battle, "climber").soldiers(),
        100,
        "a ladder opens only its own wall cell"
    );
}

#[test]
fn ladder_climbs_are_deterministic_and_continue_identically_after_save_and_load() {
    let mut first = assault(SiegeProfile::StoneWallsV1);
    place_ladder(&mut first);
    move_to(&mut first, "climber", INSIDE);
    let mut second = first.clone();
    let first_timeline = advance_until_arrived(&mut first, "climber", INSIDE);
    let second_timeline = advance_until_arrived(&mut second, "climber", INSIDE);
    assert_eq!(first_timeline, second_timeline);
    assert_eq!(first, second);

    // Save mid-climb, while the climber stands on the ladder.
    let mut saved = assault(SiegeProfile::StoneWallsV1);
    place_ladder(&mut saved);
    move_to(&mut saved, "climber", INSIDE);
    let mut ticks = 0;
    while !in_wall_band(&saved, position(&saved, "climber")) {
        ticks += 1;
        assert!(ticks < TICK_LIMIT, "the climber must reach the ladder");
        saved.advance_ticks(1);
    }
    let encoded = serde_json::to_string(&saved).unwrap();
    let mut restored: TacticalBattle = serde_json::from_str(&encoded).unwrap();
    assert_eq!(restored, saved);
    assert_eq!(restored.siege_ladders(), saved.siege_ladders());
    let restored_timeline = advance_until_arrived(&mut restored, "climber", INSIDE);
    let continued_timeline = advance_until_arrived(&mut saved, "climber", INSIDE);
    assert_eq!(restored_timeline, continued_timeline);
    assert_eq!(restored, saved);
    assert_eq!(
        restored, first,
        "saving mid-climb never changes the outcome"
    );
}

#[test]
fn sieges_without_ladders_keep_their_document_shape_and_older_saves_load() {
    let mut battle = TacticalBattle::deploy_siege_with_profile(
        BATTLEFIELD,
        vec![
            unit(
                "a",
                BattleSide::Attacker,
                UnitKind::Spearmen,
                30_000,
                LADDER_Y,
            ),
            TacticalUnit::new(
                "d",
                BattleSide::Defender,
                100,
                BattlePoint::new(70_000, LADDER_Y),
                Formation::Line { files: 10 },
                1_000,
            ),
        ],
        BattlefieldLocation::ForestClearing,
        SiegeProfile::PalisadeV1,
    )
    .unwrap();
    battle.advance_ticks(5);
    let document = serde_json::to_value(&battle).unwrap();
    let keys = |value: &serde_json::Value| -> Vec<String> {
        value.as_object().unwrap().keys().cloned().collect()
    };
    // The exact shape recorded before ladders existed (baseline 3c5ec13).
    assert_eq!(
        keys(&document),
        ["battlefield", "siege", "state", "terrain", "tick", "units"]
    );
    assert_eq!(
        keys(&document["siege"]),
        ["capture", "gateIntegrity", "gateState", "layout", "profile"]
    );
    assert_eq!(
        keys(&document["siege"]["layout"]),
        [
            "capturePoint",
            "deploymentZones",
            "gate",
            "towers",
            "wallSegments"
        ]
    );
    let base_unit = [
        "attackRangeMm",
        "destination",
        "engagementTarget",
        "fatigue",
        "formation",
        "id",
        "morale",
        "position",
        "pursuitCasualties",
        "side",
        "soldiers",
        "speedMmPerTick",
        "state",
    ];
    let mut profiled_unit = base_unit.to_vec();
    profiled_unit.extend(["combatProfile", "facing", "unitKind"]);
    profiled_unit.sort_unstable();
    assert_eq!(keys(&document["units"][0]), profiled_unit);
    assert_eq!(keys(&document["units"][1]), base_unit);

    // Such a document (a save from before ladders) loads without ladders and
    // round-trips unchanged.
    let legacy: TacticalBattle = serde_json::from_value(document.clone()).unwrap();
    assert_eq!(legacy, battle);
    assert_eq!(legacy.siege_ladders(), Some(Vec::new()));
    assert_eq!(serde_json::to_value(&legacy).unwrap(), document);
}
