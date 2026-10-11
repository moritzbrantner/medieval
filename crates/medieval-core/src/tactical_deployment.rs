//! The pre-battle deployment phase (issue #94).
//!
//! Owner decision 2026-10-11, option A: a formation is legally deployed only
//! when its whole [`FormationFootprint`] lies inside its own side's deployment
//! zone and does not overlap another formation of the same side. Every
//! deployment edit (placement, rotation, frontage and formation changes) is
//! validated here, and a rejected edit changes nothing.

use serde::{Deserialize, Serialize};

use super::{
    BattlePoint, BattleSide, Formation, TacticalBattle, TacticalBattleState,
    TacticalCompletionRules, TacticalError, TacticalUnitState, WithdrawalSides,
};
use crate::{Facing, FormationFootprint};

/// Why the authoritative battle refused a deployment edit.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DeploymentRejection {
    /// Part of the footprint lies outside the side's own deployment zone.
    OutsideZone,
    /// The footprint overlaps (or touches) another friendly formation.
    FriendlyOverlap,
}

/// One in-place change to a deployed formation; unset fields stay unchanged.
#[derive(Copy, Clone, Debug, Default)]
pub(crate) struct DeploymentEdit {
    pub(crate) position: Option<BattlePoint>,
    pub(crate) facing: Option<Facing>,
    pub(crate) formation: Option<Formation>,
}

impl TacticalBattle {
    /// Opens the deployment phase of a fresh battle (tick 0, nothing
    /// withdrawn). Completion rules are attached, as by [`Self::start`], but
    /// only take effect once [`Self::begin_battle`] runs. Standing orders are
    /// cleared: deployment starts from formations at rest.
    ///
    /// The initial layout of both sides must already satisfy the placement
    /// rule; otherwise the first offending formation is reported, zone
    /// violations before overlaps.
    pub fn into_deployment(mut self) -> Result<Self, TacticalError> {
        match self.state {
            TacticalBattleState::Finished { .. } => return Err(TacticalError::BattleFinished),
            TacticalBattleState::Running if self.tick != 0 || self.withdrawal.is_some() => {
                return Err(TacticalError::DeploymentClosed);
            }
            TacticalBattleState::Running | TacticalBattleState::Deployment => {}
        }
        let all: Vec<usize> = (0..self.units.len()).collect();
        self.check_deployment(&all)?;
        for unit in &mut self.units {
            unit.destination = None;
            unit.queued_movements.clear();
            unit.movement_mode = crate::MovementMode::March;
            unit.engagement_target = None;
            unit.arrival_facing = None;
            unit.clear_siege_orders();
        }
        self.completion_rules = Some(TacticalCompletionRules::FieldAndSiegeV1);
        self.state = TacticalBattleState::Deployment;
        Ok(self)
    }

    /// Places a formation instantly at `position`: no march order, no tick.
    /// Either side may be placed; which side a player controls is decided by
    /// the controls, not the core.
    pub fn deploy_formation(
        &mut self,
        unit_id: &str,
        position: BattlePoint,
    ) -> Result<(), TacticalError> {
        self.deploy_formations(&[(unit_id, position)])
    }

    /// Places several formations at once and validates the resulting layout,
    /// so a group may shift over its own previous ground. Atomic: either
    /// every placement is applied or none is. Zone violations are reported
    /// before overlaps, each in placement order.
    pub fn deploy_formations(
        &mut self,
        placements: &[(&str, BattlePoint)],
    ) -> Result<(), TacticalError> {
        self.ensure_deploying()?;
        let mut candidate = self.clone();
        let mut placed = Vec::with_capacity(placements.len());
        for &(unit_id, position) in placements {
            let index = candidate
                .unit_index(unit_id)
                .ok_or_else(|| TacticalError::UnitNotFound(unit_id.to_owned()))?;
            candidate.ensure_can_receive_orders(index)?;
            if placed.contains(&index) {
                return Err(TacticalError::InvalidMovementGroup);
            }
            candidate.units[index].position = position;
            placed.push(index);
        }
        candidate.check_deployment(&placed)?;
        *self = candidate;
        Ok(())
    }

    /// Leaves deployment exactly once. The deployed layout becomes the tick-0
    /// layout, completion rules become active and ticks run from now on. A
    /// second call is refused with [`TacticalError::DeploymentClosed`].
    pub fn begin_battle(&mut self) -> Result<(), TacticalError> {
        self.ensure_deploying()?;
        self.completion_rules
            .get_or_insert(TacticalCompletionRules::FieldAndSiegeV1);
        self.state = TacticalBattleState::Running;
        self.update_completion();
        Ok(())
    }

    /// Applies one in-place edit to a deployed formation under the rule;
    /// a rejected edit restores the formation exactly.
    pub(crate) fn edit_deployed_formation(
        &mut self,
        unit_id: &str,
        edit: DeploymentEdit,
    ) -> Result<(), TacticalError> {
        self.ensure_deploying()?;
        let index = self
            .unit_index(unit_id)
            .ok_or_else(|| TacticalError::UnitNotFound(unit_id.to_owned()))?;
        self.ensure_can_receive_orders(index)?;
        let previous = self.units[index].clone();
        let unit = &mut self.units[index];
        if let Some(position) = edit.position {
            unit.position = position;
        }
        if let Some(facing) = edit.facing {
            unit.facing = Some(facing);
        }
        if let Some(formation) = edit.formation {
            unit.formation = formation;
        }
        if let Err(error) = self.check_deployment(&[index]) {
            self.units[index] = previous;
            return Err(error);
        }
        Ok(())
    }

    /// A side that withdraws before the start refuses battle: it leaves the
    /// field at once, every surviving formation escapes at full strength, and
    /// the battle finishes at tick 0 (see [`Self::withdraw`]).
    pub(super) fn withdraw_before_start(&mut self, side: BattleSide) {
        self.completion_rules
            .get_or_insert(TacticalCompletionRules::FieldAndSiegeV1);
        self.state = TacticalBattleState::Running;
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
            unit.state = TacticalUnitState::Escaped {
                routed: unit.is_routed(),
            };
        }
        self.update_completion();
    }

    fn ensure_deploying(&self) -> Result<(), TacticalError> {
        match self.state {
            TacticalBattleState::Deployment => Ok(()),
            TacticalBattleState::Running => Err(TacticalError::DeploymentClosed),
            TacticalBattleState::Finished { .. } => Err(TacticalError::BattleFinished),
        }
    }

    /// The placement rule for the formations at `indices`, checked against
    /// the whole current layout: every zone first, then every overlap, then
    /// the anchor's own ground (which construction already requires).
    fn check_deployment(&self, indices: &[usize]) -> Result<(), TacticalError> {
        let zones = self.deployment_zones();
        let rejected = |index: usize, reason| TacticalError::InvalidDeploymentPlacement {
            unit_id: self.units[index].id.clone(),
            reason,
        };
        for &index in indices {
            let unit = &self.units[index];
            let zone = zones
                .into_iter()
                .find(|zone| zone.side == unit.side)
                .expect("both deployment sides have zones");
            if !unit.formation_footprint().is_inside(zone) {
                return Err(rejected(index, DeploymentRejection::OutsideZone));
            }
        }
        for &index in indices {
            let unit = &self.units[index];
            let footprint: FormationFootprint = unit.formation_footprint();
            let overlapping = self.units.iter().enumerate().any(|(other_index, other)| {
                other_index != index
                    && other.side == unit.side
                    && !other.is_destroyed()
                    && !other.is_escaped()
                    && footprint.overlaps(other.formation_footprint())
            });
            if overlapping {
                return Err(rejected(index, DeploymentRejection::FriendlyOverlap));
            }
        }
        for &index in indices {
            let unit = &self.units[index];
            if !self.terrain.is_passable_at(self.battlefield, unit.position) {
                return Err(TacticalError::UnitOnImpassableTerrain {
                    unit_id: unit.id.clone(),
                    position: unit.position,
                });
            }
            if self
                .siege
                .is_some_and(|siege| !siege.is_passable_at(unit.position))
            {
                return Err(TacticalError::UnitOnImpassableSiegeStructure {
                    unit_id: unit.id.clone(),
                    position: unit.position,
                });
            }
        }
        Ok(())
    }
}
