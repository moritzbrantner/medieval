//! Acceptance for issue #94: deployment through the shared Rust tactical
//! controls. `shared/tactical_controls.rs` is compiled verbatim into both the
//! browser battle (`web-battle-wasm`) and the desktop battle window
//! (`src-tauri`), so exercising it here is exercising the semantics both
//! surfaces share. It is included the same way those crates include it.
//!
//! Seams named here: the request kinds `deploySelected` (`xMm`, `yMm`) and
//! `startBattle`, mapped onto the core `deploy_formation` and `begin_battle`
//! deployment APIs (see `medieval-core/tests/tactical_deployment.rs`).

#[allow(dead_code, unused_imports)]
mod shared {
    include!("../../../shared/tactical_controls.rs");
}

use medieval_core::{
    BattlePoint, BattleSide, BattlefieldLocation, DeploymentZone, Facing, FlatBattlefield,
    Formation, TacticalBattle, TacticalBattleState, TacticalFinishReason, TacticalUnit,
};
use shared::{TacticalControlError, TacticalControlRequest, TacticalControls};

fn unit(id: &str, side: BattleSide, x_mm: u32, y_mm: u32) -> TacticalUnit {
    TacticalUnit::new(
        id,
        side,
        40,
        BattlePoint::new(x_mm, y_mm),
        Formation::Line { files: 10 },
        1_000,
    )
    .with_facing(match side {
        BattleSide::Attacker => Facing::east(),
        BattleSide::Defender => Facing::west(),
    })
}

/// 90 m × 60 m: attacker zone x 0..=30 000, defender zone x 60 000..=90 000.
fn deploying() -> (TacticalBattle, TacticalControls) {
    let battle = TacticalBattle::deploy_at_location(
        FlatBattlefield::new(90_000, 60_000),
        vec![
            unit("attacker-a", BattleSide::Attacker, 10_000, 10_000),
            unit("attacker-b", BattleSide::Attacker, 10_000, 30_000),
            unit("defender", BattleSide::Defender, 80_000, 30_000),
        ],
        BattlefieldLocation::ForestClearing,
    )
    .unwrap()
    .into_deployment()
    .unwrap();
    let controls = TacticalControls::new(&battle, BattleSide::Attacker);
    (battle, controls)
}

fn request(kind: &str) -> TacticalControlRequest {
    TacticalControlRequest {
        kind: kind.to_owned(),
        ..TacticalControlRequest::default()
    }
}

fn select(controls: &mut TacticalControls, battle: &mut TacticalBattle, id: &str) {
    controls
        .apply_request(
            battle,
            TacticalControlRequest {
                unit_ids: Some(vec![id.to_owned()]),
                ..request("selectReplace")
            },
        )
        .unwrap();
}

fn deploy_at(x_mm: u32, y_mm: u32) -> TacticalControlRequest {
    TacticalControlRequest {
        x_mm: Some(x_mm),
        y_mm: Some(y_mm),
        ..request("deploySelected")
    }
}

fn position(battle: &TacticalBattle, id: &str) -> BattlePoint {
    battle
        .units()
        .iter()
        .find(|unit| unit.id() == id)
        .unwrap()
        .position()
}

fn attacker_zone(battle: &TacticalBattle) -> DeploymentZone {
    battle
        .deployment_zones()
        .into_iter()
        .find(|zone| zone.side == BattleSide::Attacker)
        .unwrap()
}

/// Asserts a request is refused by the Rust rules and changes neither the
/// battle nor the control state.
fn assert_rule_rejected(
    controls: &mut TacticalControls,
    battle: &mut TacticalBattle,
    request: TacticalControlRequest,
) {
    let battle_before = battle.clone();
    let controls_before = controls.clone();
    match controls.apply_request(battle, request) {
        Err(TacticalControlError::RuleRejected(_)) => {}
        other => panic!("expected a Rust rule rejection, got {other:?}"),
    }
    assert_eq!(*battle, battle_before);
    assert_eq!(*controls, controls_before);
}

#[test]
fn deploy_selected_places_the_own_formation_under_the_core_rule() {
    let (mut battle, mut controls) = deploying();
    assert!(matches!(
        controls.apply_request(&mut battle, deploy_at(15_000, 20_000)),
        Err(TacticalControlError::NoUnitsSelected)
    ));

    select(&mut controls, &mut battle, "attacker-a");
    controls
        .apply_request(&mut battle, deploy_at(15_000, 20_000))
        .unwrap();
    assert_eq!(
        position(&battle, "attacker-a"),
        BattlePoint::new(15_000, 20_000)
    );
    let placed = battle
        .units()
        .iter()
        .find(|unit| unit.id() == "attacker-a")
        .unwrap();
    assert_eq!(placed.destination(), None);
    assert_eq!(battle.tick(), 0);
    assert_eq!(battle.state(), TacticalBattleState::Deployment);

    // Neutral ground, an overhanging footprint and a friendly overlap are all
    // refused by the core and leave everything unchanged.
    assert_rule_rejected(&mut controls, &mut battle, deploy_at(45_000, 20_000));
    let edge = attacker_zone(&battle).max_x_mm - 1;
    assert_rule_rejected(&mut controls, &mut battle, deploy_at(edge, 20_000));
    let friend = position(&battle, "attacker-b");
    assert_rule_rejected(
        &mut controls,
        &mut battle,
        deploy_at(friend.x_mm, friend.y_mm),
    );

    // Enemy formations cannot be selected, so they cannot be deployed.
    assert!(matches!(
        controls.apply_request(
            &mut battle,
            TacticalControlRequest {
                unit_ids: Some(vec!["defender".to_owned()]),
                ..request("selectReplace")
            },
        ),
        Err(TacticalControlError::UncontrollableUnit(_))
    ));
    assert_eq!(
        position(&battle, "defender"),
        BattlePoint::new(80_000, 30_000)
    );
}

#[test]
fn facing_and_frontage_controls_apply_in_place_and_respect_the_rule() {
    let (mut battle, mut controls) = deploying();
    select(&mut controls, &mut battle, "attacker-a");
    controls
        .apply_request(&mut battle, deploy_at(15_000, 20_000))
        .unwrap();
    controls
        .apply_request(
            &mut battle,
            TacticalControlRequest {
                quarter_turns: Some(-1),
                ..request("turnSelected")
            },
        )
        .unwrap();
    let turned = battle
        .units()
        .iter()
        .find(|unit| unit.id() == "attacker-a")
        .unwrap();
    assert_eq!(turned.facing().unwrap().direction(), [0, -1]);
    assert_eq!(turned.position(), BattlePoint::new(15_000, 20_000));
    assert_eq!(turned.destination(), None);

    controls
        .apply_request(
            &mut battle,
            TacticalControlRequest {
                width_mm: Some(16_000),
                ..request("frontageSelected")
            },
        )
        .unwrap();
    let wider = battle
        .units()
        .iter()
        .find(|unit| unit.id() == "attacker-a")
        .unwrap();
    assert_eq!(wider.formation(), Formation::Line { files: 16 });
    assert_eq!(wider.position(), BattlePoint::new(15_000, 20_000));
    assert_eq!(battle.tick(), 0);

    // 32 m is wider than the 30 m zone itself.
    assert_rule_rejected(
        &mut controls,
        &mut battle,
        TacticalControlRequest {
            width_mm: Some(32_000),
            ..request("frontageSelected")
        },
    );
}

#[test]
fn start_battle_is_a_shared_control_that_starts_exactly_once() {
    let (mut battle, mut controls) = deploying();
    select(&mut controls, &mut battle, "attacker-a");
    controls
        .apply_request(&mut battle, deploy_at(15_000, 20_000))
        .unwrap();
    let deployed = battle.clone();

    controls
        .apply_request(&mut battle, request("startBattle"))
        .unwrap();
    assert_eq!(battle.state(), TacticalBattleState::Running);
    assert_eq!(battle.tick(), 0);
    assert_eq!(battle.units(), deployed.units());

    assert_rule_rejected(&mut controls, &mut battle, request("startBattle"));
    assert_rule_rejected(&mut controls, &mut battle, deploy_at(12_000, 20_000));

    // After the start a move is an ordinary march again.
    controls
        .apply_request(
            &mut battle,
            TacticalControlRequest {
                x_mm: Some(20_000),
                y_mm: Some(20_000),
                ..request("moveSelected")
            },
        )
        .unwrap();
    let marching = battle
        .units()
        .iter()
        .find(|unit| unit.id() == "attacker-a")
        .unwrap();
    assert_eq!(marching.position(), BattlePoint::new(15_000, 20_000));
    assert_eq!(
        marching.destination(),
        Some(BattlePoint::new(20_000, 20_000))
    );
}

#[test]
fn withdraw_is_allowed_during_deployment() {
    let (mut battle, mut controls) = deploying();
    controls
        .apply_request(&mut battle, request("withdraw"))
        .unwrap();
    assert_ne!(battle.state(), TacticalBattleState::Deployment);
    for _ in 0..200 {
        if matches!(battle.state(), TacticalBattleState::Finished { .. }) {
            break;
        }
        battle.advance_ticks(100);
    }
    assert!(matches!(
        battle.state(),
        TacticalBattleState::Finished {
            winner: Some(BattleSide::Defender),
            reason: TacticalFinishReason::Withdrawal,
            ..
        }
    ));
}
