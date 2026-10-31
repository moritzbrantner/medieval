//! Tick advancement, completion rules and order clean-up after each tick.

use super::error::TacticalError;
use super::{
    BattleSide, TACTICAL_TICKS_PER_SECOND, TacticalBattle, TacticalBattleState,
    TacticalFinishReason, TacticalUnit, TacticalUnitState, find_unit,
};
use crate::TacticalWorkCounters;
use crate::deployment::siege::{SiegeBattleState, SiegeGateState};
use crate::siege_engine::{SiegeEngineKind, SiegeEngineTarget};

impl TacticalBattle {
    pub fn advance_ticks(&mut self, ticks: u32) {
        self.advance_ticks_measured(ticks);
    }

    /// Advances the same authoritative rules while returning deterministic work.
    /// Counters are not retained or serialized in the battle.
    pub fn advance_ticks_measured(&mut self, ticks: u32) -> TacticalWorkCounters {
        let mut counters = TacticalWorkCounters::default();
        for _ in 0..ticks {
            if self.ensure_running().is_err() {
                break;
            }
            counters.ticks += 1;
            self.advance_movement_phase(&mut counters);
            self.tick = self.tick.saturating_add(1);
            if self
                .tick
                .is_multiple_of(u64::from(TACTICAL_TICKS_PER_SECOND))
            {
                self.resolve_combat_pulse(&mut counters);
                if let Some(siege) = &mut self.siege {
                    counters.unit_scan_visits += 2 * self.units.len() as u64;
                    let engines = &self.siege_engines;
                    let has_ram = |unit: &TacticalUnit| {
                        engines.iter().any(|engine| {
                            engine.kind == SiegeEngineKind::BatteringRam
                                && engine.crew_unit_id == unit.id
                        })
                    };
                    if siege.advance_gate_assault_with_rams(&self.units, has_ram) {
                        for unit in &mut self.units {
                            unit.gate_attack = false;
                        }
                    }
                    siege.advance_capture(&self.units);
                }
            }
            counters.unit_scan_visits += self.units.len() as u64;
            self.clear_invalid_engagement_targets(&mut counters);
            counters.unit_scan_visits += self.update_completion();
        }
        counters
    }

    pub(crate) fn ensure_running(&self) -> Result<(), TacticalError> {
        match self.state {
            TacticalBattleState::Running => Ok(()),
            TacticalBattleState::Deployment => Err(TacticalError::BattleNotStarted),
            TacticalBattleState::Finished { .. } => Err(TacticalError::BattleFinished),
        }
    }

    /// Returns the units visited by the completion scans.
    pub(super) fn update_completion(&mut self) -> u64 {
        let visits = std::cell::Cell::new(0_u64);
        if self.completion_rules.is_none() || self.ensure_running().is_err() {
            return 0;
        }
        let captured_by = self
            .siege
            .and_then(SiegeBattleState::final_objective_captured_by);
        let result = if let Some(winner) = captured_by {
            Some((Some(winner), TacticalFinishReason::SiegeCapture))
        } else if let Some(withdrawal) = self.withdrawal {
            let pending = |side| {
                self.units.iter().any(|unit| {
                    visits.set(visits.get() + 1);
                    unit.side == side && unit.is_withdrawing()
                })
            };
            match (withdrawal.attacker, withdrawal.defender) {
                (true, true)
                    if !pending(BattleSide::Attacker) && !pending(BattleSide::Defender) =>
                {
                    Some((None, TacticalFinishReason::MutualWithdrawal))
                }
                (true, false) if !pending(BattleSide::Attacker) => {
                    Some((Some(BattleSide::Defender), TacticalFinishReason::Withdrawal))
                }
                (false, true) if !pending(BattleSide::Defender) => {
                    Some((Some(BattleSide::Attacker), TacticalFinishReason::Withdrawal))
                }
                _ => None,
            }
        } else {
            let active = |side| {
                self.units.iter().any(|unit| {
                    visits.set(visits.get() + 1);
                    unit.side == side
                        && unit.soldiers > 0
                        && unit.state == TacticalUnitState::Formed
                })
            };
            match (active(BattleSide::Attacker), active(BattleSide::Defender)) {
                (true, true) => None,
                (true, false) => Some((
                    Some(BattleSide::Attacker),
                    TacticalFinishReason::ForceDefeated,
                )),
                (false, true) => Some((
                    Some(BattleSide::Defender),
                    TacticalFinishReason::ForceDefeated,
                )),
                (false, false) => Some((None, TacticalFinishReason::MutualDefeat)),
            }
        };
        if let Some((winner, reason)) = result {
            self.state = TacticalBattleState::Finished {
                winner,
                finishing_tick: self.tick,
                reason,
            };
        }
        visits.get()
    }

    fn clear_invalid_engagement_targets(&mut self, counters: &mut TacticalWorkCounters) {
        let units = &self.units;
        let invalid: Vec<usize> = (0..units.len())
            .filter(|&index| {
                let unit = &units[index];
                let Some(target_id) = unit.engagement_target.as_deref() else {
                    return false;
                };
                unit.state != TacticalUnitState::Formed
                    || find_unit(units, target_id, counters).is_some_and(|target| {
                        matches!(
                            target.state,
                            TacticalUnitState::Destroyed | TacticalUnitState::Escaped { .. }
                        )
                    })
            })
            .collect();
        for index in invalid {
            self.units[index].engagement_target = None;
        }
        let gate_closed = self
            .siege
            .is_some_and(|siege| siege.gate_state == SiegeGateState::Closed);
        for unit in &mut self.units {
            if unit.gate_attack && (!gate_closed || unit.state != TacticalUnitState::Formed) {
                unit.gate_attack = false;
            }
            if unit.state != TacticalUnitState::Formed {
                unit.ladder_order = None;
            }
        }
        // A catapult order ends with a routed or destroyed crew, a gate that
        // is no longer closed, or a target that is no longer formed.
        let units = &self.units;
        let invalid_fire: Vec<usize> = (0..units.len())
            .filter(|&index| {
                let unit = &units[index];
                match &unit.siege_engine_fire {
                    None => false,
                    Some(_) if unit.state != TacticalUnitState::Formed => true,
                    Some(SiegeEngineTarget::Gate) => !gate_closed,
                    Some(SiegeEngineTarget::Unit(target_id)) => {
                        find_unit(units, target_id, counters)
                            .is_none_or(|target| target.state != TacticalUnitState::Formed)
                    }
                }
            })
            .collect();
        for index in invalid_fire {
            self.units[index].siege_engine_fire = None;
        }
    }
}
