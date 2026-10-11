//! Acceptance for #124: walls are not attackable or traversable positions.
//!
//! No wall-access mechanic exists yet (#125), so a wall segment separates the
//! two sides completely for movement and melee: a move order into a wall is
//! rejected, an engaged unit never stands inside a wall, and formed units on
//! opposite sides of a wall do not trade melee blows through it even when the
//! wall is thinner than melee contact distance (the palisade). Missiles
//! still fly over walls, and a breached gate still lets the sides meet.
//!
//! These tests use only existing public APIs; no new seam is needed.

use medieval_core::{
    BattlePoint, BattleSide, BattlefieldLocation, FlatBattlefield, Formation, MovementOrder,
    SiegeProfile, TacticalBattle, TacticalError, TacticalUnit, UnitCombatProfile, UnitKind,
};

/// Core `COMBAT_CONTACT_DISTANCE_MM` (documented melee reach; not re-exported).
const COMBAT_CONTACT_DISTANCE_MM: u32 = 1_500;

/// Liveness guard for reaching ordered positions, not a tuning budget.
const SETTLE_TICK_LIMIT: u32 = 2_000;

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

fn palisade(units: Vec<TacticalUnit>) -> TacticalBattle {
    TacticalBattle::deploy_siege_with_profile(
        FlatBattlefield::new(100_000, 100_000),
        units,
        BattlefieldLocation::ForestClearing,
        SiegeProfile::PalisadeV1,
    )
    .unwrap()
}

fn find<'a>(battle: &'a TacticalBattle, id: &str) -> &'a TacticalUnit {
    battle.units().iter().find(|unit| unit.id() == id).unwrap()
}

fn move_to(battle: &mut TacticalBattle, id: &str, destination: BattlePoint) {
    battle
        .issue_move_order(MovementOrder {
            unit_id: id.into(),
            destination,
        })
        .unwrap();
}

/// Advances until no unit still travels to an ordered destination.
fn settle(battle: &mut TacticalBattle) {
    for _ in 0..SETTLE_TICK_LIMIT {
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

fn distance_squared(left: BattlePoint, right: BattlePoint) -> u64 {
    let dx = u64::from(left.x_mm.abs_diff(right.x_mm));
    let dy = u64::from(left.y_mm.abs_diff(right.y_mm));
    dx * dx + dy * dy
}

fn assert_every_unit_outside_walls(battle: &TacticalBattle) {
    let siege = battle.siege_snapshot().unwrap();
    for unit in battle.units() {
        assert!(
            !siege.layout.wall_contains(unit.position()),
            "{} stands inside a wall at {:?}",
            unit.id(),
            unit.position()
        );
        assert!(siege.is_passable_at(unit.position()));
    }
}

#[test]
fn move_orders_into_wall_positions_are_rejected_whatever_the_gate_state() {
    let battlefield = FlatBattlefield::new(100_000, 100_000);
    let base = TacticalBattle::deploy_siege(
        battlefield,
        vec![
            unit("attacker", BattleSide::Attacker, 30_000, 20_000),
            unit("defender", BattleSide::Defender, 70_000, 20_000),
        ],
    )
    .unwrap();
    let siege = base.siege_snapshot().unwrap();
    let wall_points = siege.layout.wall_segments.map(|segment| segment.center());

    let mut open = base.clone();
    open.open_siege_gate().unwrap();
    let mut destroyed = base.clone();
    destroyed.destroy_siege_gate().unwrap();

    for mut battle in [base, open, destroyed] {
        for id in ["attacker", "defender"] {
            for destination in wall_points {
                let before = battle.clone();
                assert_eq!(
                    battle.issue_move_order(MovementOrder {
                        unit_id: id.into(),
                        destination,
                    }),
                    Err(TacticalError::DestinationImpassable {
                        unit_id: id.into(),
                        destination,
                    }),
                    "a wall position is never a legal destination"
                );
                assert_eq!(battle, before);
            }
        }
    }
}

#[test]
fn an_engagement_across_a_closed_wall_never_routes_a_unit_into_the_wall() {
    let mut battle = TacticalBattle::deploy_siege(
        FlatBattlefield::new(100_000, 100_000),
        vec![
            unit("attacker", BattleSide::Attacker, 30_000, 20_000),
            unit("defender", BattleSide::Defender, 70_000, 20_000),
        ],
    )
    .unwrap();
    battle
        .issue_engagement_order("attacker", "defender")
        .unwrap();
    let wall_min_x = battle.siege_snapshot().unwrap().layout.wall_segments[0].min_x_mm;
    for _ in 0..400 {
        battle.advance_ticks(1);
        assert_every_unit_outside_walls(&battle);
        assert!(
            find(&battle, "attacker").position().x_mm < wall_min_x,
            "the attacker cannot cross the wall line while the gate is closed"
        );
    }
}

#[test]
fn formed_units_cannot_fight_in_melee_through_a_wall() {
    // Palisade on a 100 m square: wall x 49.5–50.5 m, gate y 43–57 m. Both
    // units stand against the wall at y 25 m, away from the gate and towers,
    // closer together than melee contact distance but on opposite sides.
    let mut battle = palisade(vec![
        unit("assault", BattleSide::Attacker, 30_000, 25_000),
        unit("wall-guard", BattleSide::Defender, 70_000, 25_000),
    ]);
    let wall = battle.siege_snapshot().unwrap().layout.wall_segments[0];
    let outside = BattlePoint::new(wall.min_x_mm - 1, 25_000);
    let inside = BattlePoint::new(wall.max_x_mm + 1, 25_000);
    let reach = u64::from(COMBAT_CONTACT_DISTANCE_MM);
    assert!(
        distance_squared(outside, inside) <= reach * reach,
        "fixture: the units are within melee contact distance across the wall"
    );
    move_to(&mut battle, "assault", outside);
    move_to(&mut battle, "wall-guard", inside);
    settle(&mut battle);
    assert_eq!(find(&battle, "assault").position(), outside);
    assert_eq!(find(&battle, "wall-guard").position(), inside);

    battle
        .issue_engagement_order("assault", "wall-guard")
        .unwrap();
    // Ten combat pulses.
    for _ in 0..200 {
        battle.advance_ticks(1);
        assert_every_unit_outside_walls(&battle);
    }
    assert_eq!(
        find(&battle, "assault").soldiers(),
        100,
        "the wall blocks melee in both directions"
    );
    assert_eq!(
        find(&battle, "wall-guard").soldiers(),
        100,
        "a unit cannot attack an enemy through a wall position"
    );
}

#[test]
fn a_breached_gate_still_lets_the_sides_meet_in_melee() {
    let mut battle = palisade(vec![
        unit("assault", BattleSide::Attacker, 30_000, 50_000),
        unit("gate-guard", BattleSide::Defender, 66_000, 50_000),
    ]);
    battle.destroy_siege_gate().unwrap();
    battle
        .issue_engagement_order("assault", "gate-guard")
        .unwrap();
    for _ in 0..1_200 {
        battle.advance_ticks(1);
        assert_every_unit_outside_walls(&battle);
    }
    assert!(
        find(&battle, "gate-guard").soldiers() < 100,
        "walls block melee, the breached gate does not"
    );
}

#[test]
fn missiles_still_fly_over_a_wall() {
    // Archers outside the wall engage a defender inside it within missile range.
    let archers = unit("archers", BattleSide::Attacker, 30_000, 25_000)
        .with_combat_stats(UnitCombatProfile::v1(UnitKind::Archers));
    let range = archers.attack_range_mm();
    let mut battle = palisade(vec![
        archers,
        unit("wall-guard", BattleSide::Defender, 70_000, 25_000),
    ]);
    let wall = battle.siege_snapshot().unwrap().layout.wall_segments[0];
    let target = BattlePoint::new(wall.max_x_mm + 2_000, 25_000);
    let shooter = BattlePoint::new(target.x_mm - range / 2, 25_000);
    assert!(shooter.x_mm < wall.min_x_mm);
    move_to(&mut battle, "archers", shooter);
    move_to(&mut battle, "wall-guard", target);
    settle(&mut battle);

    battle
        .issue_engagement_order("archers", "wall-guard")
        .unwrap();
    battle.advance_ticks(200);
    assert!(
        find(&battle, "wall-guard").soldiers() < 100,
        "walls block traversal and melee, not missiles"
    );
    assert!(find(&battle, "archers").position().x_mm < wall.min_x_mm);
}
