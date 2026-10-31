use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::deployment::siege::{SiegeBattleState, SiegeGateState, SiegeLadderPlacement};
use crate::siege_engine::{SiegeEngineState, SiegeEngineTarget};
use crate::terrain::TacticalTerrain;
use crate::{TacticalWorkCounters, UnitKind};

mod combat;
mod damage;
mod error;
mod geometry;
mod movement;
mod orders;
mod setup;
mod siege_engines;
mod siege_orders;
#[cfg(test)]
mod tests;
mod tick;
mod unit;

pub use error::TacticalError;

#[path = "tactical_deployment.rs"]
mod deployment_phase;
pub(crate) use deployment_phase::DeploymentEdit;
pub use deployment_phase::DeploymentRejection;

pub const TACTICAL_TICKS_PER_SECOND: u32 = 20;
pub const MAX_TACTICAL_FATIGUE: u16 = 1_000;
pub const MAX_TACTICAL_MORALE: u16 = 1_000;
pub const ROUT_MORALE_THRESHOLD: u16 = 250;
pub const COMBAT_CONTACT_DISTANCE_MM: u32 = 1_500;
pub const PURSUIT_DISTANCE_MM: u32 = 6_000;

const MOVEMENT_FATIGUE_PER_TICK: u16 = 1;
const ROUT_FATIGUE_PER_TICK: u16 = 2;
const IDLE_FATIGUE_RECOVERY_PER_TICK: u16 = 1;
const COMBAT_FATIGUE_PER_PULSE: u16 = 30;
const PURSUIT_CASUALTY_DIVISOR: u32 = 4;
const MELEE_CASUALTY_DIVISOR: u32 = 8;
const RANGED_CASUALTY_DIVISOR: u32 = 12;

const fn default_attack_range_mm() -> u32 {
    COMBAT_CONTACT_DISTANCE_MM
}

const fn valid_attack_range_mm(attack_range_mm: u32) -> bool {
    attack_range_mm >= COMBAT_CONTACT_DISTANCE_MM && attack_range_mm <= i32::MAX as u32
}

fn deserialize_attack_range_mm<'de, D>(deserializer: D) -> Result<u32, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let attack_range_mm = u32::deserialize(deserializer)?;
    if valid_attack_range_mm(attack_range_mm) {
        Ok(attack_range_mm)
    } else {
        Err(serde::de::Error::custom(format!(
            "invalid tactical attack range {attack_range_mm} mm"
        )))
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BattlePoint {
    pub x_mm: u32,
    pub y_mm: u32,
}

impl BattlePoint {
    #[must_use]
    pub const fn new(x_mm: u32, y_mm: u32) -> Self {
        Self { x_mm, y_mm }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FlatBattlefield {
    pub width_mm: u32,
    pub depth_mm: u32,
}

impl FlatBattlefield {
    #[must_use]
    pub const fn new(width_mm: u32, depth_mm: u32) -> Self {
        Self { width_mm, depth_mm }
    }

    const fn contains(self, point: BattlePoint) -> bool {
        point.x_mm <= self.width_mm && point.y_mm <= self.depth_mm
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BattleSide {
    Attacker,
    Defender,
}

/// Versioned completion policy. Its absence preserves historical sandbox replays.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
enum TacticalCompletionRules {
    FieldAndSiegeV1,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TacticalFinishReason {
    ForceDefeated,
    MutualDefeat,
    SiegeCapture,
    Withdrawal,
    MutualWithdrawal,
}

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "phase", rename_all = "camelCase")]
pub enum TacticalBattleState {
    /// Pre-battle deployment: formations are arranged inside their own zones
    /// under the footprint rule, and no tick runs until
    /// [`TacticalBattle::begin_battle`]. Entered with
    /// [`TacticalBattle::into_deployment`].
    Deployment,
    #[default]
    Running,
    Finished {
        winner: Option<BattleSide>,
        #[serde(rename = "finishingTick")]
        finishing_tick: u64,
        reason: TacticalFinishReason,
    },
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Formation {
    Line { files: u16 },
    Column { files: u16 },
}

impl Formation {
    const fn files(self) -> u16 {
        match self {
            Self::Line { files } | Self::Column { files } => files,
        }
    }

    const fn frontage_slots(self, soldiers: u16) -> u16 {
        let frontage = match self {
            Self::Line { files } => files,
            Self::Column { files } => files.saturating_add(1) / 2,
        };
        if frontage < soldiers {
            frontage
        } else {
            soldiers
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
enum TacticalUnitState {
    Formed,
    Routed,
    Withdrawing { routed: bool },
    Escaped { routed: bool },
    Destroyed,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MovementOrder {
    pub unit_id: String,
    pub destination: BattlePoint,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TacticalUnitProvenance {
    pub source_army_id: String,
    pub initial_soldiers: u16,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TacticalUnit {
    id: String,
    side: BattleSide,
    soldiers: u16,
    position: BattlePoint,
    formation: Formation,
    speed_mm_per_tick: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    unit_kind: Option<UnitKind>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    combat_profile: Option<crate::UnitCombatProfile>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    facing: Option<crate::Facing>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    campaign_provenance: Option<TacticalUnitProvenance>,
    #[serde(
        default = "default_attack_range_mm",
        deserialize_with = "deserialize_attack_range_mm"
    )]
    attack_range_mm: u32,
    destination: Option<BattlePoint>,
    #[serde(default, skip_serializing_if = "crate::MovementMode::is_march")]
    movement_mode: crate::MovementMode,
    #[serde(
        default,
        skip_serializing_if = "Vec::is_empty",
        deserialize_with = "crate::group_orders::deserialize_waypoints"
    )]
    queued_movements: Vec<crate::MovementWaypoint>,
    engagement_target: Option<String>,
    /// Explicit order to assault the siege gate. Serialized only while set so
    /// documents without gate assaults keep their exact shape.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    gate_attack: bool,
    /// Pending order to place this unit's ladder at a wall point. Serialized
    /// only while set so documents without ladders keep their exact shape.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    ladder_order: Option<BattlePoint>,
    /// Standing fire order of this unit's catapult. Serialized only while set
    /// so documents without engines keep their exact shape.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    siege_engine_fire: Option<SiegeEngineTarget>,
    fatigue: u16,
    morale: u16,
    state: TacticalUnitState,
    #[serde(default)]
    pursuit_casualties: u16,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    charge: Option<crate::CavalryChargeState>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    arrival_facing: Option<crate::Facing>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    ammunition: Option<MissileAmmunition>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "u16", into = "u16")]
struct MissileAmmunition(u16);

impl TryFrom<u16> for MissileAmmunition {
    type Error = &'static str;
    fn try_from(value: u16) -> Result<Self, Self::Error> {
        let maximum = crate::UnitCombatProfile::v1(UnitKind::Archers)
            .stats()
            .missile
            .expect("archers have missiles")
            .ammunition;
        if value <= maximum {
            Ok(Self(value))
        } else {
            Err("ammunition exceeds the core volley budget")
        }
    }
}

impl From<MissileAmmunition> for u16 {
    fn from(value: MissileAmmunition) -> Self {
        value.0
    }
}

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WithdrawalSides {
    attacker: bool,
    defender: bool,
}

impl WithdrawalSides {
    const fn contains(self, side: BattleSide) -> bool {
        match side {
            BattleSide::Attacker => self.attacker,
            BattleSide::Defender => self.defender,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", from = "TacticalBattleWire")]
pub struct TacticalBattle {
    tick: u64,
    battlefield: FlatBattlefield,
    terrain: TacticalTerrain,
    #[serde(skip_serializing_if = "Option::is_none")]
    siege: Option<SiegeBattleState>,
    units: Vec<TacticalUnit>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) campaign_seed: Option<crate::TacticalBattleSeed>,
    #[serde(skip_serializing_if = "Option::is_none")]
    completion_rules: Option<TacticalCompletionRules>,
    state: TacticalBattleState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    withdrawal: Option<WithdrawalSides>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    ranged_damage_credit: BTreeMap<String, BTreeMap<String, CombatDamageCredit>>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    melee_damage_credit: BTreeMap<String, BTreeMap<String, CombatDamageCredit>>,
    /// Fractional siege tower damage per tower index and target unit.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    tower_damage_credit: BTreeMap<u8, BTreeMap<String, CombatDamageCredit>>,
    /// Placed siege ladders in placement order, omitted while empty so
    /// battles without ladders keep their exact document shape.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    siege_ladders: Vec<SiegeLadderPlacement>,
    /// Siege engines in setup order, omitted while empty so battles without
    /// engines keep their exact document shape.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    siege_engines: Vec<SiegeEngineState>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "u16", into = "u16")]
struct CombatDamageCredit(u16);

impl TryFrom<u16> for CombatDamageCredit {
    type Error = &'static str;
    fn try_from(value: u16) -> Result<Self, Self::Error> {
        if value < 1_000 {
            Ok(Self(value))
        } else {
            Err("combat damage credit must be below 1,000")
        }
    }
}

impl From<CombatDamageCredit> for u16 {
    fn from(value: CombatDamageCredit) -> Self {
        value.0
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct TacticalBattleWire {
    tick: u64,
    battlefield: FlatBattlefield,
    #[serde(default)]
    terrain: TacticalTerrain,
    #[serde(default)]
    siege: Option<SiegeBattleState>,
    units: Vec<TacticalUnit>,
    #[serde(default)]
    campaign_seed: Option<crate::TacticalBattleSeed>,
    #[serde(default)]
    completion_rules: Option<TacticalCompletionRules>,
    #[serde(default)]
    state: TacticalBattleState,
    #[serde(default)]
    withdrawal: Option<WithdrawalSides>,
    #[serde(default)]
    ranged_damage_credit: BTreeMap<String, BTreeMap<String, CombatDamageCredit>>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    melee_damage_credit: BTreeMap<String, BTreeMap<String, CombatDamageCredit>>,
    /// Fractional siege tower damage per tower index and target unit.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    tower_damage_credit: BTreeMap<u8, BTreeMap<String, CombatDamageCredit>>,
    #[serde(default)]
    siege_ladders: Vec<SiegeLadderPlacement>,
    #[serde(default)]
    siege_engines: Vec<SiegeEngineState>,
}

impl From<TacticalBattleWire> for TacticalBattle {
    fn from(mut wire: TacticalBattleWire) -> Self {
        wire.units.sort_by(|left, right| left.id.cmp(&right.id));
        Self {
            tick: wire.tick,
            battlefield: wire.battlefield,
            terrain: wire.terrain,
            siege: wire.siege.map(|mut siege| {
                // A gate destroyed before integrity was recorded has none left.
                if siege.gate_state == SiegeGateState::Destroyed {
                    siege.gate_integrity = 0;
                }
                siege
            }),
            campaign_seed: wire.campaign_seed,
            completion_rules: wire.completion_rules,
            state: wire.state,
            withdrawal: wire.withdrawal,
            ranged_damage_credit: wire.ranged_damage_credit,
            melee_damage_credit: wire.melee_damage_credit,
            tower_damage_credit: wire.tower_damage_credit,
            siege_ladders: if wire.siege.is_some() {
                wire.siege_ladders
            } else {
                Vec::new()
            },
            siege_engines: if wire.siege.is_some() {
                wire.siege_engines
                    .into_iter()
                    .map(SiegeEngineState::normalized)
                    .collect()
            } else {
                Vec::new()
            },
            units: if wire.siege.is_some() {
                wire.units
            } else {
                // A field battle has no wall to climb and no engines: pending
                // ladder and engine orders go too.
                wire.units
                    .into_iter()
                    .map(|mut unit| {
                        unit.ladder_order = None;
                        unit.siege_engine_fire = None;
                        unit
                    })
                    .collect()
            },
        }
    }
}

/// Finds a unit by id in the id-ordered unit vector with one indexed lookup.
fn find_work_target<'a>(
    units: &'a [TacticalUnit],
    id: &str,
    counters: &mut TacticalWorkCounters,
) -> Option<&'a TacticalUnit> {
    find_unit(units, id, counters)
}

fn find_unit<'a>(
    units: &'a [TacticalUnit],
    id: &str,
    counters: &mut TacticalWorkCounters,
) -> Option<&'a TacticalUnit> {
    counters.indexed_unit_lookups += 1;
    units
        .binary_search_by(|unit| unit.id.as_str().cmp(id))
        .ok()
        .map(|index| &units[index])
}
