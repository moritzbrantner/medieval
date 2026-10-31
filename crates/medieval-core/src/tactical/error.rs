//! Errors returned by tactical battle orders and construction.

use super::{BattlePoint, BattleSide, DeploymentRejection};
use std::fmt;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TacticalError {
    InvalidMovementGroup,
    GroupDestinationOutOfBounds(String),
    WaypointLimitReached(String),
    InvalidBattlefield {
        width_mm: u32,
        depth_mm: u32,
    },
    CampaignForceTooLarge(BattleSide),
    InvalidCampaignProvenance(BattleSide),
    DeploymentFull {
        unit_id: String,
        side: BattleSide,
    },
    BattleFinished,
    /// An order other than Withdraw was given during deployment.
    BattleNotStarted,
    /// A deployment edit or start after the battle has already begun.
    DeploymentClosed,
    /// A deployment edit whose resulting footprint breaks the placement rule.
    InvalidDeploymentPlacement {
        unit_id: String,
        reason: DeploymentRejection,
    },
    WithdrawalRequiresStartedBattle,
    WithdrawalNoSurvivors(BattleSide),
    WithdrawalBlockedBySiege(BattleSide),
    TargetEscaped(String),
    EmptyUnitId,
    DuplicateUnitId(String),
    ZeroSoldiers(String),
    InvalidFormation(String),
    InvalidFormationPlacement {
        unit_id: String,
        position: BattlePoint,
    },
    ZeroMovementSpeed(String),
    InvalidAttackRange {
        unit_id: String,
        attack_range_mm: u32,
    },
    UnitOutOfBounds {
        unit_id: String,
        position: BattlePoint,
    },
    UnitOnImpassableTerrain {
        unit_id: String,
        position: BattlePoint,
    },
    UnitOnImpassableSiegeStructure {
        unit_id: String,
        position: BattlePoint,
    },
    UnitOutsideDeploymentZone {
        unit_id: String,
        side: BattleSide,
        position: BattlePoint,
    },
    UnitNotFound(String),
    UnitCannotReceiveOrders {
        unit_id: String,
    },
    DestinationOutOfBounds {
        unit_id: String,
        destination: BattlePoint,
    },
    DestinationImpassable {
        unit_id: String,
        destination: BattlePoint,
    },
    FriendlyEngagement {
        unit_id: String,
        target_unit_id: String,
    },
    TargetDestroyed(String),
    NotSiegeBattle,
    SiegeExitInUse,
    /// Only attackers may assault the gate; defenders never damage their own.
    FriendlyGateAttack(String),
    /// Only a standing, closed gate can be assaulted.
    SiegeGateNotClosed,
    /// A destroyed gate stays destroyed; it cannot be opened or closed again.
    SiegeGateDestroyed,
    /// Only formed attacker foot units carry a ladder, one per unit.
    LadderUnavailable(String),
    /// A ladder stands only inside a wall segment, reachable from the unit's
    /// side of the wall.
    InvalidLadderPosition {
        unit_id: String,
        position: BattlePoint,
    },
    /// The unit does not crew a usable engine of the ordered kind: no engine,
    /// another kind, or an empty catapult. Also an invalid pre-battle crew.
    SiegeEngineUnavailable(String),
    /// A catapult fires only at formed enemy units.
    InvalidSiegeEngineTarget {
        unit_id: String,
        target_unit_id: String,
    },
    /// The target lies outside the catapult's minimum and maximum range.
    SiegeEngineTargetOutOfRange(String),
}

impl fmt::Display for TacticalError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidMovementGroup => write!(
                formatter,
                "movement group must contain distinct units from one side"
            ),
            Self::GroupDestinationOutOfBounds(id) => write!(
                formatter,
                "group destination places {id} outside the battlefield"
            ),
            Self::WaypointLimitReached(id) => write!(
                formatter,
                "tactical unit {id} has too many queued waypoints"
            ),
            Self::InvalidBattlefield { width_mm, depth_mm } => write!(
                formatter,
                "battlefield dimensions must be positive, got {width_mm}x{depth_mm} mm"
            ),
            Self::CampaignForceTooLarge(side) => write!(
                formatter,
                "{side:?} campaign force exceeds tactical construction capacity"
            ),
            Self::InvalidCampaignProvenance(side) => write!(
                formatter,
                "{side:?} source army rosters do not match campaign force provenance"
            ),
            Self::DeploymentFull { unit_id, side } => write!(
                formatter,
                "{side:?} deployment has no legal space for formation {unit_id}"
            ),
            Self::WithdrawalRequiresStartedBattle => {
                write!(formatter, "withdrawal requires a started tactical match")
            }
            Self::WithdrawalNoSurvivors(side) => {
                write!(formatter, "{side:?} has no surviving units to withdraw")
            }
            Self::WithdrawalBlockedBySiege(side) => write!(
                formatter,
                "{side:?} has no legal exit from the current siege"
            ),
            Self::TargetEscaped(unit_id) => {
                write!(formatter, "tactical unit {unit_id} has escaped")
            }
            Self::BattleFinished => write!(formatter, "the tactical battle has finished"),
            Self::BattleNotStarted => write!(
                formatter,
                "the battle has not started: arrange your formations, then Start battle"
            ),
            Self::DeploymentClosed => write!(
                formatter,
                "deployment is over: the battle has already started"
            ),
            Self::InvalidDeploymentPlacement { unit_id, reason } => match reason {
                DeploymentRejection::OutsideZone => write!(
                    formatter,
                    "{unit_id} cannot deploy there: its whole formation footprint must lie inside its own deployment zone"
                ),
                DeploymentRejection::FriendlyOverlap => write!(
                    formatter,
                    "{unit_id} cannot deploy there: its formation footprint would overlap another friendly formation"
                ),
            },
            Self::EmptyUnitId => write!(formatter, "tactical unit IDs must not be empty"),
            Self::DuplicateUnitId(unit_id) => {
                write!(formatter, "tactical unit ID {unit_id} is duplicated")
            }
            Self::ZeroSoldiers(unit_id) => {
                write!(formatter, "tactical unit {unit_id} must contain soldiers")
            }
            Self::InvalidFormationPlacement { unit_id, position } => write!(
                formatter,
                "formation {unit_id} does not fit legal ground at {}, {}",
                position.x_mm, position.y_mm
            ),
            Self::InvalidFormation(unit_id) => {
                write!(formatter, "tactical unit {unit_id} has an empty formation")
            }
            Self::ZeroMovementSpeed(unit_id) => write!(
                formatter,
                "tactical unit {unit_id} must have a positive movement speed"
            ),
            Self::InvalidAttackRange {
                unit_id,
                attack_range_mm,
            } => write!(
                formatter,
                "tactical unit {unit_id} has invalid attack range {attack_range_mm} mm"
            ),
            Self::UnitOutOfBounds { unit_id, position } => write!(
                formatter,
                "tactical unit {unit_id} starts outside the battlefield at ({}, {}) mm",
                position.x_mm, position.y_mm
            ),
            Self::UnitOnImpassableTerrain { unit_id, position } => write!(
                formatter,
                "tactical unit {unit_id} starts on impassable terrain at ({}, {}) mm",
                position.x_mm, position.y_mm
            ),
            Self::UnitOnImpassableSiegeStructure { unit_id, position } => write!(
                formatter,
                "tactical unit {unit_id} starts on an impassable siege structure at ({}, {}) mm",
                position.x_mm, position.y_mm
            ),
            Self::UnitOutsideDeploymentZone {
                unit_id,
                side,
                position,
            } => write!(
                formatter,
                "tactical unit {unit_id} starts outside the {:?} deployment zone at ({}, {}) mm",
                side, position.x_mm, position.y_mm
            ),
            Self::UnitNotFound(unit_id) => {
                write!(formatter, "tactical unit {unit_id} does not exist")
            }
            Self::UnitCannotReceiveOrders { unit_id } => {
                write!(formatter, "tactical unit {unit_id} cannot receive orders")
            }
            Self::DestinationOutOfBounds {
                unit_id,
                destination,
            } => write!(
                formatter,
                "movement order for {unit_id} leaves the battlefield at ({}, {}) mm",
                destination.x_mm, destination.y_mm
            ),
            Self::DestinationImpassable {
                unit_id,
                destination,
            } => write!(
                formatter,
                "movement order for {unit_id} targets impassable terrain at ({}, {}) mm",
                destination.x_mm, destination.y_mm
            ),
            Self::FriendlyEngagement {
                unit_id,
                target_unit_id,
            } => write!(
                formatter,
                "tactical unit {unit_id} cannot engage friendly unit {target_unit_id}"
            ),
            Self::TargetDestroyed(unit_id) => {
                write!(formatter, "tactical unit {unit_id} is already destroyed")
            }
            Self::SiegeExitInUse => {
                write!(formatter, "withdrawing units still need the siege exit")
            }
            Self::NotSiegeBattle => write!(formatter, "battle has no siege state"),
            Self::FriendlyGateAttack(unit_id) => write!(
                formatter,
                "tactical unit {unit_id} cannot attack its own side's siege gate"
            ),
            Self::SiegeGateNotClosed => {
                write!(formatter, "only a closed siege gate can be attacked")
            }
            Self::SiegeGateDestroyed => {
                write!(formatter, "a destroyed siege gate cannot change state")
            }
            Self::LadderUnavailable(unit_id) => {
                write!(formatter, "tactical unit {unit_id} has no ladder to place")
            }
            Self::InvalidLadderPosition { unit_id, position } => write!(
                formatter,
                "tactical unit {unit_id} cannot place a ladder at ({}, {}) mm",
                position.x_mm, position.y_mm
            ),
            Self::SiegeEngineUnavailable(unit_id) => write!(
                formatter,
                "tactical unit {unit_id} has no usable siege engine for this order"
            ),
            Self::InvalidSiegeEngineTarget {
                unit_id,
                target_unit_id,
            } => write!(
                formatter,
                "the siege engine of {unit_id} cannot fire at {target_unit_id}"
            ),
            Self::SiegeEngineTargetOutOfRange(unit_id) => write!(
                formatter,
                "the siege engine target of {unit_id} is out of range"
            ),
        }
    }
}

impl std::error::Error for TacticalError {}
