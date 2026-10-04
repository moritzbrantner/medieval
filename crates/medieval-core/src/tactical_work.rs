use serde::{Deserialize, Serialize};

/// Work performed by tick advancement, independent of simulation state and timing.
/// Query counts distinguish tactical proximity requests from actual physics calls.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TacticalWorkCounters {
    pub ticks: u64,
    pub movement_unit_visits: u64,
    pub snapshot_clones: u64,
    pub snapshot_unit_copies: u64,
    pub target_candidate_visits: u64,
    pub proximity_queries: u64,
    pub physics_contact_queries: u64,
    pub path_requests: u64,
    pub terrain_speed_queries: u64,
    pub combat_pulses: u64,
    pub engagement_pairs: u64,
    pub ranged_volleys: u64,
    pub melee_contacts: u64,
    pub pursuit_contacts: u64,
}
