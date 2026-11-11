//! Battle construction and read-only queries.

use super::error::TacticalError;
use super::{
    BattlePoint, FlatBattlefield, TacticalBattle, TacticalBattleState, TacticalCompletionRules,
    TacticalUnit, valid_attack_range_mm,
};
use crate::deployment::siege::{SiegeBattleState, SiegeLadder, SiegeProfile};
use crate::deployment::{DeploymentZone, standard_deployment_zone, standard_deployment_zones};
use crate::siege_engine::{SiegeEngineKind, SiegeEngineSetup, SiegeEngineState, SiegeEngineStatus};
use crate::terrain::{BattlefieldLocation, TacticalTerrain};
use std::collections::{BTreeMap, HashSet};

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
            tower_damage_credit: BTreeMap::new(),
            siege_ladders: Vec::new(),
            siege_engines: Vec::new(),
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

    /// A profiled siege with explicit pre-battle equipment. Each engine is
    /// crewed by one formed attacker foot unit (never a defender or mounted
    /// `Knights`), and one unit crews at most one engine. Engines are listed
    /// in setup order; catapults start loaded with full ammunition. An empty
    /// setup is exactly [`Self::deploy_siege_with_profile`].
    pub fn deploy_siege_with_engines(
        battlefield: FlatBattlefield,
        units: Vec<TacticalUnit>,
        location: BattlefieldLocation,
        profile: SiegeProfile,
        engines: Vec<SiegeEngineSetup>,
    ) -> Result<Self, TacticalError> {
        let mut battle = Self::deploy_siege_with_profile(battlefield, units, location, profile)?;
        let mut crews = HashSet::with_capacity(engines.len());
        for setup in &engines {
            let index = battle
                .unit_index(&setup.crew_unit_id)
                .ok_or_else(|| TacticalError::UnitNotFound(setup.crew_unit_id.clone()))?;
            let crew = &battle.units[index];
            if !crew.can_use_ladders()
                || !crew.can_receive_orders()
                || !crews.insert(setup.crew_unit_id.as_str())
            {
                return Err(TacticalError::SiegeEngineUnavailable(
                    setup.crew_unit_id.clone(),
                ));
            }
        }
        battle.siege_engines = engines.into_iter().map(SiegeEngineState::new).collect();
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

    /// Core-owned activity of every siege tower in layout order: whether a
    /// formed defender staffs it and which formed attacker it fires at on the
    /// next combat pulse. `None` outside a siege.
    #[must_use]
    pub fn siege_tower_activity(&self) -> Option<Vec<crate::SiegeTowerActivity>> {
        self.siege.map(|siege| siege.tower_activity(&self.units))
    }

    /// Core-owned siege capture objectives in order, outer to inner, with
    /// their progress and whether each is unlocked. Single-point profiles have
    /// one objective mirroring `layout.capture_point` and `capture`. `None`
    /// outside a siege.
    #[must_use]
    pub fn siege_objectives(&self) -> Option<Vec<crate::SiegeObjectiveStatus>> {
        self.siege.map(SiegeBattleState::objectives)
    }

    /// Placed siege ladders in placement order, with whether a formed
    /// defender holds each one's top. `None` outside a siege.
    #[must_use]
    pub fn siege_ladders(&self) -> Option<Vec<SiegeLadder>> {
        self.siege?;
        Some(
            self.siege_ladders
                .iter()
                .map(|ladder| SiegeLadder {
                    area: ladder.area,
                    held_by_defender: SiegeBattleState::ladder_held(ladder, &self.units),
                })
                .collect(),
        )
    }

    /// Core-owned state of every siege engine in setup order, at its crew's
    /// position. `None` outside a siege; empty without explicit setup.
    #[must_use]
    pub fn siege_engines(&self) -> Option<Vec<SiegeEngineStatus>> {
        self.siege?;
        Some(
            self.siege_engines
                .iter()
                .filter_map(|engine| {
                    let crew = self.unit_index(&engine.crew_unit_id)?;
                    Some(engine.status(self.units[crew].position))
                })
                .collect(),
        )
    }

    /// The engine crewed by `unit_id`, if any.
    pub(super) fn siege_engine_of(&self, unit_id: &str) -> Option<&SiegeEngineState> {
        self.siege_engines
            .iter()
            .find(|engine| engine.crew_unit_id == unit_id)
    }

    pub(super) fn has_siege_engine(&self, unit_id: &str, kind: SiegeEngineKind) -> bool {
        self.siege_engine_of(unit_id)
            .is_some_and(|engine| engine.kind == kind)
    }

    /// Whether `unit` climbs ladders: an eligible unit that does not crew a
    /// siege engine (an engine does not climb walls).
    pub(super) fn climbs_ladders(&self, unit: &TacticalUnit) -> bool {
        unit.can_use_ladders() && self.siege_engine_of(&unit.id).is_none()
    }

    /// Whether a siege wall stands between two positions, so formed units
    /// there cannot fight in melee. Placed ladders open their own cells.
    pub(super) fn wall_separates(&self, left: BattlePoint, right: BattlePoint) -> bool {
        self.siege.is_some_and(|siege| {
            siege.wall_separates_with_ladders(
                left,
                right,
                self.siege_ladders.iter().map(|ladder| ladder.area),
            )
        })
    }
}
