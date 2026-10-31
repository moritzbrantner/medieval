//! The combat pulse: engagement pairs, volleys, melee and casualty application.

use super::damage::{
    allocated_frontage, casualty_morale_shock, melee_casualties_with_credit, pursuit_casualties,
    ranged_casualties, ranged_damage_milli, tower_volley_damage_milli,
};
use super::geometry::measured_points_within_distance;
use super::{
    COMBAT_CONTACT_DISTANCE_MM, COMBAT_FATIGUE_PER_PULSE, MAX_TACTICAL_FATIGUE,
    PURSUIT_DISTANCE_MM, ROUT_MORALE_THRESHOLD, TacticalBattle, TacticalUnitState,
    find_work_target,
};
use crate::TacticalWorkCounters;
use std::collections::{BTreeMap, BTreeSet};

impl TacticalBattle {
    /// Resolves one pulse against the pulse-start unit state. Units are only
    /// read until casualties are applied (spent ammunition is applied after
    /// the volley and melee loops), so the pulse reads live units without a snapshot.
    pub(super) fn resolve_combat_pulse(&mut self, counters: &mut TacticalWorkCounters) {
        counters.combat_pulses += 1;
        let snapshot = &self.units;
        let mut pairs = BTreeSet::new();
        // Pair collection, the volley loop, and charge recovery each visit
        // every unit once.
        counters.unit_scan_visits += 3 * snapshot.len() as u64;
        for unit in snapshot {
            if unit.state != TacticalUnitState::Formed {
                continue;
            }
            let Some(target_id) = unit.engagement_target.as_deref() else {
                continue;
            };
            let Some(target) = find_work_target(snapshot, target_id, counters) else {
                continue;
            };
            if target.is_destroyed() || target.is_escaped() || target.side == unit.side {
                continue;
            }
            let pair = if unit.id < target.id {
                (unit.id.clone(), target.id.clone())
            } else {
                (target.id.clone(), unit.id.clone())
            };
            pairs.insert(pair);
        }

        counters.engagement_pairs += pairs.len() as u64;
        let formed_contacts = pairs
            .iter()
            .filter(|(left_id, right_id)| {
                let left = find_work_target(snapshot, left_id, counters).unwrap();
                let right = find_work_target(snapshot, right_id, counters).unwrap();
                left.state == TacticalUnitState::Formed
                    && right.state == TacticalUnitState::Formed
                    && measured_points_within_distance(
                        counters,
                        left.position,
                        right.position,
                        COMBAT_CONTACT_DISTANCE_MM,
                    )
                    && !self.wall_separates(left.position, right.position)
            })
            .fold(
                BTreeMap::<String, u16>::new(),
                |mut contacts, (left, right)| {
                    *contacts.entry(left.clone()).or_default() += 1;
                    *contacts.entry(right.clone()).or_default() += 1;
                    contacts
                },
            );

        let mut casualties: BTreeMap<String, u32> = BTreeMap::new();
        let mut engaged_units = BTreeSet::new();
        let mut consumed_charges = BTreeSet::new();

        let mut spent_ammunition = Vec::new();
        for attacker in snapshot {
            if attacker.state != TacticalUnitState::Formed
                || attacker.attack_range_mm() <= COMBAT_CONTACT_DISTANCE_MM
                || formed_contacts.contains_key(&attacker.id)
            {
                continue;
            }
            let Some(target_id) = attacker.engagement_target.as_deref() else {
                continue;
            };
            let Some(target) = find_work_target(snapshot, target_id, counters) else {
                continue;
            };
            if target.state != TacticalUnitState::Formed || target.side == attacker.side {
                continue;
            }
            if (measured_points_within_distance(
                counters,
                attacker.position,
                target.position,
                COMBAT_CONTACT_DISTANCE_MM,
            ) && !self.wall_separates(attacker.position, target.position))
                || !measured_points_within_distance(
                    counters,
                    attacker.position,
                    target.position,
                    attacker.attack_range_mm(),
                )
            {
                continue;
            }
            if attacker.ammunition.is_some() {
                spent_ammunition.push(attacker.id.clone());
            }
            counters.ranged_volleys += 1;
            let losses = if attacker.stats().is_some() || target.stats().is_some() {
                let damage = ranged_damage_milli(attacker, target, self.terrain, self.battlefield);
                let credit = self
                    .ranged_damage_credit
                    .entry(attacker.id.clone())
                    .or_default()
                    .entry(target.id.clone())
                    .or_default();
                let total = damage.saturating_add(u32::from(credit.0));
                credit.0 = u16::try_from(total % 1_000).expect("fractional damage fits u16");
                if damage > 0 {
                    engaged_units.insert(attacker.id.clone());
                }
                u16::try_from((total / 1_000).min(u32::from(target.soldiers)))
                    .expect("casualties are bounded by target soldiers")
            } else {
                ranged_casualties(attacker, target, self.terrain, self.battlefield)
            };
            if losses > 0 {
                *casualties.entry(target.id.clone()).or_default() += u32::from(losses);
                engaged_units.insert(attacker.id.clone());
            }
        }

        if let Some(siege) = self.siege {
            // One scan resolves the staffing and target of every tower.
            counters.unit_scan_visits += snapshot.len() as u64;
            for (tower_index, (_, target_index)) in
                siege.tower_targets(snapshot).into_iter().enumerate()
            {
                let Some(target) = target_index.map(|index| &snapshot[index]) else {
                    continue;
                };
                let tower = siege.layout.towers[tower_index];
                counters.ranged_volleys += 1;
                let damage = tower_volley_damage_milli(
                    siege.profile,
                    tower.center,
                    target,
                    self.terrain,
                    self.battlefield,
                );
                let credit = self
                    .tower_damage_credit
                    .entry(u8::try_from(tower_index).expect("four towers fit u8"))
                    .or_default()
                    .entry(target.id.clone())
                    .or_default();
                let total = damage.saturating_add(u32::from(credit.0));
                credit.0 = u16::try_from(total % 1_000).expect("fractional damage fits u16");
                let losses = (total / 1_000)
                    .min(u32::from(siege.profile.tower_max_casualties_per_pulse()))
                    .min(u32::from(target.soldiers));
                if losses > 0 {
                    *casualties.entry(target.id.clone()).or_default() += losses;
                }
            }
        }

        let engine_pulse = self.resolve_siege_engine_shots(counters, &mut casualties);

        let mut contacts_assigned = BTreeMap::<String, u16>::new();
        for (left_id, right_id) in pairs {
            let left = find_work_target(snapshot, &left_id, counters).unwrap();
            let right = find_work_target(snapshot, &right_id, counters).unwrap();
            match (left.state, right.state) {
                (TacticalUnitState::Formed, TacticalUnitState::Formed)
                    if measured_points_within_distance(
                        counters,
                        left.position,
                        right.position,
                        COMBAT_CONTACT_DISTANCE_MM,
                    ) && !self.wall_separates(left.position, right.position) =>
                {
                    counters.melee_contacts += 1;
                    let left_frontage = allocated_frontage(
                        left,
                        formed_contacts[&left.id],
                        contacts_assigned.entry(left.id.clone()).or_default(),
                    );
                    let right_frontage = allocated_frontage(
                        right,
                        formed_contacts[&right.id],
                        contacts_assigned.entry(right.id.clone()).or_default(),
                    );
                    let left_losses = melee_casualties_with_credit(
                        right,
                        left,
                        right_frontage,
                        self.terrain,
                        self.battlefield,
                        &mut self.melee_damage_credit,
                    );
                    let right_losses = melee_casualties_with_credit(
                        left,
                        right,
                        left_frontage,
                        self.terrain,
                        self.battlefield,
                        &mut self.melee_damage_credit,
                    );
                    *casualties.entry(left.id.clone()).or_default() += u32::from(left_losses);
                    *casualties.entry(right.id.clone()).or_default() += u32::from(right_losses);
                    if left_frontage > 0
                        && left.engagement_target.as_deref() == Some(right.id.as_str())
                    {
                        consumed_charges.insert(left.id.clone());
                    }
                    if right_frontage > 0
                        && right.engagement_target.as_deref() == Some(left.id.as_str())
                    {
                        consumed_charges.insert(right.id.clone());
                    }
                    engaged_units.insert(left.id.clone());
                    engaged_units.insert(right.id.clone());
                }
                (
                    TacticalUnitState::Formed,
                    TacticalUnitState::Routed | TacticalUnitState::Withdrawing { .. },
                ) if left.engagement_target.as_deref() == Some(right.id.as_str())
                    && measured_points_within_distance(
                        counters,
                        left.position,
                        right.position,
                        PURSUIT_DISTANCE_MM,
                    )
                    && !self.wall_separates(left.position, right.position) =>
                {
                    counters.pursuit_contacts += 1;
                    *casualties.entry(right.id.clone()).or_default() += u32::from(
                        pursuit_casualties(left, right, self.terrain, self.battlefield),
                    );
                    engaged_units.insert(left.id.clone());
                }
                (
                    TacticalUnitState::Routed | TacticalUnitState::Withdrawing { .. },
                    TacticalUnitState::Formed,
                ) if right.engagement_target.as_deref() == Some(left.id.as_str())
                    && measured_points_within_distance(
                        counters,
                        left.position,
                        right.position,
                        PURSUIT_DISTANCE_MM,
                    )
                    && !self.wall_separates(left.position, right.position) =>
                {
                    counters.pursuit_contacts += 1;
                    *casualties.entry(left.id.clone()).or_default() += u32::from(
                        pursuit_casualties(right, left, self.terrain, self.battlefield),
                    );
                    engaged_units.insert(right.id.clone());
                }
                _ => {}
            }
        }

        self.apply_siege_engine_pulse(engine_pulse);

        for attacker_id in spent_ammunition {
            if let Some(index) = self.unit_index(&attacker_id)
                && let Some(ammunition) = &mut self.units[index].ammunition
            {
                ammunition.0 = ammunition.0.saturating_sub(1);
            }
        }

        for unit_id in engaged_units {
            if let Some(index) = self.unit_index(&unit_id) {
                self.units[index].fatigue = self.units[index]
                    .fatigue
                    .saturating_add(COMBAT_FATIGUE_PER_PULSE)
                    .min(MAX_TACTICAL_FATIGUE);
            }
        }

        for (unit_id, requested_losses) in casualties {
            let Some(index) = self.unit_index(&unit_id) else {
                continue;
            };
            let before = self.units[index].soldiers;
            if before == 0 || self.units[index].state == TacticalUnitState::Destroyed {
                continue;
            }
            let applied = u16::try_from(requested_losses.min(u32::from(before))).unwrap();
            if self.units[index].is_pursuit_target() {
                self.units[index].pursuit_casualties =
                    self.units[index].pursuit_casualties.saturating_add(applied);
            }
            self.units[index].soldiers -= applied;
            if self.units[index].soldiers == 0 {
                self.units[index].morale = 0;
                self.units[index].state = TacticalUnitState::Destroyed;
                self.units[index].destination = None;
                self.units[index].arrival_facing = None;
                self.units[index].queued_movements.clear();
                self.units[index].movement_mode = crate::MovementMode::March;
                self.units[index].engagement_target = None;
                self.units[index].clear_siege_orders();
                continue;
            }

            if self.units[index].can_receive_orders() || self.units[index].is_withdrawing() {
                let shock = casualty_morale_shock(before, applied);
                self.units[index].morale = self.units[index].morale.saturating_sub(shock);
                if self.units[index].morale <= ROUT_MORALE_THRESHOLD {
                    self.units[index].state = if self.units[index].is_withdrawing() {
                        TacticalUnitState::Withdrawing { routed: true }
                    } else {
                        TacticalUnitState::Routed
                    };
                    self.units[index].destination = None;
                    self.units[index].arrival_facing = None;
                    self.units[index].queued_movements.clear();
                    self.units[index].movement_mode = crate::MovementMode::March;
                    self.units[index].engagement_target = None;
                    self.units[index].clear_siege_orders();
                }
            }
        }
        for unit in &mut self.units {
            if consumed_charges.contains(&unit.id)
                && matches!(unit.charge, Some(crate::CavalryChargeState::Contact { .. }))
            {
                unit.charge = Some(crate::CavalryChargeState::Recovering {
                    ticks_remaining: crate::charge::CHARGE_RECOVERY_TICKS,
                });
            }
        }
    }
}
