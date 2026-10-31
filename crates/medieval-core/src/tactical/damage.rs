//! Pure casualty, frontage and damage arithmetic.

use super::{
    BattlePoint, CombatDamageCredit, FlatBattlefield, MAX_TACTICAL_MORALE, MELEE_CASUALTY_DIVISOR,
    PURSUIT_CASUALTY_DIVISOR, RANGED_CASUALTY_DIVISOR, TacticalUnit, TacticalUnitState,
};
use crate::deployment::siege::SiegeProfile;
use crate::siege_engine::CATAPULT_SHOT_FRONTAGE;
use crate::terrain::{COMBAT_FACTOR_BASE_MILLI, TacticalTerrain};
use std::collections::BTreeMap;

pub(super) fn allocated_frontage(
    attacker: &TacticalUnit,
    contacts: u16,
    assigned: &mut u16,
) -> u16 {
    let frontage = attacker.frontage_slots();
    let allocation = if contacts == 0 || *assigned >= contacts {
        0
    } else {
        frontage / contacts + u16::from(*assigned < frontage % contacts)
    };
    *assigned = assigned.saturating_add(1);
    allocation
}

fn terrain_adjusted_frontage(
    effective_frontage: u32,
    attacker: &TacticalUnit,
    defender: &TacticalUnit,
    terrain: TacticalTerrain,
    battlefield: FlatBattlefield,
) -> u32 {
    effective_frontage.saturating_mul(terrain.elevation_damage_factor_milli(
        battlefield,
        attacker.position,
        defender.position,
    )) / COMBAT_FACTOR_BASE_MILLI
}

pub(super) fn melee_casualties_with_credit(
    attacker: &TacticalUnit,
    defender: &TacticalUnit,
    frontage: u16,
    terrain: TacticalTerrain,
    battlefield: FlatBattlefield,
    credits: &mut BTreeMap<String, BTreeMap<String, CombatDamageCredit>>,
) -> u16 {
    if attacker.stats().is_none() && defender.stats().is_none() {
        return melee_casualties(attacker, defender, frontage, terrain, battlefield);
    }
    if attacker.state != TacticalUnitState::Formed || defender.soldiers == 0 || frontage == 0 {
        return 0;
    }
    let fatigue = 1_000_u64.saturating_sub(u64::from(attacker.fatigue) / 2);
    let morale = 750_u64 + u64::from(attacker.morale) / 4;
    let damage = u64::from(frontage) * 1_000 * fatigue * morale / 1_000_000;
    let damage = damage
        * u64::from(terrain.elevation_damage_factor_milli(
            battlefield,
            attacker.position,
            defender.position,
        ))
        / 1_000;
    let attack = attacker
        .stats()
        .map_or(1_000, |stats| u64::from(stats.melee_attack_milli));
    let resistance = defender.stats().map_or(1_000, |stats| {
        u64::from(stats.defense_milli) + u64::from(stats.armor_milli)
    });
    let damage = damage * attack / resistance;
    let damage = damage * u64::from(contact_factor_milli(attacker, defender)) / 1_000;
    let damage = damage * u64::from(charge_factor_milli(attacker, defender)) / 1_000;
    let damage = damage * u64::from(matchup_factor_milli(attacker, defender))
        / 1_000
        / u64::from(MELEE_CASUALTY_DIVISOR);
    let credit = credits
        .entry(attacker.id.clone())
        .or_default()
        .entry(defender.id.clone())
        .or_default();
    let total = damage + u64::from(credit.0);
    credit.0 = u16::try_from(total % 1_000).expect("fractional damage fits u16");
    u16::try_from((total / 1_000).min(u64::from(defender.soldiers)))
        .expect("casualties are bounded by soldiers")
}

pub(super) fn melee_casualties(
    attacker: &TacticalUnit,
    defender: &TacticalUnit,
    frontage: u16,
    terrain: TacticalTerrain,
    battlefield: FlatBattlefield,
) -> u16 {
    if attacker.state != TacticalUnitState::Formed || defender.soldiers == 0 || frontage == 0 {
        return 0;
    }
    let frontage = u32::from(frontage);
    let fatigue_factor = 1_000_u32.saturating_sub(u32::from(attacker.fatigue) / 2);
    let morale_factor = 750_u32.saturating_add(u32::from(attacker.morale) / 4);
    let effective_frontage = frontage
        .saturating_mul(fatigue_factor)
        .saturating_mul(morale_factor)
        / 1_000_000;
    let effective_frontage =
        terrain_adjusted_frontage(effective_frontage, attacker, defender, terrain, battlefield);
    let effective_frontage = stat_adjusted_frontage(effective_frontage, attacker, defender, false);
    let effective_frontage =
        effective_frontage.saturating_mul(contact_factor_milli(attacker, defender)) / 1_000;
    let effective_frontage =
        effective_frontage.saturating_mul(matchup_factor_milli(attacker, defender)) / 1_000;
    let losses = (effective_frontage / MELEE_CASUALTY_DIVISOR).max(1);
    u16::try_from(losses.min(u32::from(defender.soldiers))).unwrap()
}

pub(super) fn ranged_casualties(
    attacker: &TacticalUnit,
    defender: &TacticalUnit,
    terrain: TacticalTerrain,
    battlefield: FlatBattlefield,
) -> u16 {
    if attacker.state != TacticalUnitState::Formed || defender.soldiers == 0 {
        return 0;
    }
    let frontage = u32::from(attacker.frontage_slots());
    let fatigue_factor = 1_000_u32.saturating_sub(u32::from(attacker.fatigue) / 2);
    let morale_factor = 750_u32.saturating_add(u32::from(attacker.morale) / 4);
    let effective_frontage = frontage
        .saturating_mul(fatigue_factor)
        .saturating_mul(morale_factor)
        / 1_000_000;
    let effective_frontage =
        terrain_adjusted_frontage(effective_frontage, attacker, defender, terrain, battlefield)
            .saturating_mul(
                terrain.ranged_target_damage_factor_milli(battlefield, defender.position),
            )
            / COMBAT_FACTOR_BASE_MILLI;
    let effective_frontage = stat_adjusted_frontage(effective_frontage, attacker, defender, true);
    let losses = (effective_frontage / RANGED_CASUALTY_DIVISOR).max(1);
    u16::try_from(losses.min(u32::from(defender.soldiers))).unwrap()
}

fn charge_factor_milli(attacker: &TacticalUnit, defender: &TacticalUnit) -> u32 {
    if !matches!(attacker.charge, Some(crate::CavalryChargeState::Contact { run_up_mm }) if run_up_mm >= crate::charge::CHARGE_RUN_UP_MM)
        || attacker.engagement_target.as_deref() != Some(defender.id.as_str())
        || (defender
            .combat_profile()
            .is_some_and(|profile| profile.kind == crate::UnitKind::Spearmen)
            && defender.incoming_arc(attacker.position) == Some(crate::CombatArc::Front))
    {
        return 1_000;
    }
    1_000
        + attacker
            .stats()
            .map_or(0, |stats| u32::from(stats.charge_impact_milli))
}

// Only explicit combat profiles participate; historical kind metadata is visual.
fn matchup_factor_milli(attacker: &TacticalUnit, defender: &TacticalUnit) -> u32 {
    let (Some(attacker_profile), Some(defender_profile)) =
        (attacker.combat_profile(), defender.combat_profile())
    else {
        return 1_000;
    };
    match (attacker_profile.kind, defender_profile.kind) {
        (crate::UnitKind::Spearmen, crate::UnitKind::Knights)
            if attacker.incoming_arc(defender.position) == Some(crate::CombatArc::Front) =>
        {
            1_750
        }
        (crate::UnitKind::Knights, crate::UnitKind::Spearmen)
            if defender.incoming_arc(attacker.position) == Some(crate::CombatArc::Front) =>
        {
            600
        }
        (crate::UnitKind::Knights, crate::UnitKind::Levy | crate::UnitKind::Archers)
            if matches!(
                defender.incoming_arc(attacker.position),
                Some(crate::CombatArc::Flank | crate::CombatArc::Rear)
            ) =>
        {
            1_250
        }
        _ => 1_000,
    }
}

fn contact_factor_milli(attacker: &TacticalUnit, defender: &TacticalUnit) -> u32 {
    let bonus = match defender.incoming_arc(attacker.position) {
        Some(crate::CombatArc::Flank) => 250_u32,
        Some(crate::CombatArc::Rear) => 500_u32,
        Some(crate::CombatArc::Front) | None => return 1_000,
    };
    let resistance = defender
        .stats()
        .map_or(1_000, |stats| u32::from(stats.formation_resistance_milli));
    1_000 + (bonus * 1_000 / resistance).min(750)
}

pub(super) fn ranged_damage_milli(
    attacker: &TacticalUnit,
    defender: &TacticalUnit,
    terrain: TacticalTerrain,
    battlefield: FlatBattlefield,
) -> u32 {
    let fatigue = 1_000_u64.saturating_sub(u64::from(attacker.fatigue) / 2);
    let morale = 750_u64 + u64::from(attacker.morale) / 4;
    let frontage = u64::from(attacker.frontage_slots()) * 1_000 * fatigue * morale / 1_000_000;
    let frontage = frontage
        * u64::from(terrain.elevation_damage_factor_milli(
            battlefield,
            attacker.position,
            defender.position,
        ))
        / 1_000;
    let frontage = frontage
        * u64::from(terrain.ranged_target_damage_factor_milli(battlefield, defender.position))
        / 1_000;
    let attack = attacker
        .stats()
        .and_then(|stats| stats.missile)
        .map_or(1_000, |missile| u64::from(missile.damage_milli));
    let resistance = 1_000
        + defender
            .stats()
            .map_or(0, |stats| u64::from(stats.armor_milli));
    u32::try_from(frontage * attack / resistance / u64::from(RANGED_CASUALTY_DIVISOR))
        .expect("bounded frontage and stat factors fit u32 damage")
}

/// Damage of one staffed siege tower volley, in thousandths of a soldier.
/// It follows the stat-profile ranged rule with the tower as the shooter: the
/// profile's volley frontage at full fatigue and morale and base missile
/// damage, elevation from the tower to the target, the target's ground cover,
/// and the target's armor.
pub(super) fn tower_volley_damage_milli(
    profile: SiegeProfile,
    tower: BattlePoint,
    defender: &TacticalUnit,
    terrain: TacticalTerrain,
    battlefield: FlatBattlefield,
) -> u32 {
    let frontage = u64::from(profile.tower_volley_frontage()) * 1_000;
    let frontage = frontage
        * u64::from(terrain.elevation_damage_factor_milli(battlefield, tower, defender.position))
        / 1_000;
    let frontage = frontage
        * u64::from(terrain.ranged_target_damage_factor_milli(battlefield, defender.position))
        / 1_000;
    let resistance = 1_000
        + defender
            .stats()
            .map_or(0, |stats| u64::from(stats.armor_milli));
    u32::try_from(frontage * 1_000 / resistance / u64::from(RANGED_CASUALTY_DIVISOR))
        .expect("bounded tower frontage fits u32 damage")
}

/// Damage of one catapult stone hitting a unit, in thousandths of a soldier.
/// It reuses the tower volley rule with the crew position as the shooter:
/// [`CATAPULT_SHOT_FRONTAGE`] at base missile damage, elevation from the crew
/// to the target, the target's ground cover, and the target's armor.
pub(super) fn catapult_shot_damage_milli(
    crew: BattlePoint,
    defender: &TacticalUnit,
    terrain: TacticalTerrain,
    battlefield: FlatBattlefield,
) -> u32 {
    let frontage = u64::from(CATAPULT_SHOT_FRONTAGE) * 1_000;
    let frontage = frontage
        * u64::from(terrain.elevation_damage_factor_milli(battlefield, crew, defender.position))
        / 1_000;
    let frontage = frontage
        * u64::from(terrain.ranged_target_damage_factor_milli(battlefield, defender.position))
        / 1_000;
    let resistance = 1_000
        + defender
            .stats()
            .map_or(0, |stats| u64::from(stats.armor_milli));
    u32::try_from(frontage * 1_000 / resistance / u64::from(RANGED_CASUALTY_DIVISOR))
        .expect("bounded catapult frontage fits u32 damage")
}

fn stat_adjusted_frontage(
    frontage: u32,
    attacker: &TacticalUnit,
    defender: &TacticalUnit,
    ranged: bool,
) -> u32 {
    let attack = attacker.stats().map_or(1_000, |stats| {
        if ranged {
            stats
                .missile
                .map_or(1_000, |missile| u32::from(missile.damage_milli))
        } else {
            u32::from(stats.melee_attack_milli)
        }
    });
    let resistance = defender.stats().map_or(1_000, |stats| {
        (if ranged {
            1_000
        } else {
            u32::from(stats.defense_milli)
        }) + u32::from(stats.armor_milli)
    });
    frontage.saturating_mul(attack) / resistance
}

pub(super) fn pursuit_casualties(
    pursuer: &TacticalUnit,
    target: &TacticalUnit,
    terrain: TacticalTerrain,
    battlefield: FlatBattlefield,
) -> u16 {
    let frontage = u32::from(pursuer.frontage_slots());
    let fatigue_factor = 1_000_u32.saturating_sub(u32::from(pursuer.fatigue) / 2);
    let effective_frontage = frontage.saturating_mul(fatigue_factor) / 1_000;
    let effective_frontage =
        terrain_adjusted_frontage(effective_frontage, pursuer, target, terrain, battlefield);
    u16::try_from((effective_frontage / PURSUIT_CASUALTY_DIVISOR).max(1)).unwrap()
}

pub(super) fn casualty_morale_shock(before: u16, casualties: u16) -> u16 {
    if before == 0 || casualties == 0 {
        return 0;
    }
    let casualty_ratio_milli = u32::from(casualties) * 1_000 / u32::from(before);
    let shock = u32::from(casualties) * 8 + casualty_ratio_milli / 2;
    u16::try_from(shock.min(u32::from(MAX_TACTICAL_MORALE))).unwrap()
}
