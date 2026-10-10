use std::{
    collections::{BTreeMap, BTreeSet, HashSet},
    fmt,
};

use physics_engine::{Collider, ColliderShape, Vec3i, collider_contact};
use serde::{Deserialize, Serialize};

use crate::deployment::siege::{SiegeBattleState, SiegeGateState, SiegeProfile};
use crate::deployment::{DeploymentZone, standard_deployment_zone, standard_deployment_zones};
use crate::terrain::{BattlefieldLocation, COMBAT_FACTOR_BASE_MILLI, TacticalTerrain};
use crate::{TacticalWorkCounters, UnitKind};

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

impl TacticalUnit {
    #[must_use]
    pub fn new(
        id: impl Into<String>,
        side: BattleSide,
        soldiers: u16,
        position: BattlePoint,
        formation: Formation,
        speed_mm_per_tick: u32,
    ) -> Self {
        Self {
            id: id.into(),
            side,
            soldiers,
            position,
            formation,
            speed_mm_per_tick,
            unit_kind: None,
            combat_profile: None,
            facing: None,
            campaign_provenance: None,
            attack_range_mm: COMBAT_CONTACT_DISTANCE_MM,
            destination: None,
            movement_mode: crate::MovementMode::March,
            queued_movements: Vec::new(),
            engagement_target: None,
            fatigue: 0,
            morale: MAX_TACTICAL_MORALE,
            state: TacticalUnitState::Formed,
            pursuit_casualties: 0,
            charge: None,
            arrival_facing: None,
            ammunition: None,
        }
    }

    pub(crate) fn with_campaign_provenance(mut self, army_id: &str, initial_soldiers: u16) -> Self {
        self.campaign_provenance = Some(TacticalUnitProvenance {
            source_army_id: army_id.to_owned(),
            initial_soldiers,
        });
        self
    }

    #[must_use]
    pub const fn campaign_provenance(&self) -> Option<&TacticalUnitProvenance> {
        self.campaign_provenance.as_ref()
    }

    #[must_use]
    pub const fn with_unit_kind(mut self, unit_kind: UnitKind) -> Self {
        self.unit_kind = Some(unit_kind);
        self.combat_profile = None;
        self.charge = None;
        self.ammunition = None;
        self
    }

    #[must_use]
    pub const fn with_combat_stats(mut self, profile: crate::UnitCombatProfile) -> Self {
        let stats = profile.stats();
        self.ammunition = match stats.missile {
            Some(missile) => Some(MissileAmmunition(missile.ammunition)),
            None => None,
        };
        self.charge = if stats.charge_impact_milli > 0 {
            Some(crate::CavalryChargeState::Ready)
        } else {
            None
        };
        self.unit_kind = Some(profile.kind);
        self.combat_profile = Some(profile);
        self.facing = Some(match self.side {
            BattleSide::Attacker => crate::Facing::east(),
            BattleSide::Defender => crate::Facing::west(),
        });
        self.speed_mm_per_tick = stats.movement_mm_per_tick;
        self.morale = stats.initial_morale;
        self.attack_range_mm = match stats.missile {
            Some(missile) => missile.range_mm,
            None => COMBAT_CONTACT_DISTANCE_MM,
        };
        self
    }

    #[must_use]
    pub const fn with_facing(mut self, facing: crate::Facing) -> Self {
        self.facing = Some(facing);
        self
    }

    #[must_use]
    pub const fn movement_mode(&self) -> crate::MovementMode {
        self.movement_mode
    }

    #[must_use]
    pub fn queued_movements(&self) -> &[crate::MovementWaypoint] {
        &self.queued_movements
    }

    #[must_use]
    pub const fn facing(&self) -> Option<crate::Facing> {
        self.facing
    }

    #[must_use]
    pub fn incoming_arc(&self, attacker_position: BattlePoint) -> Option<crate::CombatArc> {
        self.facing
            .map(|facing| facing.classify(self.position, attacker_position))
    }

    #[must_use]
    pub const fn charge_state(&self) -> Option<crate::CavalryChargeState> {
        self.charge
    }

    fn interrupt_charge(&mut self) {
        self.charge = self.charge.map(crate::CavalryChargeState::interrupted);
    }

    #[must_use]
    pub const fn combat_profile(&self) -> Option<crate::UnitCombatProfile> {
        self.combat_profile
    }

    #[must_use]
    pub const fn stats(&self) -> Option<crate::UnitStats> {
        match self.combat_profile {
            Some(profile) => Some(profile.stats()),
            None => None,
        }
    }

    #[must_use]
    pub const fn with_attack_range_mm(mut self, attack_range_mm: u32) -> Self {
        self.attack_range_mm = attack_range_mm;
        self
    }

    #[must_use]
    pub fn id(&self) -> &str {
        &self.id
    }

    #[must_use]
    pub const fn side(&self) -> BattleSide {
        self.side
    }

    #[must_use]
    pub const fn soldiers(&self) -> u16 {
        self.soldiers
    }

    #[must_use]
    pub const fn position(&self) -> BattlePoint {
        self.position
    }

    #[must_use]
    pub fn formation_footprint(&self) -> crate::FormationFootprint {
        crate::FormationFootprint::at(self, self.position)
    }

    /// Initial campaign heading: zero faces +X, 180 degrees faces -X.
    #[must_use]
    pub const fn initial_facing_millidegrees(&self) -> u32 {
        match self.side {
            BattleSide::Attacker => 0,
            BattleSide::Defender => 180_000,
        }
    }

    pub(crate) fn set_initial_position(&mut self, point: BattlePoint) {
        self.position = point;
    }

    #[must_use]
    pub const fn formation(&self) -> Formation {
        self.formation
    }

    #[must_use]
    pub const fn frontage_slots(&self) -> u16 {
        self.formation.frontage_slots(self.soldiers)
    }

    #[must_use]
    pub const fn speed_mm_per_tick(&self) -> u32 {
        self.speed_mm_per_tick
    }

    #[must_use]
    pub const fn unit_kind(&self) -> Option<UnitKind> {
        match self.combat_profile {
            Some(profile) => Some(profile.kind),
            None => self.unit_kind,
        }
    }

    #[must_use]
    pub const fn attack_range_mm(&self) -> u32 {
        match self.ammunition {
            Some(MissileAmmunition(0)) => COMBAT_CONTACT_DISTANCE_MM,
            _ => self.attack_range_mm,
        }
    }

    #[must_use]
    pub const fn ammunition(&self) -> Option<u16> {
        match self.ammunition {
            Some(ammunition) => Some(ammunition.0),
            None => None,
        }
    }

    #[must_use]
    pub const fn destination(&self) -> Option<BattlePoint> {
        self.destination
    }

    #[must_use]
    pub fn engagement_target(&self) -> Option<&str> {
        self.engagement_target.as_deref()
    }

    #[must_use]
    pub const fn fatigue(&self) -> u16 {
        self.fatigue
    }

    #[must_use]
    pub const fn morale(&self) -> u16 {
        self.morale
    }

    #[must_use]
    pub const fn is_routed(&self) -> bool {
        matches!(
            self.state,
            TacticalUnitState::Routed
                | TacticalUnitState::Withdrawing { routed: true }
                | TacticalUnitState::Escaped { routed: true }
        )
    }

    #[must_use]
    pub const fn is_withdrawing(&self) -> bool {
        matches!(self.state, TacticalUnitState::Withdrawing { .. })
    }

    #[must_use]
    pub const fn is_escaped(&self) -> bool {
        matches!(self.state, TacticalUnitState::Escaped { .. })
    }

    #[must_use]
    pub const fn can_receive_orders(&self) -> bool {
        matches!(self.state, TacticalUnitState::Formed)
    }

    #[must_use]
    pub const fn pursuit_casualties(&self) -> u16 {
        self.pursuit_casualties
    }

    const fn is_pursuit_target(&self) -> bool {
        matches!(
            self.state,
            TacticalUnitState::Routed | TacticalUnitState::Withdrawing { .. }
        )
    }

    #[must_use]
    pub const fn is_destroyed(&self) -> bool {
        matches!(self.state, TacticalUnitState::Destroyed)
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
}

impl From<TacticalBattleWire> for TacticalBattle {
    fn from(mut wire: TacticalBattleWire) -> Self {
        wire.units.sort_by(|left, right| left.id.cmp(&right.id));
        Self {
            tick: wire.tick,
            battlefield: wire.battlefield,
            terrain: wire.terrain,
            siege: wire.siege,
            campaign_seed: wire.campaign_seed,
            completion_rules: wire.completion_rules,
            state: wire.state,
            withdrawal: wire.withdrawal,
            ranged_damage_credit: wire.ranged_damage_credit,
            melee_damage_credit: wire.melee_damage_credit,
            units: wire.units,
        }
    }
}

impl TacticalBattle {
    /// Starts an authoritative match with version-one completion rules.
    ///
    /// A side is defeated when it has no formed surviving units. Both sides
    /// defeated on the same tick is a draw; siege capture takes precedence.
    /// Sandbox constructors and historical documents remain open-ended until
    /// explicitly started. Calling this again never resets a finished result.
    #[must_use]
    pub fn start(mut self) -> Self {
        self.completion_rules = Some(TacticalCompletionRules::FieldAndSiegeV1);
        self.update_completion();
        self
    }

    #[must_use]
    pub const fn state(&self) -> TacticalBattleState {
        self.state
    }

    pub fn new(
        battlefield: FlatBattlefield,
        units: Vec<TacticalUnit>,
    ) -> Result<Self, TacticalError> {
        Self::new_at_location(battlefield, units, BattlefieldLocation::MountainPass)
    }

    pub fn new_at_location(
        battlefield: FlatBattlefield,
        units: Vec<TacticalUnit>,
        location: BattlefieldLocation,
    ) -> Result<Self, TacticalError> {
        Self::new_with_terrain(battlefield, units, TacticalTerrain::for_location(location))
    }

    fn new_with_terrain(
        battlefield: FlatBattlefield,
        mut units: Vec<TacticalUnit>,
        terrain: TacticalTerrain,
    ) -> Result<Self, TacticalError> {
        if battlefield.width_mm == 0 || battlefield.depth_mm == 0 {
            return Err(TacticalError::InvalidBattlefield {
                width_mm: battlefield.width_mm,
                depth_mm: battlefield.depth_mm,
            });
        }
        let mut unit_ids = HashSet::with_capacity(units.len());
        for unit in &units {
            if unit.id.trim().is_empty() {
                return Err(TacticalError::EmptyUnitId);
            }
            if !unit_ids.insert(unit.id.clone()) {
                return Err(TacticalError::DuplicateUnitId(unit.id.clone()));
            }
            if unit.soldiers == 0 {
                return Err(TacticalError::ZeroSoldiers(unit.id.clone()));
            }
            if unit.formation.files() == 0 {
                return Err(TacticalError::InvalidFormation(unit.id.clone()));
            }
            if unit.speed_mm_per_tick == 0 {
                return Err(TacticalError::ZeroMovementSpeed(unit.id.clone()));
            }
            if !valid_attack_range_mm(unit.attack_range_mm) {
                return Err(TacticalError::InvalidAttackRange {
                    unit_id: unit.id.clone(),
                    attack_range_mm: unit.attack_range_mm,
                });
            }
            if !battlefield.contains(unit.position) {
                return Err(TacticalError::UnitOutOfBounds {
                    unit_id: unit.id.clone(),
                    position: unit.position,
                });
            }
            if !terrain.is_passable_at(battlefield, unit.position) {
                return Err(TacticalError::UnitOnImpassableTerrain {
                    unit_id: unit.id.clone(),
                    position: unit.position,
                });
            }
        }

        units.sort_by(|left, right| left.id.cmp(&right.id));
        Ok(Self {
            tick: 0,
            battlefield,
            terrain,
            siege: None,
            campaign_seed: None,
            completion_rules: None,
            state: TacticalBattleState::Running,
            withdrawal: None,
            ranged_damage_credit: BTreeMap::new(),
            melee_damage_credit: BTreeMap::new(),
            units,
        })
    }

    pub fn deploy(
        battlefield: FlatBattlefield,
        units: Vec<TacticalUnit>,
    ) -> Result<Self, TacticalError> {
        Self::deploy_at_location(battlefield, units, BattlefieldLocation::MountainPass)
    }

    pub fn deploy_at_location(
        battlefield: FlatBattlefield,
        units: Vec<TacticalUnit>,
        location: BattlefieldLocation,
    ) -> Result<Self, TacticalError> {
        let battle = Self::new_at_location(battlefield, units, location)?;
        for unit in &battle.units {
            let zone = standard_deployment_zone(battlefield, unit.side);
            if !zone.contains(unit.position) {
                return Err(TacticalError::UnitOutsideDeploymentZone {
                    unit_id: unit.id.clone(),
                    side: unit.side,
                    position: unit.position,
                });
            }
        }
        Ok(battle)
    }

    pub fn deploy_siege(
        battlefield: FlatBattlefield,
        units: Vec<TacticalUnit>,
    ) -> Result<Self, TacticalError> {
        Self::deploy_siege_at_location(battlefield, units, BattlefieldLocation::MountainPass)
    }

    /// Development fixture: the stone-walls profile at `location`.
    pub fn deploy_siege_at_location(
        battlefield: FlatBattlefield,
        units: Vec<TacticalUnit>,
        location: BattlefieldLocation,
    ) -> Result<Self, TacticalError> {
        Self::deploy_siege_with_profile(battlefield, units, location, SiegeProfile::StoneWallsV1)
    }

    pub fn deploy_siege_with_profile(
        battlefield: FlatBattlefield,
        units: Vec<TacticalUnit>,
        location: BattlefieldLocation,
        profile: SiegeProfile,
    ) -> Result<Self, TacticalError> {
        let mut battle = Self::new_at_location(battlefield, units, location)?;
        let siege = SiegeBattleState::for_profile(battlefield, profile);
        for unit in &battle.units {
            let zone = siege
                .layout
                .deployment_zones
                .iter()
                .find(|zone| zone.side == unit.side)
                .expect("deterministic siege layout contains both deployment sides");
            if !zone.contains(unit.position) {
                return Err(TacticalError::UnitOutsideDeploymentZone {
                    unit_id: unit.id.clone(),
                    side: unit.side,
                    position: unit.position,
                });
            }
            if !siege.is_passable_at(unit.position) {
                return Err(TacticalError::UnitOnImpassableSiegeStructure {
                    unit_id: unit.id.clone(),
                    position: unit.position,
                });
            }
        }
        battle.siege = Some(siege);
        Ok(battle)
    }

    #[must_use]
    pub fn deployment_zones(&self) -> [DeploymentZone; 2] {
        self.siege.map_or_else(
            || standard_deployment_zones(self.battlefield),
            |siege| siege.layout.deployment_zones,
        )
    }

    #[must_use]
    pub const fn terrain(&self) -> TacticalTerrain {
        self.terrain
    }

    #[must_use]
    pub const fn tick(&self) -> u64 {
        self.tick
    }

    #[must_use]
    pub const fn battlefield(&self) -> FlatBattlefield {
        self.battlefield
    }

    #[must_use]
    pub const fn campaign_seed(&self) -> Option<&crate::TacticalBattleSeed> {
        self.campaign_seed.as_ref()
    }

    #[must_use]
    pub fn units(&self) -> &[TacticalUnit] {
        &self.units
    }

    #[must_use]
    pub const fn siege_snapshot(&self) -> Option<SiegeBattleState> {
        self.siege
    }

    pub fn open_siege_gate(&mut self) -> Result<(), TacticalError> {
        self.set_siege_gate_state(SiegeGateState::Open)
    }

    pub fn close_siege_gate(&mut self) -> Result<(), TacticalError> {
        self.set_siege_gate_state(SiegeGateState::Closed)
    }

    pub fn destroy_siege_gate(&mut self) -> Result<(), TacticalError> {
        self.set_siege_gate_state(SiegeGateState::Destroyed)
    }

    fn set_siege_gate_state(&mut self, gate_state: SiegeGateState) -> Result<(), TacticalError> {
        self.ensure_running()?;
        let Some(siege) = &mut self.siege else {
            return Err(TacticalError::NotSiegeBattle);
        };
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
        Ok(())
    }

    /// Commits a side to retreat toward its entry edge. Arrival marks a whole
    /// unit escaped; until then existing pursuit rules can inflict casualties.
    /// Fortress defenders have no exit in the prototype layout. Attackers
    /// inside its wall need a traversable gate. Repeated intents are idempotent.
    pub fn withdraw(&mut self, side: BattleSide) -> Result<(), TacticalError> {
        self.validate_withdrawal(side)?;
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
        self.ensure_running()?;
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
        unit.engagement_target = Some(target_unit_id.to_owned());
        Ok(())
    }

    pub fn issue_formation_order(
        &mut self,
        unit_id: &str,
        formation: Formation,
    ) -> Result<(), TacticalError> {
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

    pub fn advance_ticks(&mut self, ticks: u32) {
        self.advance_ticks_measured(ticks);
    }

    /// Advances the same authoritative rules while returning deterministic work.
    /// Counters are not retained or serialized in the battle.
    pub fn advance_ticks_measured(&mut self, ticks: u32) -> TacticalWorkCounters {
        let mut counters = TacticalWorkCounters::default();
        for _ in 0..ticks {
            if self.ensure_running().is_err() {
                break;
            }
            counters.ticks += 1;
            self.advance_movement_phase(&mut counters);
            self.tick = self.tick.saturating_add(1);
            if self
                .tick
                .is_multiple_of(u64::from(TACTICAL_TICKS_PER_SECOND))
            {
                self.resolve_combat_pulse(&mut counters);
                if let Some(siege) = &mut self.siege {
                    counters.unit_scan_visits += self.units.len() as u64;
                    siege.advance_capture(&self.units);
                }
            }
            counters.unit_scan_visits += self.units.len() as u64;
            self.clear_invalid_engagement_targets(&mut counters);
            counters.unit_scan_visits += self.update_completion();
        }
        counters
    }

    pub(crate) fn ensure_running(&self) -> Result<(), TacticalError> {
        match self.state {
            TacticalBattleState::Running => Ok(()),
            TacticalBattleState::Finished { .. } => Err(TacticalError::BattleFinished),
        }
    }

    /// Returns the units visited by the completion scans.
    fn update_completion(&mut self) -> u64 {
        let visits = std::cell::Cell::new(0_u64);
        if self.completion_rules.is_none() || self.ensure_running().is_err() {
            return 0;
        }
        let captured_by = self.siege.and_then(|siege| siege.capture.captured_by);
        let result = if let Some(winner) = captured_by {
            Some((Some(winner), TacticalFinishReason::SiegeCapture))
        } else if let Some(withdrawal) = self.withdrawal {
            let pending = |side| {
                self.units.iter().any(|unit| {
                    visits.set(visits.get() + 1);
                    unit.side == side && unit.is_withdrawing()
                })
            };
            match (withdrawal.attacker, withdrawal.defender) {
                (true, true)
                    if !pending(BattleSide::Attacker) && !pending(BattleSide::Defender) =>
                {
                    Some((None, TacticalFinishReason::MutualWithdrawal))
                }
                (true, false) if !pending(BattleSide::Attacker) => {
                    Some((Some(BattleSide::Defender), TacticalFinishReason::Withdrawal))
                }
                (false, true) if !pending(BattleSide::Defender) => {
                    Some((Some(BattleSide::Attacker), TacticalFinishReason::Withdrawal))
                }
                _ => None,
            }
        } else {
            let active = |side| {
                self.units.iter().any(|unit| {
                    visits.set(visits.get() + 1);
                    unit.side == side
                        && unit.soldiers > 0
                        && unit.state == TacticalUnitState::Formed
                })
            };
            match (active(BattleSide::Attacker), active(BattleSide::Defender)) {
                (true, true) => None,
                (true, false) => Some((
                    Some(BattleSide::Attacker),
                    TacticalFinishReason::ForceDefeated,
                )),
                (false, true) => Some((
                    Some(BattleSide::Defender),
                    TacticalFinishReason::ForceDefeated,
                )),
                (false, false) => Some((None, TacticalFinishReason::MutualDefeat)),
            }
        };
        if let Some((winner, reason)) = result {
            self.state = TacticalBattleState::Finished {
                winner,
                finishing_tick: self.tick,
                reason,
            };
        }
        visits.get()
    }

    fn unit_index(&self, unit_id: &str) -> Option<usize> {
        self.units
            .binary_search_by(|unit| unit.id.as_str().cmp(unit_id))
            .ok()
    }

    fn ensure_can_receive_orders(&self, unit_index: usize) -> Result<(), TacticalError> {
        let unit = &self.units[unit_index];
        if unit.state == TacticalUnitState::Formed {
            Ok(())
        } else {
            Err(TacticalError::UnitCannotReceiveOrders {
                unit_id: unit.id.clone(),
            })
        }
    }

    fn is_passable_at(&self, point: BattlePoint) -> bool {
        self.terrain.is_passable_at(self.battlefield, point)
            && self.siege.is_none_or(|siege| siege.is_passable_at(point))
    }

    fn movement_waypoint(&self, from: BattlePoint, destination: BattlePoint) -> BattlePoint {
        let terrain_waypoint = self
            .terrain
            .movement_waypoint(self.battlefield, from, destination);
        self.siege.map_or(terrain_waypoint, |siege| {
            siege.movement_waypoint(from, terrain_waypoint)
        })
    }

    fn measured_movement_waypoint(
        &self,
        counters: &mut TacticalWorkCounters,
        from: BattlePoint,
        destination: BattlePoint,
    ) -> BattlePoint {
        counters.path_requests += 1;
        self.movement_waypoint(from, destination)
    }

    fn advance_movement_orders(&mut self, counters: &mut TacticalWorkCounters) {
        // Queued-waypoint promotion and attack-move acquisition each visit
        // every unit once.
        counters.unit_scan_visits += 2 * self.units.len() as u64;
        for unit in &mut self.units {
            if unit.state == TacticalUnitState::Formed
                && unit.destination.is_none()
                && unit.engagement_target.is_none()
                && !unit.queued_movements.is_empty()
            {
                let waypoint = unit.queued_movements.remove(0);
                unit.destination =
                    (unit.position != waypoint.destination).then_some(waypoint.destination);
                unit.movement_mode = waypoint.mode;
                unit.arrival_facing = None;
                unit.interrupt_charge();
            }
        }
        // Acquisition reads only enemies' side, state and position, which this
        // loop never changes, so it reads live units instead of a snapshot.
        for index in 0..self.units.len() {
            let unit = &self.units[index];
            if unit.state != TacticalUnitState::Formed
                || unit.movement_mode != crate::MovementMode::AttackMove
            {
                continue;
            }
            if unit.destination.is_none() {
                self.units[index].engagement_target = None;
                self.units[index].movement_mode = crate::MovementMode::March;
                continue;
            }
            let units = &self.units;
            let radius = unit.attack_range_mm().max(5_000);
            let mut eligible = |enemy: &&TacticalUnit| {
                counters.target_candidate_visits += 1;
                enemy.side != unit.side
                    && enemy.state == TacticalUnitState::Formed
                    && measured_points_within_distance(
                        counters,
                        unit.position,
                        enemy.position,
                        radius,
                    )
            };
            let retained = units
                .iter()
                .filter(&mut eligible)
                .find(|enemy| Some(enemy.id.as_str()) == unit.engagement_target.as_deref());
            let target = retained.or_else(|| {
                units.iter().filter(&mut eligible).min_by(|left, right| {
                    point_distance_squared(unit.position, left.position)
                        .cmp(&point_distance_squared(unit.position, right.position))
                        .then_with(|| left.id.cmp(&right.id))
                })
            });
            let target = target.map(|enemy| enemy.id.clone());
            if self.units[index].engagement_target != target {
                self.units[index].interrupt_charge();
                self.units[index].engagement_target = target;
            }
        }
    }

    /// Every unit moves against the state all units had when the phase began.
    /// Instead of cloning the whole unit vector, a unit is copied only right
    /// before its own step may change it; reads of other units go through
    /// [`MovementBefore`], so idle units cost a visit but no copy.
    fn advance_movement_phase(&mut self, counters: &mut TacticalWorkCounters) {
        self.advance_movement_orders(counters);
        let mut before = MovementBefore::default();
        for index in 0..self.units.len() {
            counters.movement_unit_visits += 1;
            if !movement_may_change(&self.units[index]) {
                continue;
            }
            counters.snapshot_unit_copies += 1;
            before.save(index, self.units[index].clone());
            let unit = before.saved_unit(index);
            match unit.state {
                TacticalUnitState::Formed => {
                    let target = unit
                        .engagement_target
                        .as_deref()
                        .and_then(|target_id| {
                            before
                                .find(&self.units, target_id, counters)
                                .map(TargetView::of)
                        })
                        .filter(|target| {
                            !matches!(
                                target.state,
                                TacticalUnitState::Destroyed | TacticalUnitState::Escaped { .. }
                            )
                        });
                    self.advance_formed_unit(counters, index, unit, target);
                }
                TacticalUnitState::Routed => {
                    let nearest_enemy = before
                        .units(&self.units)
                        .filter(|candidate| {
                            counters.target_candidate_visits += 1;
                            candidate.side != unit.side
                                && candidate.state == TacticalUnitState::Formed
                        })
                        .min_by(|left, right| {
                            point_distance_squared(unit.position, left.position)
                                .cmp(&point_distance_squared(unit.position, right.position))
                                .then_with(|| left.id.cmp(&right.id))
                        })
                        .map(|enemy| enemy.position);
                    self.advance_routed_unit(counters, index, unit, nearest_enemy);
                }
                TacticalUnitState::Withdrawing { routed } => {
                    self.advance_withdrawing_unit(counters, index, unit, routed);
                }
                TacticalUnitState::Escaped { .. } | TacticalUnitState::Destroyed => {}
            }
        }
        counters.unit_scan_visits += self.units.len() as u64;
        let charging: Vec<usize> = before
            .units(&self.units)
            .enumerate()
            .filter_map(|(index, unit)| unit.charge.is_some().then_some(index))
            .collect();
        for index in charging {
            let charge = self.next_charge(counters, index, &before);
            self.units[index].charge = Some(charge);
        }
    }

    /// The charge state of a unit that carried one when movement began.
    fn next_charge(
        &self,
        counters: &mut TacticalWorkCounters,
        index: usize,
        before_units: &MovementBefore,
    ) -> crate::CavalryChargeState {
        use crate::CavalryChargeState as Charge;
        let before = before_units.unit(&self.units, index);
        let previous = before
            .charge
            .expect("only units with a charge state advance it");
        if before.state != TacticalUnitState::Formed {
            return Charge::Ready;
        }
        if let Charge::Recovering { ticks_remaining } = previous {
            return if ticks_remaining <= 1 {
                Charge::Ready
            } else {
                Charge::Recovering {
                    ticks_remaining: ticks_remaining - 1,
                }
            };
        }
        let target = before
            .engagement_target
            .as_deref()
            .and_then(|id| before_units.find(&self.units, id, counters))
            .filter(|target| target.state == TacticalUnitState::Formed);
        let Some(target) = target else {
            return previous.interrupted();
        };
        let next = self.units[index].position;
        let target_position = find_unit(&self.units, &target.id, counters)
            .expect("movement preserves units")
            .position;
        let intercepted = self.units.iter().any(|other| {
            counters.target_candidate_visits += 1;
            other.side != before.side
                && other.id != target.id
                && other.state == TacticalUnitState::Formed
                && measured_points_within_distance(
                    counters,
                    next,
                    other.position,
                    COMBAT_CONTACT_DISTANCE_MM,
                )
        });
        let straight =
            before.facing.is_some_and(|facing| {
                facing.classify(before.position, target.position) == crate::CombatArc::Front
            }) && self.measured_movement_waypoint(counters, before.position, target.position)
                == target.position
                && self
                    .terrain
                    .ground_cover_at(self.battlefield, before.position)
                    == crate::TacticalGroundCover::Open
                && self.terrain.ground_cover_at(self.battlefield, next)
                    == crate::TacticalGroundCover::Open;
        if intercepted || !straight {
            return previous.interrupted();
        }
        let displacement = next
            .x_mm
            .abs_diff(before.position.x_mm)
            .max(next.y_mm.abs_diff(before.position.y_mm));
        let run_up_mm = previous
            .run_up_mm()
            .saturating_add(displacement)
            .min(crate::charge::CHARGE_RUN_UP_MM);
        if measured_points_within_distance(
            counters,
            next,
            target_position,
            COMBAT_CONTACT_DISTANCE_MM,
        ) {
            return Charge::Contact { run_up_mm };
        }
        if next == before.position
            && !measured_points_within_distance(
                counters,
                before.position,
                target.position,
                COMBAT_CONTACT_DISTANCE_MM,
            )
        {
            return previous.interrupted();
        }
        if run_up_mm >= crate::charge::CHARGE_RUN_UP_MM {
            Charge::Charging { run_up_mm }
        } else {
            Charge::Approaching { run_up_mm }
        }
    }

    fn advance_formed_unit(
        &mut self,
        counters: &mut TacticalWorkCounters,
        index: usize,
        unit: &TacticalUnit,
        target: Option<TargetView>,
    ) {
        if let Some(target) = target
            && target.is_pursuit_target()
            && !measured_points_within_distance(
                counters,
                unit.position,
                target.position,
                PURSUIT_DISTANCE_MM,
            )
        {
            self.units[index].engagement_target = None;
            self.units[index].fatigue = self.units[index]
                .fatigue
                .saturating_sub(IDLE_FATIGUE_RECOVERY_PER_TICK);
            return;
        }

        let destination = if unit.movement_mode == crate::MovementMode::AttackMove {
            target.map(|target| target.position).or(unit.destination)
        } else {
            unit.destination
                .or_else(|| target.map(|target| target.position))
        };
        let Some(destination) = destination else {
            self.units[index].fatigue = self.units[index]
                .fatigue
                .saturating_sub(IDLE_FATIGUE_RECOVERY_PER_TICK);
            return;
        };

        let engagement_stop_distance = target
            .filter(|target| target.state == TacticalUnitState::Formed)
            .map_or(COMBAT_CONTACT_DISTANCE_MM, |_| unit.attack_range_mm());
        if unit.engagement_target.is_some()
            && measured_points_within_distance(
                counters,
                unit.position,
                destination,
                engagement_stop_distance,
            )
        {
            self.units[index].fatigue = self.units[index]
                .fatigue
                .saturating_sub(IDLE_FATIGUE_RECOVERY_PER_TICK);
            return;
        }

        let base_movement_speed = if target.is_some_and(|target| target.is_pursuit_target()) {
            unit.speed_mm_per_tick.saturating_mul(2)
        } else {
            unit.speed_mm_per_tick
        };
        counters.terrain_speed_queries += 1;
        let movement_speed = self.terrain().movement_speed_mm_per_tick(
            self.battlefield,
            unit.position,
            base_movement_speed,
        );
        let waypoint = self.measured_movement_waypoint(counters, unit.position, destination);
        let next = move_point_toward(unit.position, waypoint, movement_speed);
        debug_assert!(self.is_passable_at(next));
        if unit.facing.is_some() && next != unit.position {
            self.units[index].facing = crate::Facing::toward(unit.position, next);
        }
        self.units[index].position = next;
        if self.units[index].destination == Some(destination) && next == destination {
            self.units[index].destination = None;
            if let Some(facing) = self.units[index].arrival_facing.take() {
                self.units[index].facing = Some(facing);
            }
        }
        if next != unit.position {
            self.units[index].fatigue = self.units[index]
                .fatigue
                .saturating_add(MOVEMENT_FATIGUE_PER_TICK)
                .min(MAX_TACTICAL_FATIGUE);
        }
    }

    fn advance_withdrawing_unit(
        &mut self,
        counters: &mut TacticalWorkCounters,
        index: usize,
        unit: &TacticalUnit,
        routed: bool,
    ) {
        let edge = match unit.side {
            BattleSide::Attacker => 0,
            BattleSide::Defender => self.battlefield.width_mm,
        };
        let destination = BattlePoint::new(edge, unit.position.y_mm);
        let base_speed = if routed {
            unit.speed_mm_per_tick.saturating_mul(2)
        } else {
            unit.speed_mm_per_tick
        };
        counters.terrain_speed_queries += 1;
        let speed =
            self.terrain
                .movement_speed_mm_per_tick(self.battlefield, unit.position, base_speed);
        let waypoint = self.measured_movement_waypoint(counters, unit.position, destination);
        let next = move_point_toward(unit.position, waypoint, speed);
        debug_assert!(self.is_passable_at(next));
        let before_position = unit.position;
        let unit = &mut self.units[index];
        unit.position = next;
        if next.x_mm == edge {
            unit.state = TacticalUnitState::Escaped { routed };
        } else if next != before_position {
            unit.fatigue = unit
                .fatigue
                .saturating_add(if routed {
                    ROUT_FATIGUE_PER_TICK
                } else {
                    MOVEMENT_FATIGUE_PER_TICK
                })
                .min(MAX_TACTICAL_FATIGUE);
        }
    }

    fn advance_routed_unit(
        &mut self,
        counters: &mut TacticalWorkCounters,
        index: usize,
        unit: &TacticalUnit,
        nearest_enemy: Option<BattlePoint>,
    ) {
        let Some(enemy_position) = nearest_enemy else {
            return;
        };
        counters.terrain_speed_queries += 1;
        let route_speed = self.terrain().movement_speed_mm_per_tick(
            self.battlefield,
            unit.position,
            unit.speed_mm_per_tick.saturating_mul(2),
        );
        let desired = move_point_away(
            unit.position,
            enemy_position,
            route_speed,
            self.battlefield,
            unit.side,
        );
        let waypoint = self.measured_movement_waypoint(counters, unit.position, desired);
        let next = move_point_toward(unit.position, waypoint, route_speed);
        debug_assert!(self.is_passable_at(next));
        self.units[index].position = next;
        if next != unit.position {
            self.units[index].fatigue = self.units[index]
                .fatigue
                .saturating_add(ROUT_FATIGUE_PER_TICK)
                .min(MAX_TACTICAL_FATIGUE);
        }
    }

    /// Resolves one pulse against the pulse-start unit state. Units are only
    /// read until casualties are applied (spent ammunition is applied after
    /// the volley and melee loops), so the pulse reads live units without a snapshot.
    fn resolve_combat_pulse(&mut self, counters: &mut TacticalWorkCounters) {
        counters.combat_pulses += 1;
        let snapshot = &self.units;
        let mut pairs = BTreeSet::new();
        // Pair collection, the volley loop, and charge recovery each visit
        // every unit once.
        counters.unit_scan_visits += 3 * snapshot.len() as u64;
        for unit in snapshot {
            if unit.state != TacticalUnitState::Formed {
                continue;
            }
            let Some(target_id) = unit.engagement_target.as_deref() else {
                continue;
            };
            let Some(target) = find_work_target(snapshot, target_id, counters) else {
                continue;
            };
            if target.is_destroyed() || target.is_escaped() || target.side == unit.side {
                continue;
            }
            let pair = if unit.id < target.id {
                (unit.id.clone(), target.id.clone())
            } else {
                (target.id.clone(), unit.id.clone())
            };
            pairs.insert(pair);
        }

        counters.engagement_pairs += pairs.len() as u64;
        let formed_contacts = pairs
            .iter()
            .filter(|(left_id, right_id)| {
                let left = find_work_target(snapshot, left_id, counters).unwrap();
                let right = find_work_target(snapshot, right_id, counters).unwrap();
                left.state == TacticalUnitState::Formed
                    && right.state == TacticalUnitState::Formed
                    && measured_points_within_distance(
                        counters,
                        left.position,
                        right.position,
                        COMBAT_CONTACT_DISTANCE_MM,
                    )
            })
            .fold(
                BTreeMap::<String, u16>::new(),
                |mut contacts, (left, right)| {
                    *contacts.entry(left.clone()).or_default() += 1;
                    *contacts.entry(right.clone()).or_default() += 1;
                    contacts
                },
            );

        let mut casualties: BTreeMap<String, u32> = BTreeMap::new();
        let mut engaged_units = BTreeSet::new();
        let mut consumed_charges = BTreeSet::new();

        let mut spent_ammunition = Vec::new();
        for attacker in snapshot {
            if attacker.state != TacticalUnitState::Formed
                || attacker.attack_range_mm() <= COMBAT_CONTACT_DISTANCE_MM
                || formed_contacts.contains_key(&attacker.id)
            {
                continue;
            }
            let Some(target_id) = attacker.engagement_target.as_deref() else {
                continue;
            };
            let Some(target) = find_work_target(snapshot, target_id, counters) else {
                continue;
            };
            if target.state != TacticalUnitState::Formed || target.side == attacker.side {
                continue;
            }
            if measured_points_within_distance(
                counters,
                attacker.position,
                target.position,
                COMBAT_CONTACT_DISTANCE_MM,
            ) || !measured_points_within_distance(
                counters,
                attacker.position,
                target.position,
                attacker.attack_range_mm(),
            ) {
                continue;
            }
            if attacker.ammunition.is_some() {
                spent_ammunition.push(attacker.id.clone());
            }
            counters.ranged_volleys += 1;
            let losses = if attacker.stats().is_some() || target.stats().is_some() {
                let damage = ranged_damage_milli(attacker, target, self.terrain, self.battlefield);
                let credit = self
                    .ranged_damage_credit
                    .entry(attacker.id.clone())
                    .or_default()
                    .entry(target.id.clone())
                    .or_default();
                let total = damage.saturating_add(u32::from(credit.0));
                credit.0 = u16::try_from(total % 1_000).expect("fractional damage fits u16");
                if damage > 0 {
                    engaged_units.insert(attacker.id.clone());
                }
                u16::try_from((total / 1_000).min(u32::from(target.soldiers)))
                    .expect("casualties are bounded by target soldiers")
            } else {
                ranged_casualties(attacker, target, self.terrain, self.battlefield)
            };
            if losses > 0 {
                *casualties.entry(target.id.clone()).or_default() += u32::from(losses);
                engaged_units.insert(attacker.id.clone());
            }
        }

        let mut contacts_assigned = BTreeMap::<String, u16>::new();
        for (left_id, right_id) in pairs {
            let left = find_work_target(snapshot, &left_id, counters).unwrap();
            let right = find_work_target(snapshot, &right_id, counters).unwrap();
            match (left.state, right.state) {
                (TacticalUnitState::Formed, TacticalUnitState::Formed)
                    if measured_points_within_distance(
                        counters,
                        left.position,
                        right.position,
                        COMBAT_CONTACT_DISTANCE_MM,
                    ) =>
                {
                    counters.melee_contacts += 1;
                    let left_frontage = allocated_frontage(
                        left,
                        formed_contacts[&left.id],
                        contacts_assigned.entry(left.id.clone()).or_default(),
                    );
                    let right_frontage = allocated_frontage(
                        right,
                        formed_contacts[&right.id],
                        contacts_assigned.entry(right.id.clone()).or_default(),
                    );
                    let left_losses = melee_casualties_with_credit(
                        right,
                        left,
                        right_frontage,
                        self.terrain,
                        self.battlefield,
                        &mut self.melee_damage_credit,
                    );
                    let right_losses = melee_casualties_with_credit(
                        left,
                        right,
                        left_frontage,
                        self.terrain,
                        self.battlefield,
                        &mut self.melee_damage_credit,
                    );
                    *casualties.entry(left.id.clone()).or_default() += u32::from(left_losses);
                    *casualties.entry(right.id.clone()).or_default() += u32::from(right_losses);
                    if left_frontage > 0
                        && left.engagement_target.as_deref() == Some(right.id.as_str())
                    {
                        consumed_charges.insert(left.id.clone());
                    }
                    if right_frontage > 0
                        && right.engagement_target.as_deref() == Some(left.id.as_str())
                    {
                        consumed_charges.insert(right.id.clone());
                    }
                    engaged_units.insert(left.id.clone());
                    engaged_units.insert(right.id.clone());
                }
                (
                    TacticalUnitState::Formed,
                    TacticalUnitState::Routed | TacticalUnitState::Withdrawing { .. },
                ) if left.engagement_target.as_deref() == Some(right.id.as_str())
                    && measured_points_within_distance(
                        counters,
                        left.position,
                        right.position,
                        PURSUIT_DISTANCE_MM,
                    ) =>
                {
                    counters.pursuit_contacts += 1;
                    *casualties.entry(right.id.clone()).or_default() += u32::from(
                        pursuit_casualties(left, right, self.terrain, self.battlefield),
                    );
                    engaged_units.insert(left.id.clone());
                }
                (
                    TacticalUnitState::Routed | TacticalUnitState::Withdrawing { .. },
                    TacticalUnitState::Formed,
                ) if right.engagement_target.as_deref() == Some(left.id.as_str())
                    && measured_points_within_distance(
                        counters,
                        left.position,
                        right.position,
                        PURSUIT_DISTANCE_MM,
                    ) =>
                {
                    counters.pursuit_contacts += 1;
                    *casualties.entry(left.id.clone()).or_default() += u32::from(
                        pursuit_casualties(right, left, self.terrain, self.battlefield),
                    );
                    engaged_units.insert(right.id.clone());
                }
                _ => {}
            }
        }

        for attacker_id in spent_ammunition {
            if let Some(index) = self.unit_index(&attacker_id)
                && let Some(ammunition) = &mut self.units[index].ammunition
            {
                ammunition.0 = ammunition.0.saturating_sub(1);
            }
        }

        for unit_id in engaged_units {
            if let Some(index) = self.unit_index(&unit_id) {
                self.units[index].fatigue = self.units[index]
                    .fatigue
                    .saturating_add(COMBAT_FATIGUE_PER_PULSE)
                    .min(MAX_TACTICAL_FATIGUE);
            }
        }

        for (unit_id, requested_losses) in casualties {
            let Some(index) = self.unit_index(&unit_id) else {
                continue;
            };
            let before = self.units[index].soldiers;
            if before == 0 || self.units[index].state == TacticalUnitState::Destroyed {
                continue;
            }
            let applied = u16::try_from(requested_losses.min(u32::from(before))).unwrap();
            if self.units[index].is_pursuit_target() {
                self.units[index].pursuit_casualties =
                    self.units[index].pursuit_casualties.saturating_add(applied);
            }
            self.units[index].soldiers -= applied;
            if self.units[index].soldiers == 0 {
                self.units[index].morale = 0;
                self.units[index].state = TacticalUnitState::Destroyed;
                self.units[index].destination = None;
                self.units[index].arrival_facing = None;
                self.units[index].queued_movements.clear();
                self.units[index].movement_mode = crate::MovementMode::March;
                self.units[index].engagement_target = None;
                continue;
            }

            if self.units[index].can_receive_orders() || self.units[index].is_withdrawing() {
                let shock = casualty_morale_shock(before, applied);
                self.units[index].morale = self.units[index].morale.saturating_sub(shock);
                if self.units[index].morale <= ROUT_MORALE_THRESHOLD {
                    self.units[index].state = if self.units[index].is_withdrawing() {
                        TacticalUnitState::Withdrawing { routed: true }
                    } else {
                        TacticalUnitState::Routed
                    };
                    self.units[index].destination = None;
                    self.units[index].arrival_facing = None;
                    self.units[index].queued_movements.clear();
                    self.units[index].movement_mode = crate::MovementMode::March;
                    self.units[index].engagement_target = None;
                }
            }
        }
        for unit in &mut self.units {
            if consumed_charges.contains(&unit.id)
                && matches!(unit.charge, Some(crate::CavalryChargeState::Contact { .. }))
            {
                unit.charge = Some(crate::CavalryChargeState::Recovering {
                    ticks_remaining: crate::charge::CHARGE_RECOVERY_TICKS,
                });
            }
        }
    }

    fn clear_invalid_engagement_targets(&mut self, counters: &mut TacticalWorkCounters) {
        let units = &self.units;
        let invalid: Vec<usize> = (0..units.len())
            .filter(|&index| {
                let unit = &units[index];
                let Some(target_id) = unit.engagement_target.as_deref() else {
                    return false;
                };
                unit.state != TacticalUnitState::Formed
                    || find_unit(units, target_id, counters).is_some_and(|target| {
                        matches!(
                            target.state,
                            TacticalUnitState::Destroyed | TacticalUnitState::Escaped { .. }
                        )
                    })
            })
            .collect();
        for index in invalid {
            self.units[index].engagement_target = None;
        }
    }
}

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
        }
    }
}

impl std::error::Error for TacticalError {}

fn allocated_frontage(attacker: &TacticalUnit, contacts: u16, assigned: &mut u16) -> u16 {
    let frontage = attacker.frontage_slots();
    let allocation = if contacts == 0 || *assigned >= contacts {
        0
    } else {
        frontage / contacts + u16::from(*assigned < frontage % contacts)
    };
    *assigned = assigned.saturating_add(1);
    allocation
}

fn terrain_adjusted_frontage(
    effective_frontage: u32,
    attacker: &TacticalUnit,
    defender: &TacticalUnit,
    terrain: TacticalTerrain,
    battlefield: FlatBattlefield,
) -> u32 {
    effective_frontage.saturating_mul(terrain.elevation_damage_factor_milli(
        battlefield,
        attacker.position,
        defender.position,
    )) / COMBAT_FACTOR_BASE_MILLI
}

fn melee_casualties_with_credit(
    attacker: &TacticalUnit,
    defender: &TacticalUnit,
    frontage: u16,
    terrain: TacticalTerrain,
    battlefield: FlatBattlefield,
    credits: &mut BTreeMap<String, BTreeMap<String, CombatDamageCredit>>,
) -> u16 {
    if attacker.stats().is_none() && defender.stats().is_none() {
        return melee_casualties(attacker, defender, frontage, terrain, battlefield);
    }
    if attacker.state != TacticalUnitState::Formed || defender.soldiers == 0 || frontage == 0 {
        return 0;
    }
    let fatigue = 1_000_u64.saturating_sub(u64::from(attacker.fatigue) / 2);
    let morale = 750_u64 + u64::from(attacker.morale) / 4;
    let damage = u64::from(frontage) * 1_000 * fatigue * morale / 1_000_000;
    let damage = damage
        * u64::from(terrain.elevation_damage_factor_milli(
            battlefield,
            attacker.position,
            defender.position,
        ))
        / 1_000;
    let attack = attacker
        .stats()
        .map_or(1_000, |stats| u64::from(stats.melee_attack_milli));
    let resistance = defender.stats().map_or(1_000, |stats| {
        u64::from(stats.defense_milli) + u64::from(stats.armor_milli)
    });
    let damage = damage * attack / resistance;
    let damage = damage * u64::from(contact_factor_milli(attacker, defender)) / 1_000;
    let damage = damage * u64::from(charge_factor_milli(attacker, defender)) / 1_000;
    let damage = damage * u64::from(matchup_factor_milli(attacker, defender))
        / 1_000
        / u64::from(MELEE_CASUALTY_DIVISOR);
    let credit = credits
        .entry(attacker.id.clone())
        .or_default()
        .entry(defender.id.clone())
        .or_default();
    let total = damage + u64::from(credit.0);
    credit.0 = u16::try_from(total % 1_000).expect("fractional damage fits u16");
    u16::try_from((total / 1_000).min(u64::from(defender.soldiers)))
        .expect("casualties are bounded by soldiers")
}

fn melee_casualties(
    attacker: &TacticalUnit,
    defender: &TacticalUnit,
    frontage: u16,
    terrain: TacticalTerrain,
    battlefield: FlatBattlefield,
) -> u16 {
    if attacker.state != TacticalUnitState::Formed || defender.soldiers == 0 || frontage == 0 {
        return 0;
    }
    let frontage = u32::from(frontage);
    let fatigue_factor = 1_000_u32.saturating_sub(u32::from(attacker.fatigue) / 2);
    let morale_factor = 750_u32.saturating_add(u32::from(attacker.morale) / 4);
    let effective_frontage = frontage
        .saturating_mul(fatigue_factor)
        .saturating_mul(morale_factor)
        / 1_000_000;
    let effective_frontage =
        terrain_adjusted_frontage(effective_frontage, attacker, defender, terrain, battlefield);
    let effective_frontage = stat_adjusted_frontage(effective_frontage, attacker, defender, false);
    let effective_frontage =
        effective_frontage.saturating_mul(contact_factor_milli(attacker, defender)) / 1_000;
    let effective_frontage =
        effective_frontage.saturating_mul(matchup_factor_milli(attacker, defender)) / 1_000;
    let losses = (effective_frontage / MELEE_CASUALTY_DIVISOR).max(1);
    u16::try_from(losses.min(u32::from(defender.soldiers))).unwrap()
}

fn ranged_casualties(
    attacker: &TacticalUnit,
    defender: &TacticalUnit,
    terrain: TacticalTerrain,
    battlefield: FlatBattlefield,
) -> u16 {
    if attacker.state != TacticalUnitState::Formed || defender.soldiers == 0 {
        return 0;
    }
    let frontage = u32::from(attacker.frontage_slots());
    let fatigue_factor = 1_000_u32.saturating_sub(u32::from(attacker.fatigue) / 2);
    let morale_factor = 750_u32.saturating_add(u32::from(attacker.morale) / 4);
    let effective_frontage = frontage
        .saturating_mul(fatigue_factor)
        .saturating_mul(morale_factor)
        / 1_000_000;
    let effective_frontage =
        terrain_adjusted_frontage(effective_frontage, attacker, defender, terrain, battlefield)
            .saturating_mul(
                terrain.ranged_target_damage_factor_milli(battlefield, defender.position),
            )
            / COMBAT_FACTOR_BASE_MILLI;
    let effective_frontage = stat_adjusted_frontage(effective_frontage, attacker, defender, true);
    let losses = (effective_frontage / RANGED_CASUALTY_DIVISOR).max(1);
    u16::try_from(losses.min(u32::from(defender.soldiers))).unwrap()
}

fn charge_factor_milli(attacker: &TacticalUnit, defender: &TacticalUnit) -> u32 {
    if !matches!(attacker.charge, Some(crate::CavalryChargeState::Contact { run_up_mm }) if run_up_mm >= crate::charge::CHARGE_RUN_UP_MM)
        || attacker.engagement_target.as_deref() != Some(defender.id.as_str())
        || (defender
            .combat_profile()
            .is_some_and(|profile| profile.kind == crate::UnitKind::Spearmen)
            && defender.incoming_arc(attacker.position) == Some(crate::CombatArc::Front))
    {
        return 1_000;
    }
    1_000
        + attacker
            .stats()
            .map_or(0, |stats| u32::from(stats.charge_impact_milli))
}

// Only explicit combat profiles participate; historical kind metadata is visual.
fn matchup_factor_milli(attacker: &TacticalUnit, defender: &TacticalUnit) -> u32 {
    let (Some(attacker_profile), Some(defender_profile)) =
        (attacker.combat_profile(), defender.combat_profile())
    else {
        return 1_000;
    };
    match (attacker_profile.kind, defender_profile.kind) {
        (crate::UnitKind::Spearmen, crate::UnitKind::Knights)
            if attacker.incoming_arc(defender.position) == Some(crate::CombatArc::Front) =>
        {
            1_750
        }
        (crate::UnitKind::Knights, crate::UnitKind::Spearmen)
            if defender.incoming_arc(attacker.position) == Some(crate::CombatArc::Front) =>
        {
            600
        }
        (crate::UnitKind::Knights, crate::UnitKind::Levy | crate::UnitKind::Archers)
            if matches!(
                defender.incoming_arc(attacker.position),
                Some(crate::CombatArc::Flank | crate::CombatArc::Rear)
            ) =>
        {
            1_250
        }
        _ => 1_000,
    }
}

fn contact_factor_milli(attacker: &TacticalUnit, defender: &TacticalUnit) -> u32 {
    let bonus = match defender.incoming_arc(attacker.position) {
        Some(crate::CombatArc::Flank) => 250_u32,
        Some(crate::CombatArc::Rear) => 500_u32,
        Some(crate::CombatArc::Front) | None => return 1_000,
    };
    let resistance = defender
        .stats()
        .map_or(1_000, |stats| u32::from(stats.formation_resistance_milli));
    1_000 + (bonus * 1_000 / resistance).min(750)
}

fn ranged_damage_milli(
    attacker: &TacticalUnit,
    defender: &TacticalUnit,
    terrain: TacticalTerrain,
    battlefield: FlatBattlefield,
) -> u32 {
    let fatigue = 1_000_u64.saturating_sub(u64::from(attacker.fatigue) / 2);
    let morale = 750_u64 + u64::from(attacker.morale) / 4;
    let frontage = u64::from(attacker.frontage_slots()) * 1_000 * fatigue * morale / 1_000_000;
    let frontage = frontage
        * u64::from(terrain.elevation_damage_factor_milli(
            battlefield,
            attacker.position,
            defender.position,
        ))
        / 1_000;
    let frontage = frontage
        * u64::from(terrain.ranged_target_damage_factor_milli(battlefield, defender.position))
        / 1_000;
    let attack = attacker
        .stats()
        .and_then(|stats| stats.missile)
        .map_or(1_000, |missile| u64::from(missile.damage_milli));
    let resistance = 1_000
        + defender
            .stats()
            .map_or(0, |stats| u64::from(stats.armor_milli));
    u32::try_from(frontage * attack / resistance / u64::from(RANGED_CASUALTY_DIVISOR))
        .expect("bounded frontage and stat factors fit u32 damage")
}

fn stat_adjusted_frontage(
    frontage: u32,
    attacker: &TacticalUnit,
    defender: &TacticalUnit,
    ranged: bool,
) -> u32 {
    let attack = attacker.stats().map_or(1_000, |stats| {
        if ranged {
            stats
                .missile
                .map_or(1_000, |missile| u32::from(missile.damage_milli))
        } else {
            u32::from(stats.melee_attack_milli)
        }
    });
    let resistance = defender.stats().map_or(1_000, |stats| {
        (if ranged {
            1_000
        } else {
            u32::from(stats.defense_milli)
        }) + u32::from(stats.armor_milli)
    });
    frontage.saturating_mul(attack) / resistance
}

fn pursuit_casualties(
    pursuer: &TacticalUnit,
    target: &TacticalUnit,
    terrain: TacticalTerrain,
    battlefield: FlatBattlefield,
) -> u16 {
    let frontage = u32::from(pursuer.frontage_slots());
    let fatigue_factor = 1_000_u32.saturating_sub(u32::from(pursuer.fatigue) / 2);
    let effective_frontage = frontage.saturating_mul(fatigue_factor) / 1_000;
    let effective_frontage =
        terrain_adjusted_frontage(effective_frontage, pursuer, target, terrain, battlefield);
    u16::try_from((effective_frontage / PURSUIT_CASUALTY_DIVISOR).max(1)).unwrap()
}

fn casualty_morale_shock(before: u16, casualties: u16) -> u16 {
    if before == 0 || casualties == 0 {
        return 0;
    }
    let casualty_ratio_milli = u32::from(casualties) * 1_000 / u32::from(before);
    let shock = u32::from(casualties) * 8 + casualty_ratio_milli / 2;
    u16::try_from(shock.min(u32::from(MAX_TACTICAL_MORALE))).unwrap()
}

fn move_point_toward(current: BattlePoint, destination: BattlePoint, speed_mm: u32) -> BattlePoint {
    if current == destination {
        return destination;
    }
    let dx = i64::from(destination.x_mm) - i64::from(current.x_mm);
    let dy = i64::from(destination.y_mm) - i64::from(current.y_mm);
    let distance_squared = squared_components(dx, dy);
    let speed = u128::from(speed_mm);
    if distance_squared <= speed * speed {
        return destination;
    }

    let (step_x, step_y) = step_vector(dx, dy, speed_mm);
    BattlePoint::new(
        u32::try_from(i64::from(current.x_mm) + step_x)
            .expect("movement x remains between current position and target"),
        u32::try_from(i64::from(current.y_mm) + step_y)
            .expect("movement y remains between current position and target"),
    )
}

fn move_point_away(
    current: BattlePoint,
    enemy: BattlePoint,
    speed_mm: u32,
    battlefield: FlatBattlefield,
    side: BattleSide,
) -> BattlePoint {
    let mut dx = i64::from(current.x_mm) - i64::from(enemy.x_mm);
    let dy = i64::from(current.y_mm) - i64::from(enemy.y_mm);
    if dx == 0 && dy == 0 {
        dx = match side {
            BattleSide::Attacker => -1,
            BattleSide::Defender => 1,
        };
    }
    let (step_x, step_y) = step_vector(dx, dy, speed_mm);
    let x = (i64::from(current.x_mm) + step_x).clamp(0, i64::from(battlefield.width_mm));
    let y = (i64::from(current.y_mm) + step_y).clamp(0, i64::from(battlefield.depth_mm));
    BattlePoint::new(u32::try_from(x).unwrap(), u32::try_from(y).unwrap())
}

fn step_vector(dx: i64, dy: i64, speed_mm: u32) -> (i64, i64) {
    let distance = integer_sqrt_ceil(squared_components(dx, dy));
    let divisor = i128::try_from(distance).expect("movement distance must fit in i128");
    let speed = i128::from(speed_mm);
    let mut step_x =
        i64::try_from(i128::from(dx) * speed / divisor).expect("movement x step must fit in i64");
    let mut step_y =
        i64::try_from(i128::from(dy) * speed / divisor).expect("movement y step must fit in i64");

    if step_x == 0 && step_y == 0 {
        if dx.unsigned_abs() >= dy.unsigned_abs() {
            step_x = dx.signum();
        } else {
            step_y = dy.signum();
        }
    }
    (step_x, step_y)
}

/// Uses the shared physics kernel for exact deterministic tactical proximity.
///
/// Tactical positions use the full `u32` battlefield range while the physics kernel's public
/// vectors are compact `i32` coordinates. Contact is translation invariant, so rebase the left
/// point to the local origin and map the battle ground plane onto physics X/Z. The cheap axis
/// rejection keeps every converted delta inside the requested contact radius.
#[cfg(test)]
fn points_within_distance(left: BattlePoint, right: BattlePoint, distance_mm: u32) -> bool {
    measured_points_within_distance(
        &mut TacticalWorkCounters::default(),
        left,
        right,
        distance_mm,
    )
}

fn measured_points_within_distance(
    counters: &mut TacticalWorkCounters,
    left: BattlePoint,
    right: BattlePoint,
    distance_mm: u32,
) -> bool {
    counters.proximity_queries += 1;
    let radius =
        i32::try_from(distance_mm).expect("tactical contact radius must fit physics Vec3i");
    let dx = i64::from(right.x_mm) - i64::from(left.x_mm);
    let dz = i64::from(right.y_mm) - i64::from(left.y_mm);
    let distance = u64::from(distance_mm);
    if dx.unsigned_abs() > distance || dz.unsigned_abs() > distance {
        return false;
    }

    counters.physics_contact_queries += 1;
    let offset = Vec3i::new(
        i32::try_from(dx).expect("contact x delta is bounded by the tactical radius"),
        0,
        i32::try_from(dz).expect("contact z delta is bounded by the tactical radius"),
    );
    collider_contact(
        Collider::new(Vec3i::ZERO, ColliderShape::sphere(radius)),
        Collider::new(offset, ColliderShape::sphere(0)),
    )
    .expect("validated tactical contact geometry must be representable")
    .overlaps()
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

/// Whether a unit's movement step can change it. Idle formed units without
/// fatigue to recover, and escaped or destroyed units, are left untouched.
fn movement_may_change(unit: &TacticalUnit) -> bool {
    match unit.state {
        TacticalUnitState::Formed => {
            unit.destination.is_some() || unit.engagement_target.is_some() || unit.fatigue > 0
        }
        TacticalUnitState::Routed | TacticalUnitState::Withdrawing { .. } => true,
        TacticalUnitState::Escaped { .. } | TacticalUnitState::Destroyed => false,
    }
}

/// Copy-on-write view of the unit vector as it was when a movement phase
/// began. Units are saved in increasing index order right before their own
/// step; every other unit is still unchanged in the live vector.
#[derive(Default)]
struct MovementBefore {
    saved: Vec<(usize, TacticalUnit)>,
}

impl MovementBefore {
    fn save(&mut self, index: usize, unit: TacticalUnit) {
        debug_assert!(self.saved.last().is_none_or(|(last, _)| *last < index));
        self.saved.push((index, unit));
    }

    fn saved_unit(&self, index: usize) -> &TacticalUnit {
        let (saved_index, unit) = self.saved.last().expect("unit was just saved");
        debug_assert_eq!(*saved_index, index);
        unit
    }

    /// Every unit's phase-start state in index order, merging the saved copies
    /// with the live vector in one linear pass (no per-unit search).
    fn units<'a>(&'a self, live: &'a [TacticalUnit]) -> impl Iterator<Item = &'a TacticalUnit> {
        let mut saved = self.saved.iter().peekable();
        live.iter().enumerate().map(move |(index, unit)| {
            match saved.next_if(|(saved_index, _)| *saved_index == index) {
                Some((_, copy)) => copy,
                None => unit,
            }
        })
    }

    /// One unit's phase-start state; a binary search over the saved copies,
    /// used for single lookups only.
    fn unit<'a>(&'a self, live: &'a [TacticalUnit], index: usize) -> &'a TacticalUnit {
        self.saved
            .binary_search_by_key(&index, |(saved, _)| *saved)
            .map_or(&live[index], |position| &self.saved[position].1)
    }

    fn find<'a>(
        &'a self,
        live: &'a [TacticalUnit],
        id: &str,
        counters: &mut TacticalWorkCounters,
    ) -> Option<&'a TacticalUnit> {
        counters.indexed_unit_lookups += 1;
        live.binary_search_by(|unit| unit.id.as_str().cmp(id))
            .ok()
            .map(|index| self.unit(live, index))
    }
}

/// The parts of an engagement target a formed unit's movement reads.
#[derive(Copy, Clone)]
struct TargetView {
    position: BattlePoint,
    state: TacticalUnitState,
}

impl TargetView {
    fn of(unit: &TacticalUnit) -> Self {
        Self {
            position: unit.position,
            state: unit.state,
        }
    }

    const fn is_pursuit_target(self) -> bool {
        matches!(
            self.state,
            TacticalUnitState::Routed | TacticalUnitState::Withdrawing { .. }
        )
    }
}

fn point_distance_squared(left: BattlePoint, right: BattlePoint) -> u128 {
    squared_components(
        i64::from(right.x_mm) - i64::from(left.x_mm),
        i64::from(right.y_mm) - i64::from(left.y_mm),
    )
}

#[cfg(test)]
const fn square_u32(value: u32) -> u128 {
    let value = value as u128;
    value * value
}

fn squared_components(dx: i64, dy: i64) -> u128 {
    let x = u128::from(dx.unsigned_abs());
    let y = u128::from(dy.unsigned_abs());
    x * x + y * y
}

fn integer_sqrt_ceil(value: u128) -> u128 {
    if value < 2 {
        return value;
    }

    let mut low = 1_u128;
    let mut high = value;
    let mut floor = 1_u128;
    while low <= high {
        let mid = low + (high - low) / 2;
        if mid <= value / mid {
            floor = mid;
            low = mid.saturating_add(1);
        } else {
            high = mid - 1;
        }
    }

    if floor * floor == value {
        floor
    } else {
        floor.saturating_add(1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_unit(
        id: &str,
        side: BattleSide,
        position: BattlePoint,
        formation: Formation,
    ) -> TacticalUnit {
        TacticalUnit::new(id, side, 80, position, formation, 1_200)
    }

    fn sample_battle() -> TacticalBattle {
        TacticalBattle::deploy(
            FlatBattlefield::new(300_000, 200_000),
            vec![
                sample_unit(
                    "attacker-spears",
                    BattleSide::Attacker,
                    BattlePoint::new(20_000, 100_000),
                    Formation::Line { files: 20 },
                ),
                sample_unit(
                    "defender-spears",
                    BattleSide::Defender,
                    BattlePoint::new(280_000, 100_000),
                    Formation::Line { files: 20 },
                ),
            ],
        )
        .unwrap()
    }

    fn contact_battle(
        attacker_formation: Formation,
        defender_formation: Formation,
    ) -> TacticalBattle {
        TacticalBattle::new(
            FlatBattlefield::new(50_000, 50_000),
            vec![
                sample_unit(
                    "attacker",
                    BattleSide::Attacker,
                    BattlePoint::new(24_500, 25_000),
                    attacker_formation,
                ),
                sample_unit(
                    "defender",
                    BattleSide::Defender,
                    BattlePoint::new(25_500, 25_000),
                    defender_formation,
                ),
            ],
        )
        .unwrap()
    }

    fn unit<'a>(battle: &'a TacticalBattle, id: &str) -> &'a TacticalUnit {
        battle.units().iter().find(|unit| unit.id() == id).unwrap()
    }

    fn engage(battle: &mut TacticalBattle, attacker: &str, defender: &str) {
        battle.issue_engagement_order(attacker, defender).unwrap();
    }

    #[test]
    fn physics_contact_preserves_tactical_distance_boundaries() {
        let origin = BattlePoint::new(0, 0);
        assert!(points_within_distance(
            origin,
            BattlePoint::new(900, 1_200),
            COMBAT_CONTACT_DISTANCE_MM,
        ));
        assert!(!points_within_distance(
            origin,
            BattlePoint::new(901, 1_200),
            COMBAT_CONTACT_DISTANCE_MM,
        ));
    }

    #[test]
    fn physics_contact_rebases_large_battlefield_coordinates() {
        let edge = BattlePoint::new(u32::MAX, u32::MAX);
        assert!(points_within_distance(
            BattlePoint::new(u32::MAX - 900, u32::MAX - 1_200),
            edge,
            COMBAT_CONTACT_DISTANCE_MM,
        ));
        assert!(!points_within_distance(
            BattlePoint::new(0, 0),
            edge,
            PURSUIT_DISTANCE_MM,
        ));
    }

    #[test]
    fn setup_validation_rejects_duplicate_ids_and_invalid_units() {
        let duplicate = TacticalBattle::new(
            FlatBattlefield::new(10_000, 10_000),
            vec![
                sample_unit(
                    "spears",
                    BattleSide::Attacker,
                    BattlePoint::new(1_000, 1_000),
                    Formation::Line { files: 10 },
                ),
                sample_unit(
                    "spears",
                    BattleSide::Defender,
                    BattlePoint::new(9_000, 9_000),
                    Formation::Line { files: 10 },
                ),
            ],
        );
        assert!(matches!(duplicate, Err(TacticalError::DuplicateUnitId(_))));

        let invalid_formation = TacticalBattle::new(
            FlatBattlefield::new(10_000, 10_000),
            vec![sample_unit(
                "spears",
                BattleSide::Attacker,
                BattlePoint::new(1_000, 1_000),
                Formation::Column { files: 0 },
            )],
        );
        assert!(matches!(
            invalid_formation,
            Err(TacticalError::InvalidFormation(_))
        ));

        let invalid_range = TacticalBattle::new(
            FlatBattlefield::new(10_000, 10_000),
            vec![
                sample_unit(
                    "archers",
                    BattleSide::Attacker,
                    BattlePoint::new(1_000, 1_000),
                    Formation::Line { files: 10 },
                )
                .with_attack_range_mm(COMBAT_CONTACT_DISTANCE_MM - 1),
            ],
        );
        assert!(matches!(
            invalid_range,
            Err(TacticalError::InvalidAttackRange { .. })
        ));
    }

    #[test]
    fn setup_rejects_units_on_blocked_river_cells() {
        let battle = TacticalBattle::new(
            FlatBattlefield::new(80_000, 80_000),
            vec![sample_unit(
                "river-unit",
                BattleSide::Attacker,
                BattlePoint::new(35_000, 15_000),
                Formation::Line { files: 10 },
            )],
        );
        assert!(matches!(
            battle,
            Err(TacticalError::UnitOnImpassableTerrain { .. })
        ));
    }

    #[test]
    fn deployment_validation_accepts_own_back_thirds_and_rejects_neutral_setup() {
        let battlefield = FlatBattlefield::new(90_000, 60_000);
        let deployed = TacticalBattle::deploy(
            battlefield,
            vec![
                sample_unit(
                    "attacker",
                    BattleSide::Attacker,
                    BattlePoint::new(30_000, 20_000),
                    Formation::Line { files: 10 },
                ),
                sample_unit(
                    "defender",
                    BattleSide::Defender,
                    BattlePoint::new(60_000, 40_000),
                    Formation::Line { files: 10 },
                ),
            ],
        )
        .unwrap();
        assert_eq!(
            deployed.deployment_zones(),
            standard_deployment_zones(battlefield)
        );

        let invalid = TacticalBattle::deploy(
            battlefield,
            vec![sample_unit(
                "attacker",
                BattleSide::Attacker,
                BattlePoint::new(45_000, 30_000),
                Formation::Line { files: 10 },
            )],
        );
        assert!(matches!(
            invalid,
            Err(TacticalError::UnitOutsideDeploymentZone {
                side: BattleSide::Attacker,
                ..
            })
        ));
    }

    #[test]
    fn siege_deployment_uses_siege_zones_and_roundtrips() {
        let battlefield = FlatBattlefield::new(100_000, 100_000);
        let battle = TacticalBattle::deploy_siege(
            battlefield,
            vec![
                sample_unit(
                    "attacker",
                    BattleSide::Attacker,
                    BattlePoint::new(20_000, 50_000),
                    Formation::Line { files: 10 },
                ),
                sample_unit(
                    "defender",
                    BattleSide::Defender,
                    BattlePoint::new(80_000, 50_000),
                    Formation::Line { files: 10 },
                ),
            ],
        )
        .unwrap();
        let [attacker_zone, defender_zone] = battle.deployment_zones();
        assert_eq!(attacker_zone.max_x_mm, 35_000);
        assert_eq!(defender_zone.min_x_mm, 65_000);
        assert!(battle.siege.is_some());
        assert_eq!(battle.siege_snapshot(), battle.siege);

        let encoded = serde_json::to_string(&battle).unwrap();
        let decoded: TacticalBattle = serde_json::from_str(&encoded).unwrap();
        assert_eq!(decoded, battle);

        let invalid = TacticalBattle::deploy_siege(
            battlefield,
            vec![sample_unit(
                "attacker",
                BattleSide::Attacker,
                BattlePoint::new(40_000, 50_000),
                Formation::Line { files: 10 },
            )],
        );
        assert!(matches!(
            invalid,
            Err(TacticalError::UnitOutsideDeploymentZone { .. })
        ));
    }

    #[test]
    fn field_battle_serialization_has_no_siege_drift() {
        let battle = sample_battle();
        let encoded = serde_json::to_value(&battle).unwrap();
        assert!(encoded.get("siege").is_none());
        assert!(battle.siege_snapshot().is_none());

        let decoded: TacticalBattle = serde_json::from_value(encoded).unwrap();
        assert_eq!(decoded, battle);
        assert!(decoded.siege.is_none());
    }

    #[test]
    fn closed_siege_gate_holds_orders_until_core_opens_it() {
        let battlefield = FlatBattlefield::new(80_000, 80_000);
        let destination = BattlePoint::new(70_000, 40_000);
        let mut battle = TacticalBattle::deploy_siege(
            battlefield,
            vec![sample_unit(
                "attacker",
                BattleSide::Attacker,
                BattlePoint::new(10_000, 40_000),
                Formation::Line { files: 10 },
            )],
        )
        .unwrap();
        battle
            .issue_move_order(MovementOrder {
                unit_id: "attacker".into(),
                destination,
            })
            .unwrap();

        battle.advance_ticks(200);
        let closed_position = unit(&battle, "attacker").position();
        let gate = battle.siege.unwrap().layout.gate;
        assert!(closed_position.x_mm < gate.min_x_mm);
        assert!(unit(&battle, "attacker").destination().is_some());

        battle.open_siege_gate().unwrap();
        battle.advance_ticks(200);
        assert_eq!(unit(&battle, "attacker").position(), destination);
        assert!(unit(&battle, "attacker").destination().is_none());
    }

    #[test]
    fn siege_capture_advances_on_combat_pulses_and_contention_fails_closed() {
        let battlefield = FlatBattlefield::new(100_000, 100_000);
        let mut siege = SiegeBattleState::test_siege(battlefield);
        siege.gate_state = SiegeGateState::Open;
        let point = siege.layout.capture_point.center;
        let mut uncontested = TacticalBattle::new(
            battlefield,
            vec![sample_unit(
                "attacker",
                BattleSide::Attacker,
                point,
                Formation::Line { files: 10 },
            )],
        )
        .unwrap();
        uncontested.siege = Some(siege);
        uncontested.advance_ticks(TACTICAL_TICKS_PER_SECOND * 10);
        assert_eq!(
            uncontested.siege.unwrap().capture.captured_by,
            Some(BattleSide::Attacker)
        );

        let mut contested = TacticalBattle::new(
            battlefield,
            vec![
                sample_unit(
                    "attacker",
                    BattleSide::Attacker,
                    point,
                    Formation::Line { files: 10 },
                ),
                sample_unit(
                    "defender",
                    BattleSide::Defender,
                    point,
                    Formation::Line { files: 10 },
                ),
            ],
        )
        .unwrap();
        contested.siege = Some(siege);
        contested.advance_ticks(TACTICAL_TICKS_PER_SECOND);
        assert_eq!(contested.siege.unwrap().capture.progress, 0);
    }

    #[test]
    fn forest_ground_cover_reduces_tick_movement_without_changing_orders() {
        let battlefield = FlatBattlefield::new(80_000, 80_000);
        let destination = BattlePoint::new(29_000, 15_000);
        let mut forest = TacticalBattle::new(
            battlefield,
            vec![sample_unit(
                "forest",
                BattleSide::Attacker,
                BattlePoint::new(25_000, 15_000),
                Formation::Line { files: 10 },
            )],
        )
        .unwrap();
        forest
            .issue_move_order(MovementOrder {
                unit_id: "forest".into(),
                destination,
            })
            .unwrap();
        forest.advance_ticks(1);
        assert_eq!(
            unit(&forest, "forest").position(),
            BattlePoint::new(25_600, 15_000)
        );
        assert_eq!(unit(&forest, "forest").destination(), Some(destination));

        let mut open = TacticalBattle::new(
            battlefield,
            vec![sample_unit(
                "open",
                BattleSide::Attacker,
                BattlePoint::new(5_000, 15_000),
                Formation::Line { files: 10 },
            )],
        )
        .unwrap();
        open.issue_move_order(MovementOrder {
            unit_id: "open".into(),
            destination: BattlePoint::new(15_000, 15_000),
        })
        .unwrap();
        open.advance_ticks(1);
        assert_eq!(
            unit(&open, "open").position(),
            BattlePoint::new(6_200, 15_000)
        );
    }

    #[test]
    fn rejected_orders_do_not_mutate_authoritative_state() {
        let mut battle = sample_battle();
        let before = battle.clone();
        assert!(matches!(
            battle.issue_move_order(MovementOrder {
                unit_id: "missing".into(),
                destination: BattlePoint::new(50_000, 50_000),
            }),
            Err(TacticalError::UnitNotFound(_))
        ));
        assert_eq!(battle, before);

        assert!(matches!(
            battle.issue_move_order(MovementOrder {
                unit_id: "attacker-spears".into(),
                destination: BattlePoint::new(300_001, 50_000),
            }),
            Err(TacticalError::DestinationOutOfBounds { .. })
        ));
        assert_eq!(battle, before);

        assert!(matches!(
            battle.issue_move_order(MovementOrder {
                unit_id: "attacker-spears".into(),
                destination: BattlePoint::new(130_000, 25_000),
            }),
            Err(TacticalError::DestinationImpassable { .. })
        ));
        assert_eq!(battle, before);
    }

    #[test]
    fn move_orders_route_through_crossing_without_entering_blocked_water() {
        let battlefield = FlatBattlefield::new(80_000, 80_000);
        let destination = BattlePoint::new(70_000, 10_000);
        let mut battle = TacticalBattle::new(
            battlefield,
            vec![sample_unit(
                "crossing",
                BattleSide::Attacker,
                BattlePoint::new(10_000, 10_000),
                Formation::Line { files: 10 },
            )],
        )
        .unwrap();
        battle
            .issue_move_order(MovementOrder {
                unit_id: "crossing".into(),
                destination,
            })
            .unwrap();

        let mut visited_crossing = false;
        for _ in 0..200 {
            battle.advance_ticks(1);
            let position = unit(&battle, "crossing").position();
            assert!(battle.terrain().is_passable_at(battlefield, position));
            if battle.terrain().river_crossing_cells().iter().any(|cell| {
                battle
                    .terrain()
                    .cell_bounds_mm(battlefield, cell.cell_x, cell.cell_z)
                    .is_some_and(|(x0, x1, z0, z1)| {
                        position.x_mm >= x0
                            && position.x_mm < x1
                            && position.y_mm >= z0
                            && position.y_mm < z1
                    })
            }) {
                visited_crossing = true;
            }
            if unit(&battle, "crossing").destination().is_none() {
                break;
            }
        }
        assert!(visited_crossing);
        assert_eq!(unit(&battle, "crossing").position(), destination);
    }

    #[test]
    fn engagement_across_river_uses_the_same_crossing_path() {
        let battlefield = FlatBattlefield::new(80_000, 80_000);
        let mut battle = TacticalBattle::new(
            battlefield,
            vec![
                sample_unit(
                    "attacker",
                    BattleSide::Attacker,
                    BattlePoint::new(10_000, 10_000),
                    Formation::Line { files: 10 },
                ),
                sample_unit(
                    "defender",
                    BattleSide::Defender,
                    BattlePoint::new(70_000, 10_000),
                    Formation::Line { files: 10 },
                ),
            ],
        )
        .unwrap();
        engage(&mut battle, "attacker", "defender");

        let mut visited_crossing = false;
        for _ in 0..100 {
            battle.advance_ticks(1);
            let position = unit(&battle, "attacker").position();
            assert!(battle.terrain().is_passable_at(battlefield, position));
            if position.x_mm >= 30_000 && position.x_mm < 40_000 && position.y_mm >= 30_000 {
                visited_crossing = true;
                break;
            }
        }
        assert!(visited_crossing);
    }

    #[test]
    fn formation_orders_change_core_formation_without_replacing_other_orders() {
        let mut battle = sample_battle();
        battle
            .issue_move_order(MovementOrder {
                unit_id: "attacker-spears".into(),
                destination: BattlePoint::new(80_000, 120_000),
            })
            .unwrap();
        battle
            .issue_formation_order("attacker-spears", Formation::Column { files: 20 })
            .unwrap();
        assert_eq!(
            unit(&battle, "attacker-spears").formation(),
            Formation::Column { files: 20 }
        );
        assert_eq!(
            unit(&battle, "attacker-spears").destination(),
            Some(BattlePoint::new(80_000, 120_000))
        );
    }

    #[test]
    fn ranged_units_hold_at_range_and_inflict_losses_before_contact() {
        let battlefield = FlatBattlefield::new(50_000, 50_000);
        let mut battle = TacticalBattle::new(
            battlefield,
            vec![
                sample_unit(
                    "archers",
                    BattleSide::Attacker,
                    BattlePoint::new(10_000, 25_000),
                    Formation::Line { files: 24 },
                )
                .with_attack_range_mm(20_000),
                sample_unit(
                    "defender",
                    BattleSide::Defender,
                    BattlePoint::new(25_000, 25_000),
                    Formation::Line { files: 20 },
                ),
            ],
        )
        .unwrap();
        engage(&mut battle, "archers", "defender");
        let start = unit(&battle, "archers").position();
        battle.advance_ticks(TACTICAL_TICKS_PER_SECOND);
        assert_eq!(unit(&battle, "archers").position(), start);
        assert_eq!(unit(&battle, "archers").attack_range_mm(), 20_000);
        assert!(unit(&battle, "defender").soldiers() < 80);
        assert!(!points_within_distance(
            unit(&battle, "archers").position(),
            unit(&battle, "defender").position(),
            COMBAT_CONTACT_DISTANCE_MM,
        ));
    }

    #[test]
    fn higher_ground_increases_melee_and_pursuit_effectiveness() {
        let terrain = TacticalTerrain::battlefield_foundation();
        let battlefield = FlatBattlefield::new(100_000, 100_000);
        let high = sample_unit(
            "high",
            BattleSide::Attacker,
            BattlePoint::new(55_000, 55_000),
            Formation::Line { files: 80 },
        );
        let low = sample_unit(
            "low",
            BattleSide::Defender,
            BattlePoint::new(5_000, 5_000),
            Formation::Line { files: 80 },
        );

        assert!(
            melee_casualties(&high, &low, 80, terrain, battlefield)
                > melee_casualties(&low, &high, 80, terrain, battlefield)
        );
        assert!(
            pursuit_casualties(&high, &low, terrain, battlefield)
                > pursuit_casualties(&low, &high, terrain, battlefield)
        );
    }

    #[test]
    fn forest_cover_reduces_ranged_losses_at_equal_elevation() {
        let terrain = TacticalTerrain::battlefield_foundation();
        let battlefield = FlatBattlefield::new(80_000, 80_000);
        let archers = sample_unit(
            "archers",
            BattleSide::Attacker,
            BattlePoint::new(15_000, 5_000),
            Formation::Line { files: 48 },
        );
        let open = sample_unit(
            "open",
            BattleSide::Defender,
            BattlePoint::new(15_000, 15_000),
            Formation::Line { files: 20 },
        );
        let forest = sample_unit(
            "forest",
            BattleSide::Defender,
            BattlePoint::new(25_000, 15_000),
            Formation::Line { files: 20 },
        );
        assert_eq!(
            terrain.height_mm(battlefield, open.position),
            terrain.height_mm(battlefield, forest.position)
        );
        assert!(
            ranged_casualties(&archers, &forest, terrain, battlefield)
                < ranged_casualties(&archers, &open, terrain, battlefield)
        );
    }

    #[test]
    fn legacy_units_without_attack_range_default_to_melee_contact() {
        let battle = sample_battle();
        let mut encoded = serde_json::to_value(&battle).unwrap();
        for unit in encoded["units"].as_array_mut().unwrap() {
            unit.as_object_mut().unwrap().remove("attackRangeMm");
        }
        let decoded: TacticalBattle = serde_json::from_value(encoded).unwrap();
        assert!(
            decoded
                .units()
                .iter()
                .all(|unit| unit.attack_range_mm() == COMBAT_CONTACT_DISTANCE_MM)
        );
    }

    #[test]
    fn fixed_tick_replay_is_deterministic_and_serializable() {
        let mut first = sample_battle();
        let mut second = sample_battle();
        let order = MovementOrder {
            unit_id: "attacker-spears".into(),
            destination: BattlePoint::new(100_000, 140_000),
        };
        first.issue_move_order(order.clone()).unwrap();
        second.issue_move_order(order).unwrap();
        first.advance_ticks(TACTICAL_TICKS_PER_SECOND * 7);
        second.advance_ticks(TACTICAL_TICKS_PER_SECOND * 7);
        assert_eq!(first.tick(), 140);
        assert_eq!(first, second);

        let encoded = serde_json::to_string(&first).unwrap();
        let decoded: TacticalBattle = serde_json::from_str(&encoded).unwrap();
        assert_eq!(decoded, first);
    }

    #[test]
    fn deserialization_restores_canonical_order_for_unit_lookup() {
        let battle = sample_battle();
        let mut encoded = serde_json::to_value(&battle).unwrap();
        encoded["units"].as_array_mut().unwrap().reverse();

        let mut decoded: TacticalBattle = serde_json::from_value(encoded).unwrap();
        decoded
            .issue_engagement_order("attacker-spears", "defender-spears")
            .unwrap();

        assert_eq!(decoded.units()[0].id(), "attacker-spears");
        assert_eq!(
            unit(&decoded, "attacker-spears").engagement_target(),
            Some("defender-spears")
        );
    }

    #[test]
    fn legacy_battle_without_terrain_preserves_height_only_v1_semantics() {
        let battle = sample_battle();
        let mut encoded = serde_json::to_value(&battle).unwrap();
        encoded.as_object_mut().unwrap().remove("terrain");

        let decoded: TacticalBattle = serde_json::from_value(encoded).unwrap();
        assert_eq!(
            decoded.terrain().profile(),
            crate::terrain::TacticalTerrainProfile::HeightFoundationV1
        );
        assert!(decoded.terrain().forest_cells().is_empty());
        assert!(decoded.terrain().river_cells().is_empty());
        assert!(decoded.siege.is_none());
    }

    #[test]
    fn movement_never_exceeds_speed_and_arrives_exactly() {
        let mut battle = sample_battle();
        let destination = BattlePoint::new(90_000, 150_000);
        battle
            .issue_move_order(MovementOrder {
                unit_id: "attacker-spears".into(),
                destination,
            })
            .unwrap();

        for _ in 0..200 {
            let before = unit(&battle, "attacker-spears").position();
            let speed = unit(&battle, "attacker-spears").speed_mm_per_tick();
            battle.advance_ticks(1);
            let after = unit(&battle, "attacker-spears").position();
            assert!(point_distance_squared(before, after) <= square_u32(speed));
            if unit(&battle, "attacker-spears").destination().is_none() {
                break;
            }
        }
        assert_eq!(unit(&battle, "attacker-spears").position(), destination);
    }

    #[test]
    fn extreme_diagonal_movement_still_respects_speed_bound() {
        let speed = 1_000_000;
        let mut battle = TacticalBattle::new(
            FlatBattlefield::new(u32::MAX, u32::MAX),
            vec![TacticalUnit::new(
                "scouts",
                BattleSide::Attacker,
                20,
                BattlePoint::new(0, 0),
                Formation::Column { files: 4 },
                speed,
            )],
        )
        .unwrap();
        battle
            .issue_move_order(MovementOrder {
                unit_id: "scouts".into(),
                destination: BattlePoint::new(u32::MAX, u32::MAX),
            })
            .unwrap();
        let before = unit(&battle, "scouts").position();
        battle.advance_ticks(1);
        let after = unit(&battle, "scouts").position();
        assert!(point_distance_squared(before, after) <= square_u32(speed));
    }

    #[test]
    fn storage_order_is_normalized_and_combat_is_order_independent() {
        let attacker = sample_unit(
            "attacker",
            BattleSide::Attacker,
            BattlePoint::new(10_000, 10_000),
            Formation::Line { files: 20 },
        );
        let defender = sample_unit(
            "defender",
            BattleSide::Defender,
            BattlePoint::new(11_000, 10_000),
            Formation::Line { files: 20 },
        );
        let mut first = TacticalBattle::new(
            FlatBattlefield::new(50_000, 50_000),
            vec![attacker.clone(), defender.clone()],
        )
        .unwrap();
        let mut second = TacticalBattle::new(
            FlatBattlefield::new(50_000, 50_000),
            vec![defender, attacker],
        )
        .unwrap();
        engage(&mut first, "attacker", "defender");
        engage(&mut second, "attacker", "defender");
        first.advance_ticks(TACTICAL_TICKS_PER_SECOND * 5);
        second.advance_ticks(TACTICAL_TICKS_PER_SECOND * 5);
        assert_eq!(first, second);
    }

    #[test]
    fn line_frontage_inflicts_more_losses_than_column_frontage() {
        let mut line = contact_battle(Formation::Line { files: 24 }, Formation::Line { files: 20 });
        let mut column = contact_battle(
            Formation::Column { files: 24 },
            Formation::Line { files: 20 },
        );
        engage(&mut line, "attacker", "defender");
        engage(&mut column, "attacker", "defender");
        line.advance_ticks(TACTICAL_TICKS_PER_SECOND);
        column.advance_ticks(TACTICAL_TICKS_PER_SECOND);
        assert!(unit(&line, "defender").soldiers() < unit(&column, "defender").soldiers());
    }

    #[test]
    fn simultaneous_opponents_share_one_frontage_budget() {
        let attacker = sample_unit(
            "attacker",
            BattleSide::Attacker,
            BattlePoint::new(25_000, 25_000),
            Formation::Line { files: 40 },
        );
        let first_defender = sample_unit(
            "defender-a",
            BattleSide::Defender,
            BattlePoint::new(24_000, 25_000),
            Formation::Line { files: 20 },
        );
        let second_defender = sample_unit(
            "defender-b",
            BattleSide::Defender,
            BattlePoint::new(26_000, 25_000),
            Formation::Line { files: 20 },
        );
        let battlefield = FlatBattlefield::new(50_000, 50_000);
        let mut single =
            TacticalBattle::new(battlefield, vec![attacker.clone(), first_defender.clone()])
                .unwrap();
        let mut surrounded =
            TacticalBattle::new(battlefield, vec![attacker, first_defender, second_defender])
                .unwrap();
        engage(&mut single, "attacker", "defender-a");
        engage(&mut surrounded, "attacker", "defender-a");
        engage(&mut surrounded, "defender-b", "attacker");

        single.advance_ticks(TACTICAL_TICKS_PER_SECOND);
        surrounded.advance_ticks(TACTICAL_TICKS_PER_SECOND);

        let single_losses = 80 - unit(&single, "defender-a").soldiers();
        let combined_losses = 160
            - unit(&surrounded, "defender-a").soldiers()
            - unit(&surrounded, "defender-b").soldiers();
        assert!(combined_losses <= single_losses);
        assert!(unit(&surrounded, "defender-a").soldiers() < 80);
        assert!(unit(&surrounded, "defender-b").soldiers() < 80);
    }

    #[test]
    fn fatigue_rises_and_reduces_melee_effectiveness() {
        let mut fresh =
            contact_battle(Formation::Line { files: 40 }, Formation::Line { files: 20 });
        let mut tired = fresh.clone();
        let tired_index = tired.unit_index("attacker").unwrap();
        tired.units[tired_index].fatigue = 800;
        engage(&mut fresh, "attacker", "defender");
        engage(&mut tired, "attacker", "defender");
        fresh.advance_ticks(TACTICAL_TICKS_PER_SECOND);
        tired.advance_ticks(TACTICAL_TICKS_PER_SECOND);
        assert!(unit(&fresh, "defender").soldiers() < unit(&tired, "defender").soldiers());
        assert!(unit(&fresh, "attacker").fatigue() > 0);
    }

    #[test]
    fn casualties_apply_morale_shocks_and_eventually_route_units() {
        let mut battle = contact_battle(
            Formation::Line { files: 60 },
            Formation::Column { files: 4 },
        );
        engage(&mut battle, "attacker", "defender");
        for _ in 0..60 {
            battle.advance_ticks(TACTICAL_TICKS_PER_SECOND);
            if unit(&battle, "defender").is_routed() || unit(&battle, "defender").is_destroyed() {
                break;
            }
        }
        let defender = unit(&battle, "defender");
        assert!(defender.soldiers() < 80);
        assert!(defender.morale() < MAX_TACTICAL_MORALE);
        assert!(defender.is_routed() || defender.is_destroyed());
    }

    #[test]
    fn routed_units_reject_orders_and_move_away_from_enemy() {
        let mut battle =
            contact_battle(Formation::Line { files: 20 }, Formation::Line { files: 20 });
        let defender_index = battle.unit_index("defender").unwrap();
        battle.units[defender_index].state = TacticalUnitState::Routed;
        battle.units[defender_index].morale = ROUT_MORALE_THRESHOLD;
        let before = battle.units[defender_index].position;
        assert!(matches!(
            battle.issue_move_order(MovementOrder {
                unit_id: "defender".into(),
                destination: BattlePoint::new(40_000, 40_000),
            }),
            Err(TacticalError::UnitCannotReceiveOrders { .. })
        ));
        battle.advance_ticks(1);
        let after = unit(&battle, "defender").position();
        assert!(battle.terrain().is_passable_at(battle.battlefield(), after));
        assert!(
            point_distance_squared(after, BattlePoint::new(24_500, 25_000))
                > point_distance_squared(before, BattlePoint::new(24_500, 25_000))
        );
    }

    #[test]
    fn pursuit_causes_bounded_losses_while_routed_target_is_close() {
        let mut battle =
            contact_battle(Formation::Line { files: 24 }, Formation::Line { files: 20 });
        engage(&mut battle, "attacker", "defender");
        let defender_index = battle.unit_index("defender").unwrap();
        battle.units[defender_index].state = TacticalUnitState::Routed;
        battle.units[defender_index].morale = ROUT_MORALE_THRESHOLD;
        let before = battle.units[defender_index].soldiers;
        battle.advance_ticks(TACTICAL_TICKS_PER_SECOND);
        let after = unit(&battle, "defender").soldiers();
        assert!(after < before);
        assert!(before - after <= unit(&battle, "attacker").frontage_slots());
    }

    #[test]
    fn zero_soldiers_destroy_unit_without_underflow_or_resurrection() {
        let mut battle = contact_battle(
            Formation::Line { files: 80 },
            Formation::Column { files: 2 },
        );
        let defender_index = battle.unit_index("defender").unwrap();
        battle.units[defender_index].soldiers = 1;
        engage(&mut battle, "attacker", "defender");
        battle.advance_ticks(TACTICAL_TICKS_PER_SECOND);
        let defender = unit(&battle, "defender");
        assert_eq!(defender.soldiers(), 0);
        assert!(defender.is_destroyed());
        battle.advance_ticks(TACTICAL_TICKS_PER_SECOND * 5);
        assert_eq!(unit(&battle, "defender").soldiers(), 0);
    }

    #[test]
    fn movement_preserves_identity_side_formation_and_soldier_count() {
        let mut battle = sample_battle();
        let before = unit(&battle, "attacker-spears").clone();
        battle
            .issue_move_order(MovementOrder {
                unit_id: "attacker-spears".into(),
                destination: BattlePoint::new(80_000, 120_000),
            })
            .unwrap();
        battle.advance_ticks(10);
        let after = unit(&battle, "attacker-spears");
        assert_eq!(after.id(), before.id());
        assert_eq!(after.side(), before.side());
        assert_eq!(after.formation(), before.formation());
        assert_eq!(after.soldiers(), before.soldiers());
        assert_eq!(after.attack_range_mm(), before.attack_range_mm());
        assert_ne!(after.position(), before.position());
    }
}
