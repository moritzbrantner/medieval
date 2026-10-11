//! Acceptance for #126: a battering ram and one ranged siege engine (a
//! catapult), made available only by explicit pre-battle siege setup.
//!
//! Resolved contract (inferred from the #123–#125 conventions; see the PR
//! record for the rationale):
//! - **Crewed equipment, not free entities.** Like the #125 ladder, an engine
//!   belongs to one existing attacker unit, its *crew*. The engine is where its
//!   crew is; it moves only with the crew's ordinary movement and is used only
//!   through orders issued to the crew. Defenders and mounted `Knights` never
//!   crew an engine, and one unit crews at most one engine.
//! - **Availability is explicit.** Only
//!   `TacticalBattle::deploy_siege_with_engines` assigns engines. Every other
//!   construction path (sandbox `deploy_siege*`, campaign seeds) has none, and
//!   engine orders for a unit without the matching engine are rejected with
//!   `TacticalError::SiegeEngineUnavailable(unit_id)` without changing the
//!   battle.
//! - **Ram.** `issue_ram_order(unit)` is the gate-attack order for a ram crew:
//!   legal only for the crew of a ram while the gate is closed; the crew
//!   approaches from any distance and, on every combat pulse within
//!   `SIEGE_GATE_ATTACK_DISTANCE`, deals `SiegeProfile::ram_damage_per_pulse`
//!   (instead of the unit gate damage), which is larger than
//!   `SiegeProfile::gate_damage_per_pulse`. The gate is destroyed on the pulse
//!   integrity reaches zero and the order then ends. A crew cannot place a
//!   ladder (an engine does not climb walls).
//! - **Catapult.** `issue_siege_engine_fire_order(unit, target)` holds the crew
//!   in place and fires at a `SiegeEngineTarget::Gate` (closed gate only, aimed
//!   at the gate center) or a `SiegeEngineTarget::Unit` (formed enemy only).
//!   The target must lie within `CATAPULT_MIN_RANGE_MM..=CATAPULT_MAX_RANGE_MM`
//!   of the crew position (Euclidean, inclusive); walls do not block the
//!   indirect shot. A loaded engine fires on the first combat pulse after the
//!   order, then needs exactly `CATAPULT_RELOAD_PULSES` pulses before the next
//!   shot; each shot spends one of `CATAPULT_AMMUNITION` stones and an empty
//!   catapult neither fires nor accepts fire orders. A gate hit removes exactly
//!   `SiegeProfile::catapult_gate_damage_per_shot` integrity; a unit hit flows
//!   through the normal casualty/morale path. A move order, rout or
//!   destruction of the crew ends firing.
//! - **Persistence.** Engine state (ammunition, reload, last shot) serializes
//!   with the battle; a battle without engines keeps the exact pre-engine
//!   document, so older saves load unchanged.
//!
//! Public seams these tests require (minimal, named here):
//! - `SiegeEngineKind { BatteringRam, Catapult }`, `SiegeEngineSetup { kind,
//!   crew_unit_id: String }`, `SiegeEngineTarget { Gate, Unit(String) }`,
//!   `SiegeEngineStatus { kind, crew_unit_id: String, position: BattlePoint,
//!   ammunition: Option<u16> (None for a ram), pulses_until_loaded: u16,
//!   last_shot_tick: Option<u64> }`, all re-exported from the crate root.
//! - `TacticalBattle::deploy_siege_with_engines(battlefield, units, location,
//!   profile, engines: Vec<SiegeEngineSetup>) -> Result<Self, TacticalError>`.
//! - `TacticalBattle::siege_engines(&self) -> Option<Vec<SiegeEngineStatus>>`
//!   (`None` outside a siege, otherwise setup order) — the presentation read
//!   model, like `siege_tower_activity` and `siege_ladders`.
//! - `TacticalBattle::issue_ram_order(&mut self, unit_id: &str)` and
//!   `TacticalBattle::issue_siege_engine_fire_order(&mut self, unit_id: &str,
//!   target: SiegeEngineTarget)`, both `-> Result<(), TacticalError>`.
//! - `TacticalError::SiegeEngineUnavailable(String)`.
//! - Rule constants re-exported from the crate root: `CATAPULT_MIN_RANGE_MM`,
//!   `CATAPULT_MAX_RANGE_MM` (u32), `CATAPULT_RELOAD_PULSES`,
//!   `CATAPULT_AMMUNITION` (u16); and `SiegeProfile::ram_damage_per_pulse`,
//!   `SiegeProfile::catapult_gate_damage_per_shot` (`const fn -> u16`).
//!
//! Deliberately left open: exact numeric tuning, crew movement speed while
//! moving an engine, catapult casualties per unit hit, tower damage (towers
//! still have no integrity), engine destruction by enemy fire, opponent-AI
//! engine use and the browser setup UI.

use medieval_core::{
    BattlePoint, BattleSide, BattlefieldLocation, CATAPULT_AMMUNITION, CATAPULT_MAX_RANGE_MM,
    CATAPULT_MIN_RANGE_MM, CATAPULT_RELOAD_PULSES, FlatBattlefield, Formation, MovementOrder,
    SIEGE_GATE_MAX_INTEGRITY, SiegeEngineKind, SiegeEngineSetup, SiegeEngineStatus,
    SiegeEngineTarget, SiegeProfile, TACTICAL_TICKS_PER_SECOND, TacticalBattle, TacticalError,
    TacticalUnit, UnitCombatProfile, UnitKind,
};

const PULSE: u64 = TACTICAL_TICKS_PER_SECOND as u64;
/// Liveness guard for reaching ordered positions, not a tuning budget.
const MOVE_TICK_LIMIT: u32 = 20_000;
/// Liveness guard for gate destruction (30 minutes of battle time).
const ASSAULT_TICK_LIMIT: u32 = 20 * 60 * 30;
const DEPTH: u32 = 100_000;
const ROW: u32 = 50_000;

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

fn big_unit(id: &str, side: BattleSide, x: u32, y: u32) -> TacticalUnit {
    TacticalUnit::new(
        id,
        side,
        60_000,
        BattlePoint::new(x, y),
        Formation::Line { files: 100 },
        1_000,
    )
}

fn setup(kind: SiegeEngineKind, crew: &str) -> SiegeEngineSetup {
    SiegeEngineSetup {
        kind,
        crew_unit_id: crew.into(),
    }
}

/// 100 m stone (or palisade) siege: a ram crew in front of the gate, a plain
/// assault unit beside it, a catapult crew, and two defenders away from the
/// gate, towers and capture point.
fn ram_siege(profile: SiegeProfile, engines: Vec<SiegeEngineSetup>) -> TacticalBattle {
    TacticalBattle::deploy_siege_with_engines(
        FlatBattlefield::new(100_000, DEPTH),
        vec![
            unit("ram-crew", BattleSide::Attacker, 20_000, ROW),
            unit("assault", BattleSide::Attacker, 20_000, 40_000),
            unit("catapult-crew", BattleSide::Attacker, 10_000, 20_000),
            unit("defender-north", BattleSide::Defender, 90_000, 10_000),
            unit("defender-south", BattleSide::Defender, 90_000, 90_000),
        ],
        BattlefieldLocation::ForestClearing,
        profile,
        engines,
    )
    .unwrap()
}

fn standard_engines() -> Vec<SiegeEngineSetup> {
    vec![
        setup(SiegeEngineKind::BatteringRam, "ram-crew"),
        setup(SiegeEngineKind::Catapult, "catapult-crew"),
    ]
}

/// A siege wide enough that a crew can stand `CATAPULT_MAX_RANGE_MM + 1` in
/// front of the gate center. Stone wall at 49–51 % of the width, defenders
/// deploy from 65 %.
fn catapult_field() -> FlatBattlefield {
    FlatBattlefield::new(2 * CATAPULT_MAX_RANGE_MM + 20_000, DEPTH)
}

fn catapult_siege(field: FlatBattlefield, defenders: Vec<TacticalUnit>) -> TacticalBattle {
    let mut units = vec![
        unit("catapult-crew", BattleSide::Attacker, 1_000, ROW),
        unit("ram-crew", BattleSide::Attacker, 1_000, 30_000),
        unit("ally", BattleSide::Attacker, 1_000, 70_000),
    ];
    units.extend(defenders);
    TacticalBattle::deploy_siege_with_engines(
        field,
        units,
        BattlefieldLocation::ForestClearing,
        SiegeProfile::StoneWallsV1,
        standard_engines(),
    )
    .unwrap()
}

fn defender_x(field: FlatBattlefield) -> u32 {
    field.width_mm * 66 / 100
}

fn find<'a>(battle: &'a TacticalBattle, id: &str) -> &'a TacticalUnit {
    battle.units().iter().find(|unit| unit.id() == id).unwrap()
}

fn engine(battle: &TacticalBattle, crew: &str) -> SiegeEngineStatus {
    battle
        .siege_engines()
        .unwrap()
        .into_iter()
        .find(|engine| engine.crew_unit_id == crew)
        .unwrap()
}

fn ammunition(battle: &TacticalBattle) -> u16 {
    engine(battle, "catapult-crew").ammunition.unwrap()
}

fn gate_integrity(battle: &TacticalBattle) -> u16 {
    battle.siege_snapshot().unwrap().gate_integrity
}

fn gate_state(battle: &TacticalBattle) -> String {
    serde_json::to_value(battle.siege_snapshot().unwrap().gate_state)
        .unwrap()
        .as_str()
        .unwrap()
        .to_owned()
}

fn gate_center(battle: &TacticalBattle) -> BattlePoint {
    battle.siege_snapshot().unwrap().layout.gate.center()
}

fn formed(unit: &TacticalUnit) -> bool {
    !(unit.is_routed() || unit.is_destroyed() || unit.is_escaped() || unit.is_withdrawing())
}

fn move_and_settle(battle: &mut TacticalBattle, id: &str, destination: BattlePoint) {
    battle
        .issue_move_order(MovementOrder {
            unit_id: id.into(),
            destination,
        })
        .unwrap();
    for _ in 0..MOVE_TICK_LIMIT {
        if find(battle, id).destination().is_none() {
            assert_eq!(find(battle, id).position(), destination);
            return;
        }
        battle.advance_ticks(1);
    }
    panic!("ordered unit must reach its destination within the liveness limit");
}

/// Advances to and through the next combat pulse.
fn next_pulse(battle: &mut TacticalBattle) {
    let ticks = PULSE - battle.tick() % PULSE;
    battle.advance_ticks(u32::try_from(ticks).unwrap());
}

/// Asserts the rejection and that the battle is unchanged.
fn rejected(
    battle: &mut TacticalBattle,
    order: impl FnOnce(&mut TacticalBattle) -> Result<(), TacticalError>,
) -> TacticalError {
    let before = battle.clone();
    let error = order(battle).expect_err("order must be rejected");
    assert_eq!(*battle, before, "a rejected order changes nothing");
    error
}

fn unavailable(error: TacticalError, crew: &str) {
    assert_eq!(error, TacticalError::SiegeEngineUnavailable(crew.into()));
}

// ---------------------------------------------------------------------------
// Pre-battle availability
// ---------------------------------------------------------------------------

#[test]
fn explicit_setup_lists_engines_in_setup_order_loaded_and_full() {
    let battle = ram_siege(SiegeProfile::StoneWallsV1, standard_engines());
    let engines = battle.siege_engines().unwrap();
    assert_eq!(engines.len(), 2);

    assert_eq!(engines[0].kind, SiegeEngineKind::BatteringRam);
    assert_eq!(engines[0].crew_unit_id, "ram-crew");
    assert_eq!(engines[0].position, find(&battle, "ram-crew").position());
    assert_eq!(engines[0].ammunition, None, "a ram uses no ammunition");
    assert_eq!(engines[0].last_shot_tick, None);

    assert_eq!(engines[1].kind, SiegeEngineKind::Catapult);
    assert_eq!(engines[1].crew_unit_id, "catapult-crew");
    assert_eq!(
        engines[1].position,
        find(&battle, "catapult-crew").position()
    );
    assert_eq!(engines[1].ammunition, Some(CATAPULT_AMMUNITION));
    assert_eq!(engines[1].pulses_until_loaded, 0, "engines start loaded");
    assert_eq!(engines[1].last_shot_tick, None);

    const { assert!(CATAPULT_AMMUNITION > 0) };
    const { assert!(CATAPULT_RELOAD_PULSES >= 2) };
    const { assert!(0 < CATAPULT_MIN_RANGE_MM && CATAPULT_MIN_RANGE_MM < CATAPULT_MAX_RANGE_MM) };
}

#[test]
fn sieges_without_explicit_setup_have_no_engines_and_reject_engine_orders() {
    let field = FlatBattlefield::new(100_000, DEPTH);
    let units = vec![
        unit("ram-crew", BattleSide::Attacker, 20_000, ROW),
        unit("defender", BattleSide::Defender, 90_000, 10_000),
    ];
    let sandbox = TacticalBattle::deploy_siege(field, units.clone()).unwrap();
    let profiled = TacticalBattle::deploy_siege_with_profile(
        field,
        units.clone(),
        BattlefieldLocation::ForestClearing,
        SiegeProfile::PalisadeV1,
    )
    .unwrap();
    let empty_setup = TacticalBattle::deploy_siege_with_engines(
        field,
        units,
        BattlefieldLocation::ForestClearing,
        SiegeProfile::PalisadeV1,
        Vec::new(),
    )
    .unwrap();
    assert_eq!(empty_setup, profiled, "an empty setup is the plain siege");

    for mut battle in [sandbox, profiled, empty_setup] {
        assert_eq!(battle.siege_engines(), Some(Vec::new()));
        unavailable(
            rejected(&mut battle, |battle| battle.issue_ram_order("ram-crew")),
            "ram-crew",
        );
        unavailable(
            rejected(&mut battle, |battle| {
                battle.issue_siege_engine_fire_order("ram-crew", SiegeEngineTarget::Gate)
            }),
            "ram-crew",
        );
        // The ordinary gate assault from #123 stays available without engines.
        assert_eq!(battle.issue_gate_attack_order("ram-crew"), Ok(()));
    }

    // Campaign sieges carry no equipment until a setup assigns it.
    let mut campaign = medieval_core::new_campaign();
    campaign.move_army("england-main", "paris").unwrap();
    let seed = campaign.pending_tactical_battle_seed().unwrap();
    let mut battle = TacticalBattle::from_campaign_seed_with_profile(
        FlatBattlefield::new(120_000, 80_000),
        seed,
        medieval_core::TacticalBattlefieldProfile::Siege {
            location: BattlefieldLocation::ForestClearing,
            fortification: SiegeProfile::StoneWallsV1,
        },
    )
    .unwrap();
    assert_eq!(battle.siege_engines(), Some(Vec::new()));
    let attackers: Vec<String> = battle
        .units()
        .iter()
        .filter(|unit| unit.side() == BattleSide::Attacker)
        .map(|unit| unit.id().to_owned())
        .collect();
    assert!(!attackers.is_empty());
    for id in attackers {
        unavailable(
            rejected(&mut battle, |battle| battle.issue_ram_order(&id)),
            &id,
        );
    }

    // A field battle has no siege engines at all.
    let field_battle = TacticalBattle::new(
        field,
        vec![
            unit("a", BattleSide::Attacker, 10_000, 10_000),
            unit("d", BattleSide::Defender, 90_000, 90_000),
        ],
    )
    .unwrap();
    assert_eq!(field_battle.siege_engines(), None);
}

#[test]
fn an_engine_is_usable_only_by_its_own_crew_and_only_for_its_kind() {
    let mut battle = ram_siege(SiegeProfile::StoneWallsV1, standard_engines());
    // Units without an engine.
    unavailable(
        rejected(&mut battle, |battle| battle.issue_ram_order("assault")),
        "assault",
    );
    unavailable(
        rejected(&mut battle, |battle| {
            battle.issue_siege_engine_fire_order("assault", SiegeEngineTarget::Gate)
        }),
        "assault",
    );
    // The catapult crew has no ram; the ram crew has no catapult.
    unavailable(
        rejected(&mut battle, |battle| {
            battle.issue_ram_order("catapult-crew")
        }),
        "catapult-crew",
    );
    unavailable(
        rejected(&mut battle, |battle| {
            battle.issue_siege_engine_fire_order("ram-crew", SiegeEngineTarget::Gate)
        }),
        "ram-crew",
    );
    // Defenders never hold attacker equipment.
    rejected(&mut battle, |battle| {
        battle.issue_ram_order("defender-north")
    });
    rejected(&mut battle, |battle| {
        battle.issue_siege_engine_fire_order("defender-north", SiegeEngineTarget::Gate)
    });
}

#[test]
fn invalid_pre_battle_setups_are_rejected() {
    let field = FlatBattlefield::new(100_000, DEPTH);
    let units = || {
        vec![
            unit("foot", BattleSide::Attacker, 20_000, ROW),
            unit("other", BattleSide::Attacker, 20_000, 30_000),
            unit("knights", BattleSide::Attacker, 20_000, 70_000)
                .with_combat_stats(UnitCombatProfile::v1(UnitKind::Knights)),
            unit("defender", BattleSide::Defender, 90_000, 10_000),
        ]
    };
    let deploy = |engines: Vec<SiegeEngineSetup>| {
        TacticalBattle::deploy_siege_with_engines(
            field,
            units(),
            BattlefieldLocation::ForestClearing,
            SiegeProfile::StoneWallsV1,
            engines,
        )
    };
    assert!(deploy(vec![setup(SiegeEngineKind::BatteringRam, "missing")]).is_err());
    assert!(
        deploy(vec![setup(SiegeEngineKind::Catapult, "defender")]).is_err(),
        "defenders never crew attacker engines"
    );
    assert!(
        deploy(vec![setup(SiegeEngineKind::BatteringRam, "knights")]).is_err(),
        "mounted units never crew engines"
    );
    assert!(
        deploy(vec![
            setup(SiegeEngineKind::BatteringRam, "foot"),
            setup(SiegeEngineKind::Catapult, "foot"),
        ])
        .is_err(),
        "one unit crews at most one engine"
    );
    let valid = deploy(vec![
        setup(SiegeEngineKind::BatteringRam, "foot"),
        setup(SiegeEngineKind::Catapult, "other"),
    ])
    .unwrap();
    assert_eq!(valid.siege_engines().unwrap().len(), 2);
}

#[test]
fn engine_crews_cannot_climb_ladders() {
    let mut battle = ram_siege(SiegeProfile::StoneWallsV1, standard_engines());
    let wall = battle.siege_snapshot().unwrap().layout.wall_segments[0];
    let at = BattlePoint::new(wall.min_x_mm + (wall.max_x_mm - wall.min_x_mm) / 2, 25_000);
    // An ordinary attacker may place a ladder there…
    let mut control = battle.clone();
    assert_eq!(control.issue_ladder_order("assault", at), Ok(()));
    // …but an engine does not climb walls.
    for crew in ["ram-crew", "catapult-crew"] {
        assert_eq!(
            rejected(&mut battle, |battle| battle.issue_ladder_order(crew, at)),
            TacticalError::LadderUnavailable(crew.into())
        );
    }
}

// ---------------------------------------------------------------------------
// Battering ram
// ---------------------------------------------------------------------------

#[test]
fn ram_damage_exceeds_unit_gate_damage_for_every_profile() {
    for profile in SiegeProfile::ALL {
        assert!(profile.ram_damage_per_pulse() > profile.gate_damage_per_pulse());
    }
}

#[test]
fn ram_orders_follow_gate_assault_legality() {
    let mut battle = ram_siege(SiegeProfile::StoneWallsV1, standard_engines());
    assert_eq!(battle.issue_ram_order("ram-crew"), Ok(()));
    assert_eq!(gate_integrity(&battle), SIEGE_GATE_MAX_INTEGRITY);
    assert_eq!(
        rejected(&mut battle, |battle| battle.issue_ram_order("missing")),
        TacticalError::UnitNotFound("missing".into())
    );

    for gate in ["open", "destroyed"] {
        let mut not_closed = ram_siege(SiegeProfile::StoneWallsV1, standard_engines());
        if gate == "open" {
            not_closed.open_siege_gate().unwrap();
        } else {
            not_closed.destroy_siege_gate().unwrap();
        }
        rejected(&mut not_closed, |battle| battle.issue_ram_order("ram-crew"));
    }

    // A routed crew cannot use its ram.
    let mut document =
        serde_json::to_value(ram_siege(SiegeProfile::StoneWallsV1, standard_engines())).unwrap();
    for unit in document["units"].as_array_mut().unwrap() {
        if unit["id"] == "ram-crew" {
            unit["state"] = serde_json::json!("routed");
        }
    }
    let mut routed: TacticalBattle = serde_json::from_value(document).unwrap();
    assert_eq!(
        rejected(&mut routed, |battle| battle.issue_ram_order("ram-crew")),
        TacticalError::UnitCannotReceiveOrders {
            unit_id: "ram-crew".into()
        }
    );

    // A finished battle rejects every command.
    let mut finished = TacticalBattle::deploy_siege_with_engines(
        FlatBattlefield::new(100_000, DEPTH),
        vec![unit("ram-crew", BattleSide::Attacker, 20_000, ROW)],
        BattlefieldLocation::ForestClearing,
        SiegeProfile::StoneWallsV1,
        vec![setup(SiegeEngineKind::BatteringRam, "ram-crew")],
    )
    .unwrap()
    .start();
    assert_eq!(
        rejected(&mut finished, |battle| battle.issue_ram_order("ram-crew")),
        TacticalError::BattleFinished
    );
}

/// Ticks one at a time until the gate falls. Returns the destruction tick and
/// the (tick, integrity, crew within reach) sample of every combat pulse.
fn ram_until_destroyed(battle: &mut TacticalBattle, crew: &str) -> (u64, Vec<(u64, u16, bool)>) {
    let mut pulses = Vec::new();
    for _ in 0..ASSAULT_TICK_LIMIT {
        battle.advance_ticks(1);
        let siege = battle.siege_snapshot().unwrap();
        if battle.tick().is_multiple_of(PULSE) {
            pulses.push((
                battle.tick(),
                siege.gate_integrity,
                siege.within_gate_attack_distance(find(battle, crew).position()),
            ));
        }
        if gate_state(battle) == "destroyed" {
            return (battle.tick(), pulses);
        }
    }
    panic!("an ordered assault must destroy the gate within the liveness limit");
}

#[test]
fn a_ram_strikes_only_within_reach_and_destroys_the_gate_on_the_exact_pulse() {
    for profile in SiegeProfile::ALL {
        let damage = profile.ram_damage_per_pulse();
        let mut battle = ram_siege(profile, standard_engines());
        let start = find(&battle, "ram-crew").position();
        assert!(
            !battle
                .siege_snapshot()
                .unwrap()
                .within_gate_attack_distance(start),
            "fixture: the crew starts out of reach"
        );
        battle.issue_ram_order("ram-crew").unwrap();
        let (destroyed_at, pulses) = ram_until_destroyed(&mut battle, "ram-crew");

        let first = pulses
            .iter()
            .position(|(_, integrity, _)| *integrity < SIEGE_GATE_MAX_INTEGRITY)
            .unwrap();
        assert!(first > 0, "the crew must first approach the gate");
        for (_, integrity, _) in &pulses[..first] {
            assert_eq!(*integrity, SIEGE_GATE_MAX_INTEGRITY);
        }
        let (_, _, reached_before) = pulses[first - 1];
        assert!(
            !reached_before,
            "no damage is dealt before the crew is in reach"
        );

        let strikes = SIEGE_GATE_MAX_INTEGRITY.div_ceil(damage);
        for (offset, (_, integrity, within)) in pulses[first..].iter().enumerate() {
            assert!(within, "the crew strikes only from within reach");
            let dealt = damage.saturating_mul(u16::try_from(offset + 1).unwrap());
            assert_eq!(
                *integrity,
                SIEGE_GATE_MAX_INTEGRITY.saturating_sub(dealt),
                "{profile:?}: each pulse removes exactly the ram damage"
            );
        }
        let first_tick = pulses[first].0;
        assert_eq!(
            destroyed_at,
            first_tick + u64::from(strikes - 1) * PULSE,
            "{profile:?}: the gate falls on the pulse integrity reaches zero"
        );
        assert_eq!(gate_integrity(&battle), 0);
        assert!(battle.siege_snapshot().unwrap().gate_state.is_traversable());
        // The order has ended and cannot be reissued against a destroyed gate.
        rejected(&mut battle, |battle| battle.issue_ram_order("ram-crew"));
    }
}

#[test]
fn a_ram_breaches_sooner_than_a_plain_assault_from_the_same_start() {
    let mut ram = ram_siege(SiegeProfile::StoneWallsV1, standard_engines());
    ram.issue_ram_order("ram-crew").unwrap();
    let (ram_tick, _) = ram_until_destroyed(&mut ram, "ram-crew");

    let mut plain = ram_siege(SiegeProfile::StoneWallsV1, Vec::new());
    plain.issue_gate_attack_order("ram-crew").unwrap();
    let mut plain_tick = None;
    for _ in 0..ASSAULT_TICK_LIMIT {
        plain.advance_ticks(1);
        if gate_state(&plain) == "destroyed" {
            plain_tick = Some(plain.tick());
            break;
        }
    }
    assert!(ram_tick < plain_tick.expect("plain assault destroys the gate"));
}

#[test]
fn a_move_order_replaces_the_ram_order() {
    let mut battle = ram_siege(SiegeProfile::StoneWallsV1, standard_engines());
    battle.issue_ram_order("ram-crew").unwrap();
    // Strike once…
    for _ in 0..ASSAULT_TICK_LIMIT {
        if gate_integrity(&battle) < SIEGE_GATE_MAX_INTEGRITY {
            break;
        }
        battle.advance_ticks(1);
    }
    let integrity = gate_integrity(&battle);
    assert!(integrity < SIEGE_GATE_MAX_INTEGRITY);
    // …then hold the crew in place, still within reach, under a move order.
    let here = find(&battle, "ram-crew").position();
    battle
        .issue_move_order(MovementOrder {
            unit_id: "ram-crew".into(),
            destination: here,
        })
        .unwrap();
    battle.advance_ticks(10 * TACTICAL_TICKS_PER_SECOND);
    assert_eq!(
        gate_integrity(&battle),
        integrity,
        "a crew pressed against the gate under another order never strikes"
    );
}

// ---------------------------------------------------------------------------
// Catapult
// ---------------------------------------------------------------------------

/// Moves the catapult crew to `distance` in front of the gate center.
fn crew_before_gate(battle: &mut TacticalBattle, distance: u32) {
    let center = gate_center(battle);
    let gate = battle.siege_snapshot().unwrap().layout.gate;
    let spot = BattlePoint::new(center.x_mm - distance, center.y_mm);
    assert!(
        spot.x_mm < gate.min_x_mm,
        "fixture: the crew stands outside the gate band"
    );
    move_and_settle(battle, "catapult-crew", spot);
}

#[test]
fn catapult_gate_shots_respect_inclusive_range_bounds() {
    // Minimum range on a field small enough that the min-range spot clears the
    // wall band; maximum range on the wide field.
    let near_field = FlatBattlefield::new(4 * CATAPULT_MIN_RANGE_MM + 20_000, DEPTH);
    for (field, distance, legal) in [
        (near_field, CATAPULT_MIN_RANGE_MM - 1, false),
        (near_field, CATAPULT_MIN_RANGE_MM, true),
        (catapult_field(), CATAPULT_MAX_RANGE_MM, true),
        (catapult_field(), CATAPULT_MAX_RANGE_MM + 1, false),
    ] {
        let mut battle = catapult_siege(
            field,
            vec![unit(
                "defender",
                BattleSide::Defender,
                field.width_mm - 1_000,
                10_000,
            )],
        );
        crew_before_gate(&mut battle, distance);
        if legal {
            assert_eq!(
                battle.issue_siege_engine_fire_order("catapult-crew", SiegeEngineTarget::Gate),
                Ok(()),
                "gate at {distance} mm is in range"
            );
        } else {
            rejected(&mut battle, |battle| {
                battle.issue_siege_engine_fire_order("catapult-crew", SiegeEngineTarget::Gate)
            });
        }
    }
}

#[test]
fn catapult_unit_targets_must_be_formed_enemies_in_range() {
    let field = catapult_field();
    let target_x = defender_x(field);
    let mut battle = catapult_siege(
        field,
        vec![
            unit("target", BattleSide::Defender, target_x, ROW),
            unit("routed", BattleSide::Defender, target_x, 20_000),
        ],
    );
    // Rout one defender through the document, as the gate tests do.
    let mut document = serde_json::to_value(&battle).unwrap();
    for unit in document["units"].as_array_mut().unwrap() {
        if unit["id"] == "routed" {
            unit["state"] = serde_json::json!("routed");
        }
    }
    battle = serde_json::from_value(document).unwrap();

    assert!(target_x - CATAPULT_MAX_RANGE_MM - 1 > 0);
    move_and_settle(
        &mut battle,
        "catapult-crew",
        BattlePoint::new(target_x - CATAPULT_MAX_RANGE_MM - 1, ROW),
    );
    rejected(&mut battle, |battle| {
        battle.issue_siege_engine_fire_order(
            "catapult-crew",
            SiegeEngineTarget::Unit("target".into()),
        )
    });

    move_and_settle(
        &mut battle,
        "catapult-crew",
        BattlePoint::new(target_x - CATAPULT_MAX_RANGE_MM, ROW),
    );
    for target in ["ally", "ram-crew", "catapult-crew", "routed", "missing"] {
        rejected(&mut battle, |battle| {
            battle.issue_siege_engine_fire_order(
                "catapult-crew",
                SiegeEngineTarget::Unit(target.into()),
            )
        });
    }
    // Walls do not block the indirect shot: the target stands behind the wall.
    assert_eq!(
        battle.issue_siege_engine_fire_order(
            "catapult-crew",
            SiegeEngineTarget::Unit("target".into())
        ),
        Ok(())
    );

    // A gate that is no longer closed is not a target.
    let mut open = battle.clone();
    open.open_siege_gate().unwrap();
    rejected(&mut open, |battle| {
        battle.issue_siege_engine_fire_order("catapult-crew", SiegeEngineTarget::Gate)
    });
}

/// A catapult crew at the middle of its range in front of the gate.
fn gate_bombardment() -> TacticalBattle {
    let field = catapult_field();
    let mut battle = catapult_siege(
        field,
        vec![unit(
            "defender",
            BattleSide::Defender,
            field.width_mm - 1_000,
            10_000,
        )],
    );
    crew_before_gate(
        &mut battle,
        CATAPULT_MIN_RANGE_MM + (CATAPULT_MAX_RANGE_MM - CATAPULT_MIN_RANGE_MM) / 2,
    );
    battle
}

/// Samples (tick, integrity, ammunition, pulses_until_loaded, last_shot_tick)
/// on every combat pulse.
fn pulse_samples(
    battle: &mut TacticalBattle,
    pulses: usize,
) -> Vec<(u64, u16, u16, u16, Option<u64>)> {
    (0..pulses)
        .map(|_| {
            next_pulse(battle);
            let catapult = engine(battle, "catapult-crew");
            (
                battle.tick(),
                gate_integrity(battle),
                catapult.ammunition.unwrap(),
                catapult.pulses_until_loaded,
                catapult.last_shot_tick,
            )
        })
        .collect()
}

#[test]
fn catapult_gate_shots_deal_exact_damage_on_a_fixed_reload_cadence() {
    let damage = SiegeProfile::StoneWallsV1.catapult_gate_damage_per_shot();
    assert!(damage > 0);
    let mut battle = gate_bombardment();
    let order_tick = battle.tick();
    battle
        .issue_siege_engine_fire_order("catapult-crew", SiegeEngineTarget::Gate)
        .unwrap();
    assert_eq!(
        ammunition(&battle),
        CATAPULT_AMMUNITION,
        "ordering spends nothing"
    );
    assert_eq!(gate_integrity(&battle), SIEGE_GATE_MAX_INTEGRITY);

    let reload = usize::from(CATAPULT_RELOAD_PULSES);
    let shots_to_destroy = SIEGE_GATE_MAX_INTEGRITY.div_ceil(damage);
    let shots = usize::from(shots_to_destroy.min(CATAPULT_AMMUNITION).min(3));
    let samples = pulse_samples(&mut battle, (shots - 1) * reload + 1);

    // A loaded engine fires on the first pulse after the order.
    let first_pulse = (order_tick / PULSE + 1) * PULSE;
    assert_eq!(samples[0].0, first_pulse);

    let mut previous = (SIEGE_GATE_MAX_INTEGRITY, CATAPULT_AMMUNITION);
    for (index, &(tick, integrity, ammo, until_loaded, last_shot)) in samples.iter().enumerate() {
        let since_shot = index % reload;
        let fired = since_shot == 0;
        if fired {
            let shot = u16::try_from(index / reload + 1).unwrap();
            assert_eq!(last_shot, Some(tick), "pulse {index}: shot recorded");
            assert_eq!(ammo, CATAPULT_AMMUNITION - shot, "one stone per shot");
            assert_eq!(
                integrity,
                SIEGE_GATE_MAX_INTEGRITY.saturating_sub(damage * shot),
                "each shot removes exactly the gate damage"
            );
            assert_eq!(until_loaded, CATAPULT_RELOAD_PULSES);
        } else {
            assert_eq!((integrity, ammo), previous, "pulse {index}: reloading");
            assert_eq!(
                until_loaded,
                CATAPULT_RELOAD_PULSES - u16::try_from(since_shot).unwrap()
            );
            assert_eq!(last_shot, Some(tick - since_shot as u64 * PULSE));
        }
        previous = (integrity, ammo);
    }
}

#[test]
fn catapult_shots_destroy_the_gate_exactly_at_zero_when_ammunition_suffices() {
    let damage = SiegeProfile::StoneWallsV1.catapult_gate_damage_per_shot();
    let needed = SIEGE_GATE_MAX_INTEGRITY.div_ceil(damage);
    let mut battle = gate_bombardment();
    battle
        .issue_siege_engine_fire_order("catapult-crew", SiegeEngineTarget::Gate)
        .unwrap();
    let shots = needed.min(CATAPULT_AMMUNITION);
    let reload = usize::from(CATAPULT_RELOAD_PULSES);
    let samples = pulse_samples(&mut battle, (usize::from(shots) - 1) * reload + 1);
    let last = samples.last().unwrap();
    if needed <= CATAPULT_AMMUNITION {
        assert_eq!(last.1, 0);
        assert_eq!(gate_state(&battle), "destroyed");
        for sample in &samples[..samples.len() - 1] {
            assert!(sample.1 > 0);
        }
        // The fire order ends with the gate: no stone is wasted on rubble.
        let ammo = ammunition(&battle);
        battle.advance_ticks(u32::from(CATAPULT_RELOAD_PULSES) * 3 * TACTICAL_TICKS_PER_SECOND);
        assert_eq!(ammunition(&battle), ammo);
    } else {
        assert!(last.1 > 0);
        assert_eq!(gate_state(&battle), "closed");
    }
}

#[test]
fn catapult_ammunition_runs_out_and_an_empty_catapult_stops_firing() {
    let field = catapult_field();
    let target_x = defender_x(field);
    // Large defenders a few metres apart along the target row, so the crew can
    // switch to a formed target whenever one routs.
    let offsets: [i64; 7] = [0, -2_000, 2_000, -4_000, 4_000, -6_000, 6_000];
    let defenders = offsets
        .iter()
        .enumerate()
        .map(|(index, offset)| {
            big_unit(
                &format!("target-{index}"),
                BattleSide::Defender,
                target_x,
                u32::try_from(i64::from(ROW) + offset).unwrap(),
            )
        })
        .collect();
    let mut battle = catapult_siege(field, defenders);
    let mid = CATAPULT_MIN_RANGE_MM + (CATAPULT_MAX_RANGE_MM - CATAPULT_MIN_RANGE_MM) / 2;
    let crew_at = BattlePoint::new(target_x - mid, ROW);
    for offset in offsets {
        let dy = offset.unsigned_abs();
        let distance_sq = u64::from(mid) * u64::from(mid) + dy * dy;
        let max = u64::from(CATAPULT_MAX_RANGE_MM);
        assert!(distance_sq <= max * max, "fixture: every target in range");
    }
    move_and_settle(&mut battle, "catapult-crew", crew_at);

    let soldiers = |battle: &TacticalBattle| -> u32 {
        battle
            .units()
            .iter()
            .filter(|unit| unit.side() == BattleSide::Defender)
            .map(|unit| u32::from(unit.soldiers()))
            .sum()
    };
    let mut current: Option<String> = None;
    let mut shots = 0_u16;
    let mut losses_on_shots = 0_u32;
    let pulse_limit =
        usize::from(CATAPULT_AMMUNITION + 1) * usize::from(CATAPULT_RELOAD_PULSES) * 2;
    for _ in 0..pulse_limit {
        if ammunition(&battle) == 0 {
            break;
        }
        let needs_target = current
            .as_deref()
            .is_none_or(|id| !formed(find(&battle, id)));
        if needs_target {
            let next = battle
                .units()
                .iter()
                .find(|unit| unit.side() == BattleSide::Defender && formed(unit))
                .expect("fixture: a formed target remains while ammunition lasts")
                .id()
                .to_owned();
            battle
                .issue_siege_engine_fire_order(
                    "catapult-crew",
                    SiegeEngineTarget::Unit(next.clone()),
                )
                .unwrap();
            current = Some(next);
        }
        let (ammo_before, soldiers_before) = (ammunition(&battle), soldiers(&battle));
        next_pulse(&mut battle);
        let fired = engine(&battle, "catapult-crew").last_shot_tick == Some(battle.tick());
        if fired {
            shots += 1;
            assert_eq!(ammunition(&battle), ammo_before - 1);
            losses_on_shots += soldiers_before - soldiers(&battle);
        } else {
            assert_eq!(
                ammunition(&battle),
                ammo_before,
                "no stone spent without a shot"
            );
            assert_eq!(
                soldiers(&battle),
                soldiers_before,
                "catapult casualties land only on shot pulses"
            );
        }
    }
    assert_eq!(ammunition(&battle), 0, "ammunition is exhausted");
    assert_eq!(shots, CATAPULT_AMMUNITION, "exactly one shot per stone");
    assert!(losses_on_shots > 0, "unit hits inflict casualties");

    // Empty: no more shots and no more fire orders.
    let soldiers_left = soldiers(&battle);
    let last_shot = engine(&battle, "catapult-crew").last_shot_tick;
    battle.advance_ticks(u32::from(CATAPULT_RELOAD_PULSES) * 3 * TACTICAL_TICKS_PER_SECOND);
    assert_eq!(ammunition(&battle), 0);
    assert_eq!(engine(&battle, "catapult-crew").last_shot_tick, last_shot);
    assert_eq!(soldiers(&battle), soldiers_left);
    let target = battle
        .units()
        .iter()
        .find(|unit| unit.side() == BattleSide::Defender && formed(unit))
        .map(|unit| unit.id().to_owned());
    if let Some(target) = target {
        unavailable(
            rejected(&mut battle, |battle| {
                battle.issue_siege_engine_fire_order(
                    "catapult-crew",
                    SiegeEngineTarget::Unit(target.clone()),
                )
            }),
            "catapult-crew",
        );
    }
}

#[test]
fn moving_or_routing_the_crew_stops_the_catapult() {
    let mut battle = gate_bombardment();
    battle
        .issue_siege_engine_fire_order("catapult-crew", SiegeEngineTarget::Gate)
        .unwrap();
    next_pulse(&mut battle);
    let after_first = (ammunition(&battle), gate_integrity(&battle));
    assert_eq!(after_first.0, CATAPULT_AMMUNITION - 1);

    // Saved with a fire order pending, then the crew routs.
    let mut document = serde_json::to_value(&battle).unwrap();
    for unit in document["units"].as_array_mut().unwrap() {
        if unit["id"] == "catapult-crew" {
            unit["state"] = serde_json::json!("routed");
        }
    }
    let mut routed: TacticalBattle = serde_json::from_value(document).unwrap();
    routed.advance_ticks(u32::from(CATAPULT_RELOAD_PULSES) * 3 * TACTICAL_TICKS_PER_SECOND);
    assert_eq!(
        (ammunition(&routed), gate_integrity(&routed)),
        after_first,
        "a routed crew does not work its engine"
    );

    // A move order replaces the fire order.
    let here = find(&battle, "catapult-crew").position();
    let back = BattlePoint::new(here.x_mm - 1_000, here.y_mm);
    battle
        .issue_move_order(MovementOrder {
            unit_id: "catapult-crew".into(),
            destination: back,
        })
        .unwrap();
    battle.advance_ticks(u32::from(CATAPULT_RELOAD_PULSES) * 3 * TACTICAL_TICKS_PER_SECOND);
    assert_eq!(find(&battle, "catapult-crew").position(), back);
    assert_eq!(
        (ammunition(&battle), gate_integrity(&battle)),
        after_first,
        "a moved crew stops firing until ordered again"
    );
    assert_eq!(
        engine(&battle, "catapult-crew").position,
        back,
        "the engine moves with its crew"
    );
}

// ---------------------------------------------------------------------------
// Determinism and persistence
// ---------------------------------------------------------------------------

fn combined_assault() -> TacticalBattle {
    let mut battle = gate_bombardment();
    battle.issue_ram_order("ram-crew").unwrap();
    battle
        .issue_siege_engine_fire_order("catapult-crew", SiegeEngineTarget::Gate)
        .unwrap();
    battle
}

fn timeline(battle: &mut TacticalBattle, ticks: u32) -> Vec<(u64, u16, Vec<SiegeEngineStatus>)> {
    (0..ticks)
        .map(|_| {
            battle.advance_ticks(1);
            (
                battle.tick(),
                gate_integrity(battle),
                battle.siege_engines().unwrap(),
            )
        })
        .collect()
}

#[test]
fn engine_timelines_are_deterministic_and_saves_resume_mid_reload() {
    let span = u32::from(CATAPULT_RELOAD_PULSES) * 4 * TACTICAL_TICKS_PER_SECOND;
    let mut first = combined_assault();
    let mut second = combined_assault();
    assert_eq!(first, second);
    assert_eq!(timeline(&mut first, span), timeline(&mut second, span));
    assert_eq!(first, second);

    // Save while the catapult is reloading after its first shot.
    let mut saved = combined_assault();
    next_pulse(&mut saved);
    saved.advance_ticks(TACTICAL_TICKS_PER_SECOND + 3);
    let status = engine(&saved, "catapult-crew");
    assert!(status.last_shot_tick.is_some());
    assert!(
        (1..CATAPULT_RELOAD_PULSES).contains(&status.pulses_until_loaded),
        "fixture: saved mid-reload"
    );
    let encoded = serde_json::to_string(&saved).unwrap();
    let mut restored: TacticalBattle = serde_json::from_str(&encoded).unwrap();
    assert_eq!(restored, saved);
    assert_eq!(restored.siege_engines(), saved.siege_engines());

    let mut reference = saved.clone();
    let rest = u32::from(CATAPULT_RELOAD_PULSES) * 3 * TACTICAL_TICKS_PER_SECOND;
    assert_eq!(
        timeline(&mut restored, rest),
        timeline(&mut reference, rest)
    );
    assert_eq!(restored, reference);
}

#[test]
fn battles_without_engines_keep_the_pre_engine_document() {
    let plain = ram_siege(SiegeProfile::StoneWallsV1, Vec::new());
    let encoded = serde_json::to_string(&plain).unwrap();
    assert!(
        !encoded.to_ascii_lowercase().contains("engine"),
        "no engine field is written while a battle has none: {encoded}"
    );
    let restored: TacticalBattle = serde_json::from_str(&encoded).unwrap();
    assert_eq!(restored, plain);
    assert_eq!(restored.siege_engines(), Some(Vec::new()));
    assert_eq!(serde_json::to_string(&restored).unwrap(), encoded);

    // With engines the document round-trips exactly, too.
    let equipped = ram_siege(SiegeProfile::StoneWallsV1, standard_engines());
    let encoded = serde_json::to_string(&equipped).unwrap();
    let restored: TacticalBattle = serde_json::from_str(&encoded).unwrap();
    assert_eq!(restored, equipped);
    assert_eq!(restored.siege_engines(), equipped.siege_engines());
}
