//! Crewed siege equipment (#126): a battering ram and a catapult.
//!
//! An engine is not a free entity. Explicit pre-battle setup
//! ([`crate::TacticalBattle::deploy_siege_with_engines`]) assigns each engine
//! to one existing attacker foot unit, its crew. The engine stands wherever the
//! crew stands and is used only through orders issued to the crew; the core
//! alone resolves range, reload, ammunition and damage.

use serde::{Deserialize, Serialize};

use crate::BattlePoint;

/// Nearest target a catapult can reach: 30 m from the crew position.
pub const CATAPULT_MIN_RANGE_MM: u32 = 30_000;
/// Farthest target a catapult can reach: 150 m from the crew position, well
/// beyond tower (30 m) and V1 archer (25 m) range.
pub const CATAPULT_MAX_RANGE_MM: u32 = 150_000;
/// Combat pulses between two catapult shots (5 s).
pub const CATAPULT_RELOAD_PULSES: u16 = 5;
/// Stones a catapult carries into battle.
pub const CATAPULT_AMMUNITION: u16 = 12;
/// Missile frontage of one catapult stone in the shared ranged rule: a hit on
/// an unarmored target removes 5 soldiers.
pub(crate) const CATAPULT_SHOT_FRONTAGE: u32 = 60;

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SiegeEngineKind {
    /// Gate-focused engine: its crew's gate assault deals
    /// [`crate::SiegeProfile::ram_damage_per_pulse`].
    BatteringRam,
    /// Ranged engine with finite stones and a fixed reload.
    Catapult,
}

/// One engine of an explicit pre-battle siege setup.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SiegeEngineSetup {
    pub kind: SiegeEngineKind,
    pub crew_unit_id: String,
}

/// What a catapult fires at.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SiegeEngineTarget {
    /// The closed gate, aimed at its center.
    Gate,
    /// A formed enemy unit.
    Unit(String),
}

/// Core read model of one engine, in setup order.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SiegeEngineStatus {
    pub kind: SiegeEngineKind,
    pub crew_unit_id: String,
    /// The crew position; an engine moves only with its crew.
    pub position: BattlePoint,
    /// Remaining stones; `None` for a ram.
    pub ammunition: Option<u16>,
    /// Combat pulses until the engine can fire again (0 = loaded).
    pub pulses_until_loaded: u16,
    /// Tick of the most recent shot, for presentation effects.
    pub last_shot_tick: Option<u64>,
}

/// Serialized engine state. Ammunition, reload and last shot persist so a
/// save made mid-reload continues exactly.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SiegeEngineState {
    pub(crate) kind: SiegeEngineKind,
    pub(crate) crew_unit_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) ammunition: Option<u16>,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub(crate) pulses_until_loaded: u16,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) last_shot_tick: Option<u64>,
}

const fn is_zero(value: &u16) -> bool {
    *value == 0
}

impl SiegeEngineState {
    pub(crate) fn new(setup: SiegeEngineSetup) -> Self {
        Self {
            kind: setup.kind,
            ammunition: (setup.kind == SiegeEngineKind::Catapult).then_some(CATAPULT_AMMUNITION),
            crew_unit_id: setup.crew_unit_id,
            pulses_until_loaded: 0,
            last_shot_tick: None,
        }
    }

    /// Restores the rule bounds of a loaded document: a ram has no stones and
    /// a catapult never holds more than its budget or waits longer than one
    /// reload.
    pub(crate) fn normalized(mut self) -> Self {
        match self.kind {
            SiegeEngineKind::BatteringRam => {
                self.ammunition = None;
                self.pulses_until_loaded = 0;
            }
            SiegeEngineKind::Catapult => {
                self.ammunition = Some(self.ammunition.unwrap_or(0).min(CATAPULT_AMMUNITION));
                self.pulses_until_loaded = self.pulses_until_loaded.min(CATAPULT_RELOAD_PULSES);
            }
        }
        self
    }

    pub(crate) fn status(&self, position: BattlePoint) -> SiegeEngineStatus {
        SiegeEngineStatus {
            kind: self.kind,
            crew_unit_id: self.crew_unit_id.clone(),
            position,
            ammunition: self.ammunition,
            pulses_until_loaded: self.pulses_until_loaded,
            last_shot_tick: self.last_shot_tick,
        }
    }
}
