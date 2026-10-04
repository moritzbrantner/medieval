use serde::{Deserialize, Serialize};

use crate::{
    BattlePoint, BattleSide, DeploymentZone, Facing, Formation, FormationFootprint, MovementOrder,
    TacticalBattle, TacticalError,
};

/// Semantic formation commands. A batch either commits every command or none.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum FormationOrder {
    Rotate {
        unit_id: String,
        facing: Facing,
    },
    Turn {
        unit_id: String,
        quarter_turns: i8,
    },
    MoveAndFace {
        unit_id: String,
        destination: BattlePoint,
        facing: Option<Facing>,
    },
    Frontage {
        unit_id: String,
        width_mm: u32,
    },
}

impl TacticalBattle {
    pub fn issue_formation_orders(
        &mut self,
        orders: &[FormationOrder],
    ) -> Result<(), TacticalError> {
        self.ensure_running()?;
        let mut candidate = self.clone();
        for order in orders {
            candidate.apply_formation_order(order)?;
        }
        *self = candidate;
        Ok(())
    }

    fn apply_formation_order(&mut self, order: &FormationOrder) -> Result<(), TacticalError> {
        match order {
            FormationOrder::Rotate { unit_id, facing } => self.rotate_formation(unit_id, *facing),
            FormationOrder::Turn {
                unit_id,
                quarter_turns,
            } => {
                let mut direction = self.order_facing(unit_id)?.direction();
                for _ in 0..quarter_turns.rem_euclid(4) {
                    direction = [-direction[1], direction[0]];
                }
                self.rotate_formation(
                    unit_id,
                    Facing::new(direction[0], direction[1])
                        .expect("quarter turns preserve a valid direction"),
                )
            }
            FormationOrder::MoveAndFace {
                unit_id,
                destination,
                facing,
            } => {
                self.validate_formation_placement(unit_id, *destination)?;
                let facing = match facing {
                    Some(facing) => *facing,
                    None => self.order_facing(unit_id)?,
                };
                self.issue_move_order(MovementOrder {
                    unit_id: unit_id.clone(),
                    destination: *destination,
                })?;
                self.set_arrival_facing(unit_id, facing)
            }
            FormationOrder::Frontage { unit_id, width_mm } => {
                let unit = self
                    .units()
                    .iter()
                    .find(|unit| unit.id() == unit_id)
                    .ok_or_else(|| TacticalError::UnitNotFound(unit_id.clone()))?;
                if *width_mm < 1_000 {
                    return Err(TacticalError::InvalidFormation(unit_id.clone()));
                }
                let files =
                    u16::try_from((width_mm / 1_000).min(u32::from(unit.soldiers())).max(1))
                        .expect("files are bounded by soldiers");
                let position = unit.position();
                let destination = unit.destination();
                let queued = unit
                    .queued_movements()
                    .iter()
                    .map(|waypoint| waypoint.destination)
                    .collect::<Vec<_>>();
                self.issue_formation_order(unit_id, Formation::Line { files })?;
                self.validate_formation_placement(unit_id, position)?;
                if let Some(destination) = destination {
                    self.validate_formation_placement(unit_id, destination)?;
                }
                for destination in queued {
                    self.validate_formation_placement(unit_id, destination)?;
                }
                Ok(())
            }
        }
    }

    fn order_facing(&self, unit_id: &str) -> Result<Facing, TacticalError> {
        let unit = self
            .units()
            .iter()
            .find(|unit| unit.id() == unit_id)
            .ok_or_else(|| TacticalError::UnitNotFound(unit_id.to_owned()))?;
        Ok(unit.facing().unwrap_or(match unit.side() {
            BattleSide::Attacker => Facing::east(),
            BattleSide::Defender => Facing::west(),
        }))
    }

    fn rotate_formation(&mut self, unit_id: &str, facing: Facing) -> Result<(), TacticalError> {
        let position = self
            .units()
            .iter()
            .find(|unit| unit.id() == unit_id)
            .ok_or_else(|| TacticalError::UnitNotFound(unit_id.to_owned()))?
            .position();
        self.validate_formation_placement(unit_id, position)?;
        self.issue_move_order(MovementOrder {
            unit_id: unit_id.to_owned(),
            destination: position,
        })?;
        self.issue_facing_order(unit_id, facing)
    }

    pub(crate) fn validate_formation_placement(
        &self,
        unit_id: &str,
        position: BattlePoint,
    ) -> Result<(), TacticalError> {
        let unit = self
            .units()
            .iter()
            .find(|unit| unit.id() == unit_id)
            .ok_or_else(|| TacticalError::UnitNotFound(unit_id.to_owned()))?;
        let bounds = FormationFootprint::at(unit, position);
        let field = self.battlefield();
        let zone = DeploymentZone {
            side: unit.side(),
            min_x_mm: 0,
            max_x_mm: field.width_mm,
            min_y_mm: 0,
            max_y_mm: field.depth_mm,
        };
        let mut blocked =
            crate::campaign_deployment::terrain_formation_obstacles(field, self.terrain());
        if let Some(siege) = self.siege_snapshot() {
            for area in siege
                .layout
                .wall_segments
                .into_iter()
                .chain((!siege.gate_state.is_traversable()).then_some(siege.layout.gate))
            {
                blocked.push(FormationFootprint {
                    min_x_mm: i64::from(area.min_x_mm),
                    max_x_mm: i64::from(area.max_x_mm),
                    min_y_mm: i64::from(area.min_y_mm),
                    max_y_mm: i64::from(area.max_y_mm),
                });
            }
        }
        if !bounds.is_inside(zone) || blocked.iter().any(|area| bounds.overlaps(*area)) {
            return Err(TacticalError::InvalidFormationPlacement {
                unit_id: unit_id.to_owned(),
                position,
            });
        }
        Ok(())
    }
}
