//! Catapult shots resolved during a combat pulse.

use super::damage::catapult_shot_damage_milli;
use super::geometry::{point_distance_squared, square_u32};
use super::{BattlePoint, TacticalBattle, TacticalUnitState, find_work_target};
use crate::TacticalWorkCounters;
use crate::deployment::siege::SiegeGateState;
use crate::siege_engine::{
    CATAPULT_MAX_RANGE_MM, CATAPULT_MIN_RANGE_MM, CATAPULT_RELOAD_PULSES, SiegeEngineKind,
    SiegeEngineTarget,
};
use std::collections::BTreeMap;

impl TacticalBattle {
    /// Resolves every catapult for this combat pulse against the pulse-start
    /// unit state, in setup order. A formed crew reloads one pulse; a loaded
    /// catapult with stones left fires at its ordered target if that target
    /// is still valid and in range. Unit hits add to `casualties` and flow
    /// through the shared casualty and morale path; gate hits are returned.
    pub(super) fn resolve_siege_engine_shots(
        &self,
        counters: &mut TacticalWorkCounters,
        casualties: &mut BTreeMap<String, u32>,
    ) -> SiegeEnginePulse {
        let mut pulse = SiegeEnginePulse::default();
        let Some(siege) = self.siege else {
            return pulse;
        };
        let snapshot = &self.units;
        for (engine_index, engine) in self.siege_engines.iter().enumerate() {
            if engine.kind != SiegeEngineKind::Catapult {
                continue;
            }
            let Some(crew) = find_work_target(snapshot, &engine.crew_unit_id, counters) else {
                continue;
            };
            if crew.state != TacticalUnitState::Formed {
                continue;
            }
            let pulses_until_loaded = engine.pulses_until_loaded.saturating_sub(1);
            let fired = pulses_until_loaded == 0
                && engine.ammunition.unwrap_or(0) > 0
                && match &crew.siege_engine_fire {
                    None => false,
                    Some(SiegeEngineTarget::Gate) => {
                        let hit = siege.gate_state == SiegeGateState::Closed
                            && within_catapult_range(crew.position, siege.layout.gate.center());
                        if hit {
                            pulse.gate_damage = pulse
                                .gate_damage
                                .saturating_add(siege.profile.catapult_gate_damage_per_shot());
                        }
                        hit
                    }
                    Some(SiegeEngineTarget::Unit(target_id)) => {
                        let target =
                            find_work_target(snapshot, target_id, counters).filter(|target| {
                                target.state == TacticalUnitState::Formed
                                    && target.side != crew.side
                                    && within_catapult_range(crew.position, target.position)
                            });
                        if let Some(target) = target {
                            let damage = catapult_shot_damage_milli(
                                crew.position,
                                target,
                                self.terrain,
                                self.battlefield,
                            );
                            let losses = (damage / 1_000).max(1).min(u32::from(target.soldiers));
                            *casualties.entry(target.id.clone()).or_default() += losses;
                        }
                        target.is_some()
                    }
                };
            if fired {
                counters.ranged_volleys += 1;
            }
            pulse
                .engines
                .push((engine_index, pulses_until_loaded, fired));
        }
        pulse
    }

    /// Applies the reload, spent stones and gate damage of one engine pulse.
    pub(super) fn apply_siege_engine_pulse(&mut self, pulse: SiegeEnginePulse) {
        for (engine_index, pulses_until_loaded, fired) in pulse.engines {
            let engine = &mut self.siege_engines[engine_index];
            engine.pulses_until_loaded = pulses_until_loaded;
            if fired {
                engine.pulses_until_loaded = CATAPULT_RELOAD_PULSES;
                engine.ammunition = engine.ammunition.map(|stones| stones.saturating_sub(1));
                engine.last_shot_tick = Some(self.tick);
            }
        }
        if let Some(siege) = &mut self.siege
            && siege.damage_gate(pulse.gate_damage)
        {
            for unit in &mut self.units {
                unit.gate_attack = false;
            }
        }
    }
}

/// Engine effects of one combat pulse: per catapult (setup index, new reload
/// count, whether it fired) and the summed gate damage.
#[derive(Default)]
pub(super) struct SiegeEnginePulse {
    engines: Vec<(usize, u16, bool)>,
    gate_damage: u16,
}

/// Whether `target` lies within the catapult's inclusive Euclidean range of
/// `from`.
pub(super) fn within_catapult_range(from: BattlePoint, target: BattlePoint) -> bool {
    let distance = point_distance_squared(from, target);
    square_u32(CATAPULT_MIN_RANGE_MM) <= distance && distance <= square_u32(CATAPULT_MAX_RANGE_MM)
}
