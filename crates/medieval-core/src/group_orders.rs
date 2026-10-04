use serde::{Deserialize, Serialize};

use crate::BattlePoint;

/// Pending waypoints per unit, excluding its current destination.
pub const MAX_QUEUED_WAYPOINTS: usize = 16;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MovementMode {
    #[default]
    March,
    AttackMove,
}

impl MovementMode {
    pub(crate) fn is_march(&self) -> bool {
        *self == Self::March
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MovementWaypoint {
    pub destination: BattlePoint,
    pub mode: MovementMode,
}

/// Destination identifies the selected group's integer centroid. Queued orders
/// preserve offsets from the last planned waypoint rather than moving positions.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GroupMovementOrder {
    pub unit_ids: Vec<String>,
    pub destination: BattlePoint,
    pub mode: MovementMode,
    #[serde(default)]
    pub queued: bool,
}

pub(crate) fn deserialize_waypoints<'de, D>(
    deserializer: D,
) -> Result<Vec<MovementWaypoint>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let waypoints = Vec::<MovementWaypoint>::deserialize(deserializer)?;
    if waypoints.len() > MAX_QUEUED_WAYPOINTS {
        return Err(serde::de::Error::custom(
            "too many queued tactical waypoints",
        ));
    }
    Ok(waypoints)
}

impl crate::TacticalBattle {
    pub fn issue_group_movement(
        &mut self,
        order: GroupMovementOrder,
    ) -> Result<(), crate::TacticalError> {
        use crate::{BattlePoint, TacticalError};
        use std::collections::BTreeSet;
        self.ensure_running()?;
        let ids = order.unit_ids.iter().collect::<BTreeSet<_>>();
        if ids.is_empty() || ids.len() != order.unit_ids.len() {
            return Err(TacticalError::InvalidMovementGroup);
        }
        let mut units = Vec::with_capacity(ids.len());
        for id in ids {
            let unit = self
                .units()
                .iter()
                .find(|unit| unit.id() == id)
                .ok_or_else(|| TacticalError::UnitNotFound(id.clone()))?;
            units.push(unit);
        }
        let side = units[0].side();
        if units.iter().any(|unit| unit.side() != side) {
            return Err(TacticalError::InvalidMovementGroup);
        }
        let mut anchors = units
            .iter()
            .map(|unit| {
                if order.queued {
                    unit.queued_movements()
                        .last()
                        .map(|waypoint| waypoint.destination)
                        .or(unit.destination())
                        .unwrap_or(unit.position())
                } else {
                    unit.position()
                }
            })
            .collect::<Vec<_>>();
        if anchors
            .iter()
            .map(|point| (point.x_mm, point.y_mm))
            .collect::<BTreeSet<_>>()
            .len()
            != anchors.len()
        {
            // Legacy or intentionally coincident positions need distinct slots.
            let width = units
                .iter()
                .map(|unit| {
                    unit.formation_footprint().max_x_mm - unit.formation_footprint().min_x_mm
                })
                .max()
                .expect("nonempty group") as u64
                + 1_000;
            let depth = units
                .iter()
                .map(|unit| {
                    unit.formation_footprint().max_y_mm - unit.formation_footprint().min_y_mm
                })
                .max()
                .expect("nonempty group") as u64
                + 1_000;
            let mut columns = 1_usize;
            while columns < anchors.len().div_ceil(columns) {
                columns += 1;
            }
            anchors = units
                .iter()
                .enumerate()
                .map(|(index, unit)| {
                    let x = u64::try_from(index % columns).expect("slot fits u64") * width;
                    let y = u64::try_from(index / columns).expect("slot fits u64") * depth;
                    Ok(BattlePoint::new(
                        u32::try_from(x).map_err(|_| {
                            TacticalError::GroupDestinationOutOfBounds(unit.id().to_owned())
                        })?,
                        u32::try_from(y).map_err(|_| {
                            TacticalError::GroupDestinationOutOfBounds(unit.id().to_owned())
                        })?,
                    ))
                })
                .collect::<Result<Vec<_>, TacticalError>>()?;
        }
        let count = anchors.len() as u128;
        let x = anchors
            .iter()
            .map(|point| u128::from(point.x_mm))
            .sum::<u128>()
            / count;
        let y = anchors
            .iter()
            .map(|point| u128::from(point.y_mm))
            .sum::<u128>()
            / count;
        let centre = BattlePoint::new(
            u32::try_from(x).expect("centroid is bounded by positions"),
            u32::try_from(y).expect("centroid is bounded by positions"),
        );
        let mut candidate = self.clone();
        for (unit, anchor) in units.into_iter().zip(anchors) {
            let x =
                i64::from(order.destination.x_mm) + i64::from(anchor.x_mm) - i64::from(centre.x_mm);
            let y =
                i64::from(order.destination.y_mm) + i64::from(anchor.y_mm) - i64::from(centre.y_mm);
            let (Ok(x), Ok(y)) = (u32::try_from(x), u32::try_from(y)) else {
                return Err(TacticalError::GroupDestinationOutOfBounds(
                    unit.id().to_owned(),
                ));
            };
            let destination = BattlePoint::new(x, y);
            candidate.validate_formation_placement(unit.id(), destination)?;
            candidate.issue_unit_waypoint(
                unit.id(),
                MovementWaypoint {
                    destination,
                    mode: order.mode,
                },
                order.queued,
            )?;
        }
        *self = candidate;
        Ok(())
    }
}
