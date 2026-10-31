//! Siege gate, ram, catapult and ladder orders.

use super::error::TacticalError;
use super::siege_engines::within_catapult_range;
use super::{BattlePoint, BattleSide, TacticalBattle};
use crate::deployment::siege::SiegeGateState;
use crate::siege_engine::{SiegeEngineKind, SiegeEngineTarget};

impl TacticalBattle {
    pub fn open_siege_gate(&mut self) -> Result<(), TacticalError> {
        self.set_siege_gate_state(SiegeGateState::Open)
    }

    pub fn close_siege_gate(&mut self) -> Result<(), TacticalError> {
        self.set_siege_gate_state(SiegeGateState::Closed)
    }

    pub fn destroy_siege_gate(&mut self) -> Result<(), TacticalError> {
        self.set_siege_gate_state(SiegeGateState::Destroyed)
    }

    /// Orders a formed attacker unit to assault the closed siege gate. The
    /// unit approaches its own side of the gate like an engagement order and
    /// strikes it on each combat pulse while within reach; the core alone
    /// applies the damage and the destruction. A layout has exactly one gate.
    pub fn issue_gate_attack_order(&mut self, unit_id: &str) -> Result<(), TacticalError> {
        self.ensure_running()?;
        let Some(siege) = self.siege else {
            return Err(TacticalError::NotSiegeBattle);
        };
        let index = self
            .unit_index(unit_id)
            .ok_or_else(|| TacticalError::UnitNotFound(unit_id.to_owned()))?;
        self.ensure_can_receive_orders(index)?;
        if self.units[index].side != BattleSide::Attacker {
            return Err(TacticalError::FriendlyGateAttack(unit_id.to_owned()));
        }
        if siege.gate_state != SiegeGateState::Closed {
            return Err(TacticalError::SiegeGateNotClosed);
        }
        let unit = &mut self.units[index];
        if !unit.gate_attack {
            unit.interrupt_charge();
        }
        unit.destination = None;
        unit.queued_movements.clear();
        unit.movement_mode = crate::MovementMode::March;
        unit.arrival_facing = None;
        unit.engagement_target = None;
        unit.ladder_order = None;
        unit.siege_engine_fire = None;
        unit.gate_attack = true;
        Ok(())
    }

    /// Orders the crew of a battering ram to assault the closed gate with it.
    /// Legality is the gate-attack order's plus the ram: only the formed
    /// crew of a ram may use it ([`TacticalError::SiegeEngineUnavailable`]
    /// otherwise). The crew approaches from any distance and, on each combat
    /// pulse within reach, deals [`SiegeProfile::ram_damage_per_pulse`]
    /// instead of the unit gate damage. A rejected order changes nothing.
    pub fn issue_ram_order(&mut self, unit_id: &str) -> Result<(), TacticalError> {
        self.ensure_running()?;
        if self.siege.is_none() {
            return Err(TacticalError::NotSiegeBattle);
        }
        let index = self
            .unit_index(unit_id)
            .ok_or_else(|| TacticalError::UnitNotFound(unit_id.to_owned()))?;
        self.ensure_can_receive_orders(index)?;
        if !self.has_siege_engine(unit_id, SiegeEngineKind::BatteringRam) {
            return Err(TacticalError::SiegeEngineUnavailable(unit_id.to_owned()));
        }
        self.issue_gate_attack_order(unit_id)
    }

    /// Orders the crew of a loaded-or-reloading catapult with stones left to
    /// hold in place and fire at `target`: the closed gate (aimed at its
    /// center) or a formed enemy unit, within
    /// `CATAPULT_MIN_RANGE_MM..=CATAPULT_MAX_RANGE_MM` of the crew (Euclidean,
    /// inclusive; walls do not block the indirect shot). The engine fires on
    /// every combat pulse it is loaded and the target is still valid and in
    /// range. A rejected order changes nothing.
    pub fn issue_siege_engine_fire_order(
        &mut self,
        unit_id: &str,
        target: SiegeEngineTarget,
    ) -> Result<(), TacticalError> {
        self.ensure_running()?;
        let Some(siege) = self.siege else {
            return Err(TacticalError::NotSiegeBattle);
        };
        let index = self
            .unit_index(unit_id)
            .ok_or_else(|| TacticalError::UnitNotFound(unit_id.to_owned()))?;
        self.ensure_can_receive_orders(index)?;
        if !self.siege_engine_of(unit_id).is_some_and(|engine| {
            engine.kind == SiegeEngineKind::Catapult && engine.ammunition.unwrap_or(0) > 0
        }) {
            return Err(TacticalError::SiegeEngineUnavailable(unit_id.to_owned()));
        }
        let crew = &self.units[index];
        let aim = match &target {
            SiegeEngineTarget::Gate => {
                if siege.gate_state != SiegeGateState::Closed {
                    return Err(TacticalError::SiegeGateNotClosed);
                }
                siege.layout.gate.center()
            }
            SiegeEngineTarget::Unit(target_id) => {
                let target_unit = self
                    .unit_index(target_id)
                    .map(|target_index| &self.units[target_index])
                    .ok_or_else(|| TacticalError::UnitNotFound(target_id.clone()))?;
                if target_unit.side == crew.side || !target_unit.can_receive_orders() {
                    return Err(TacticalError::InvalidSiegeEngineTarget {
                        unit_id: unit_id.to_owned(),
                        target_unit_id: target_id.clone(),
                    });
                }
                target_unit.position
            }
        };
        if !within_catapult_range(crew.position, aim) {
            return Err(TacticalError::SiegeEngineTargetOutOfRange(
                unit_id.to_owned(),
            ));
        }
        let unit = &mut self.units[index];
        if unit.siege_engine_fire.is_none() {
            unit.interrupt_charge();
        }
        unit.destination = None;
        unit.queued_movements.clear();
        unit.movement_mode = crate::MovementMode::March;
        unit.arrival_facing = None;
        unit.engagement_target = None;
        unit.gate_attack = false;
        unit.ladder_order = None;
        unit.siege_engine_fire = Some(target);
        Ok(())
    }

    /// Orders a formed attacker foot unit to place its one scaling ladder at
    /// `at`, a point inside a wall segment. Like the gate-attack order the
    /// unit approaches from any distance, to the foot of the ladder on its own
    /// side of the wall; the ladder exists once the unit stands there. A
    /// rejected order changes nothing.
    pub fn issue_ladder_order(
        &mut self,
        unit_id: &str,
        at: BattlePoint,
    ) -> Result<(), TacticalError> {
        self.ensure_running()?;
        let Some(siege) = self.siege else {
            return Err(TacticalError::NotSiegeBattle);
        };
        let index = self
            .unit_index(unit_id)
            .ok_or_else(|| TacticalError::UnitNotFound(unit_id.to_owned()))?;
        self.ensure_can_receive_orders(index)?;
        let unit = &self.units[index];
        if !self.climbs_ladders(unit)
            || !siege.supports_ladders()
            || self
                .siege_ladders
                .iter()
                .any(|ladder| ladder.placed_by == unit.id)
        {
            return Err(TacticalError::LadderUnavailable(unit_id.to_owned()));
        }
        let invalid = || TacticalError::InvalidLadderPosition {
            unit_id: unit_id.to_owned(),
            position: at,
        };
        let area = siege.ladder_area(at).ok_or_else(invalid)?;
        let foot = crate::deployment::siege::ladder_foot(area, unit.position);
        if !self.battlefield.contains(foot) || !self.is_passable_at(foot) {
            return Err(invalid());
        }
        let unit = &mut self.units[index];
        if unit.ladder_order.is_none() {
            unit.interrupt_charge();
        }
        unit.destination = None;
        unit.queued_movements.clear();
        unit.movement_mode = crate::MovementMode::March;
        unit.arrival_facing = None;
        unit.engagement_target = None;
        unit.gate_attack = false;
        unit.siege_engine_fire = None;
        unit.ladder_order = Some(at);
        Ok(())
    }

    fn set_siege_gate_state(&mut self, gate_state: SiegeGateState) -> Result<(), TacticalError> {
        self.ensure_running()?;
        let Some(siege) = &mut self.siege else {
            return Err(TacticalError::NotSiegeBattle);
        };
        if siege.gate_state == SiegeGateState::Destroyed {
            return Err(TacticalError::SiegeGateDestroyed);
        }
        if gate_state == SiegeGateState::Closed
            && self.units.iter().any(|unit| {
                unit.side == BattleSide::Attacker
                    && unit.is_withdrawing()
                    && unit.position.x_mm >= siege.layout.gate.min_x_mm
            })
        {
            return Err(TacticalError::SiegeExitInUse);
        }
        siege.gate_state = gate_state;
        if gate_state == SiegeGateState::Destroyed {
            siege.gate_integrity = 0;
        }
        if gate_state != SiegeGateState::Closed {
            for unit in &mut self.units {
                unit.gate_attack = false;
                if unit.siege_engine_fire == Some(SiegeEngineTarget::Gate) {
                    unit.siege_engine_fire = None;
                }
            }
        }
        Ok(())
    }
}
