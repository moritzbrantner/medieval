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
