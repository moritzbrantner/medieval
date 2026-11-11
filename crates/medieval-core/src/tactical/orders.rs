//! Withdrawal, movement, facing, engagement and formation orders.

use super::error::TacticalError;
use super::{
    BattleSide, DeploymentEdit, Formation, MovementOrder, TacticalBattle, TacticalBattleState,
    TacticalUnitState, WithdrawalSides,
};

impl TacticalBattle {
    /// Commits a side to retreat toward its entry edge. Arrival marks a whole
    /// unit escaped; until then existing pursuit rules can inflict casualties.
    /// Fortress defenders have no exit in the prototype layout. Attackers
    /// inside its wall need a traversable gate. Repeated intents are idempotent.
    pub fn withdraw(&mut self, side: BattleSide) -> Result<(), TacticalError> {
        self.validate_withdrawal(side)?;
        if self.state == TacticalBattleState::Deployment {
            self.withdraw_before_start(side);
            return Ok(());
        }
        if self
            .withdrawal
            .is_some_and(|withdrawal| withdrawal.contains(side))
        {
            return Ok(());
        }
        let withdrawal = self.withdrawal.get_or_insert_with(WithdrawalSides::default);
        match side {
            BattleSide::Attacker => withdrawal.attacker = true,
            BattleSide::Defender => withdrawal.defender = true,
        }
        for unit in self
            .units
            .iter_mut()
            .filter(|unit| unit.side == side && !unit.is_destroyed() && !unit.is_escaped())
        {
            unit.state = TacticalUnitState::Withdrawing {
                routed: unit.is_routed(),
            };
            unit.engagement_target = None;
            unit.clear_siege_orders();
            unit.arrival_facing = None;
            unit.queued_movements.clear();
            unit.movement_mode = crate::MovementMode::March;
            unit.destination = None;
        }
        self.update_completion();
        Ok(())
    }

    #[must_use]
    pub fn can_withdraw(&self, side: BattleSide) -> bool {
        !self
            .withdrawal
            .is_some_and(|withdrawal| withdrawal.contains(side))
            && self.validate_withdrawal(side).is_ok()
    }

    fn validate_withdrawal(&self, side: BattleSide) -> Result<(), TacticalError> {
        // Withdrawing is the one order a side may give before the start.
        if self.state != TacticalBattleState::Deployment {
            self.ensure_running()?;
        }
        if self.completion_rules.is_none() {
            return Err(TacticalError::WithdrawalRequiresStartedBattle);
        }
        if self
            .withdrawal
            .is_some_and(|withdrawal| withdrawal.contains(side))
        {
            return Ok(());
        }
        let survivors = || {
            self.units
                .iter()
                .filter(|unit| unit.side == side && !unit.is_destroyed() && !unit.is_escaped())
        };
        if survivors().next().is_none() {
            return Err(TacticalError::WithdrawalNoSurvivors(side));
        }
        if let Some(siege) = self.siege
            && (side == BattleSide::Defender
                || (!siege.gate_state.is_traversable()
                    && survivors().any(|unit| unit.position.x_mm >= siege.layout.gate.min_x_mm)))
        {
            return Err(TacticalError::WithdrawalBlockedBySiege(side));
        }
        Ok(())
    }

    pub(crate) fn issue_unit_waypoint(
        &mut self,
        unit_id: &str,
        waypoint: crate::MovementWaypoint,
        queued: bool,
    ) -> Result<(), TacticalError> {
        self.ensure_running()?;
        let index = self
            .unit_index(unit_id)
            .ok_or_else(|| TacticalError::UnitNotFound(unit_id.to_owned()))?;
        self.ensure_can_receive_orders(index)?;
        if queued
            && (self.units[index].destination.is_some()
                || self.units[index].engagement_target.is_some()
                || !self.units[index].queued_movements.is_empty())
        {
            if self.units[index].queued_movements.len() >= crate::MAX_QUEUED_WAYPOINTS {
                return Err(TacticalError::WaypointLimitReached(unit_id.to_owned()));
            }
            self.units[index].queued_movements.push(waypoint);
        } else {
            self.issue_move_order(MovementOrder {
                unit_id: unit_id.to_owned(),
                destination: waypoint.destination,
            })?;
            self.units[index].movement_mode = waypoint.mode;
        }
        Ok(())
    }

    pub fn issue_move_order(&mut self, order: MovementOrder) -> Result<(), TacticalError> {
        self.ensure_running()?;
        let unit_index = self
            .unit_index(&order.unit_id)
            .ok_or_else(|| TacticalError::UnitNotFound(order.unit_id.clone()))?;
        self.ensure_can_receive_orders(unit_index)?;

        if !self.battlefield.contains(order.destination) {
            return Err(TacticalError::DestinationOutOfBounds {
                unit_id: order.unit_id,
                destination: order.destination,
            });
        }
        if !self.is_passable_at(order.destination) {
            return Err(TacticalError::DestinationImpassable {
                unit_id: order.unit_id,
                destination: order.destination,
            });
        }

        let unit = &mut self.units[unit_index];
        unit.interrupt_charge();
        unit.engagement_target = None;
        unit.clear_siege_orders();
        unit.queued_movements.clear();
        unit.movement_mode = crate::MovementMode::March;
        unit.arrival_facing = None;
        unit.destination = (unit.position != order.destination).then_some(order.destination);
        Ok(())
    }

    pub(crate) fn set_arrival_facing(
        &mut self,
        unit_id: &str,
        facing: crate::Facing,
    ) -> Result<(), TacticalError> {
        let index = self
            .unit_index(unit_id)
            .ok_or_else(|| TacticalError::UnitNotFound(unit_id.to_owned()))?;
        if self.units[index].destination.is_some() {
            self.units[index].arrival_facing = Some(facing);
        } else {
            self.units[index].facing = Some(facing);
        }
        Ok(())
    }

    pub fn issue_facing_order(
        &mut self,
        unit_id: &str,
        facing: crate::Facing,
    ) -> Result<(), TacticalError> {
        self.ensure_running()?;
        let index = self
            .unit_index(unit_id)
            .ok_or_else(|| TacticalError::UnitNotFound(unit_id.to_owned()))?;
        self.ensure_can_receive_orders(index)?;
        if self.units[index].facing != Some(facing) {
            self.units[index].interrupt_charge();
        }
        self.units[index].facing = Some(facing);
        Ok(())
    }

    pub fn issue_engagement_order(
        &mut self,
        unit_id: &str,
        target_unit_id: &str,
    ) -> Result<(), TacticalError> {
        self.ensure_running()?;
        let unit_index = self
            .unit_index(unit_id)
            .ok_or_else(|| TacticalError::UnitNotFound(unit_id.to_owned()))?;
        self.ensure_can_receive_orders(unit_index)?;

        let target_index = self
            .unit_index(target_unit_id)
            .ok_or_else(|| TacticalError::UnitNotFound(target_unit_id.to_owned()))?;
        if self.units[unit_index].side == self.units[target_index].side {
            return Err(TacticalError::FriendlyEngagement {
                unit_id: unit_id.to_owned(),
                target_unit_id: target_unit_id.to_owned(),
            });
        }
        if self.units[target_index].is_escaped() {
            return Err(TacticalError::TargetEscaped(target_unit_id.to_owned()));
        }
        if self.units[target_index].state == TacticalUnitState::Destroyed {
            return Err(TacticalError::TargetDestroyed(target_unit_id.to_owned()));
        }

        let unit = &mut self.units[unit_index];
        if unit.engagement_target.as_deref() != Some(target_unit_id) {
            unit.interrupt_charge();
        }
        unit.destination = None;
        unit.queued_movements.clear();
        unit.movement_mode = crate::MovementMode::March;
        unit.arrival_facing = None;
        unit.clear_siege_orders();
        unit.engagement_target = Some(target_unit_id.to_owned());
        Ok(())
    }

    pub fn issue_formation_order(
        &mut self,
        unit_id: &str,
        formation: Formation,
    ) -> Result<(), TacticalError> {
        if self.state == TacticalBattleState::Deployment {
            if formation.files() == 0 {
                return Err(TacticalError::InvalidFormation(unit_id.to_owned()));
            }
            return self.edit_deployed_formation(
                unit_id,
                DeploymentEdit {
                    formation: Some(formation),
                    ..DeploymentEdit::default()
                },
            );
        }
        self.ensure_running()?;
        let unit_index = self
            .unit_index(unit_id)
            .ok_or_else(|| TacticalError::UnitNotFound(unit_id.to_owned()))?;
        self.ensure_can_receive_orders(unit_index)?;
        if formation.files() == 0 {
            return Err(TacticalError::InvalidFormation(unit_id.to_owned()));
        }
        // Validate pending placements with the new formation in place and
        // restore it on rejection, instead of copying the whole battle.
        let previous = std::mem::replace(&mut self.units[unit_index].formation, formation);
        let unit = &self.units[unit_index];
        if let Err(error) = unit
            .destination
            .into_iter()
            .chain(
                unit.queued_movements
                    .iter()
                    .map(|waypoint| waypoint.destination),
            )
            .try_for_each(|destination| self.validate_formation_placement(unit_id, destination))
        {
            self.units[unit_index].formation = previous;
            return Err(error);
        }
        if previous != formation {
            self.units[unit_index].interrupt_charge();
        }
        Ok(())
    }

    pub(super) fn unit_index(&self, unit_id: &str) -> Option<usize> {
        self.units
            .binary_search_by(|unit| unit.id.as_str().cmp(unit_id))
            .ok()
    }

    pub(super) fn ensure_can_receive_orders(&self, unit_index: usize) -> Result<(), TacticalError> {
        let unit = &self.units[unit_index];
        if unit.state == TacticalUnitState::Formed {
            Ok(())
        } else {
            Err(TacticalError::UnitCannotReceiveOrders {
                unit_id: unit.id.clone(),
            })
        }
    }
}
