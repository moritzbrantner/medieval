use std::{
    collections::{BTreeMap, BTreeSet, HashSet},
    fmt,
};

use physics_engine::{Collider, ColliderShape, Vec3i, collider_contact};
use serde::{Deserialize, Serialize};

use crate::deployment::{DeploymentZone, standard_deployment_zone, standard_deployment_zones};
use crate::terrain::TacticalTerrain;

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
pub struct TacticalUnit {
    id: String,
    side: BattleSide,
    soldiers: u16,
    position: BattlePoint,
    formation: Formation,
    speed_mm_per_tick: u32,
    #[serde(
        default = "default_attack_range_mm",
        deserialize_with = "deserialize_attack_range_mm"
    )]
    attack_range_mm: u32,
    destination: Option<BattlePoint>,
    engagement_target: Option<String>,
    fatigue: u16,
    morale: u16,
    state: TacticalUnitState,
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
            attack_range_mm: COMBAT_CONTACT_DISTANCE_MM,
            destination: None,
            engagement_target: None,
            fatigue: 0,
            morale: MAX_TACTICAL_MORALE,
            state: TacticalUnitState::Formed,
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
    pub const fn attack_range_mm(&self) -> u32 {
        self.attack_range_mm
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
        matches!(self.state, TacticalUnitState::Routed)
    }

    #[must_use]
    pub const fn is_destroyed(&self) -> bool {
        matches!(self.state, TacticalUnitState::Destroyed)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", from = "TacticalBattleWire")]
pub struct TacticalBattle {
    tick: u64,
    battlefield: FlatBattlefield,
    terrain: TacticalTerrain,
    units: Vec<TacticalUnit>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct TacticalBattleWire {
    tick: u64,
    battlefield: FlatBattlefield,
    #[serde(default)]
    terrain: TacticalTerrain,
    units: Vec<TacticalUnit>,
}

impl From<TacticalBattleWire> for TacticalBattle {
    fn from(mut wire: TacticalBattleWire) -> Self {
        wire.units.sort_by(|left, right| left.id.cmp(&right.id));
        Self {
            tick: wire.tick,
            battlefield: wire.battlefield,
            terrain: wire.terrain,
            units: wire.units,
        }
    }
}

impl TacticalBattle {
    pub fn new(
        battlefield: FlatBattlefield,
        mut units: Vec<TacticalUnit>,
    ) -> Result<Self, TacticalError> {
        if battlefield.width_mm == 0 || battlefield.depth_mm == 0 {
            return Err(TacticalError::InvalidBattlefield {
                width_mm: battlefield.width_mm,
                depth_mm: battlefield.depth_mm,
            });
        }

        let terrain = TacticalTerrain::battlefield_foundation();
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
            units,
        })
    }

    pub fn deploy(
        battlefield: FlatBattlefield,
        units: Vec<TacticalUnit>,
    ) -> Result<Self, TacticalError> {
        let battle = Self::new(battlefield, units)?;
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

    #[must_use]
    pub const fn deployment_zones(&self) -> [DeploymentZone; 2] {
        standard_deployment_zones(self.battlefield)
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
    pub fn units(&self) -> &[TacticalUnit] {
        &self.units
    }

    pub fn issue_move_order(&mut self, order: MovementOrder) -> Result<(), TacticalError> {
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
        if !self.terrain.is_passable_at(self.battlefield, order.destination) {
            return Err(TacticalError::DestinationImpassable {
                unit_id: order.unit_id,
                destination: order.destination,
            });
        }

        let unit = &mut self.units[unit_index];
        unit.engagement_target = None;
        unit.destination = (unit.position != order.destination).then_some(order.destination);
        Ok(())
    }

    pub fn issue_engagement_order(
        &mut self,
        unit_id: &str,
        target_unit_id: &str,
    ) -> Result<(), TacticalError> {
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
        if self.units[target_index].state == TacticalUnitState::Destroyed {
            return Err(TacticalError::TargetDestroyed(target_unit_id.to_owned()));
        }

        let unit = &mut self.units[unit_index];
        unit.destination = None;
        unit.engagement_target = Some(target_unit_id.to_owned());
        Ok(())
    }

    pub fn issue_formation_order(
        &mut self,
        unit_id: &str,
        formation: Formation,
    ) -> Result<(), TacticalError> {
        let unit_index = self
            .unit_index(unit_id)
            .ok_or_else(|| TacticalError::UnitNotFound(unit_id.to_owned()))?;
        self.ensure_can_receive_orders(unit_index)?;
        if formation.files() == 0 {
            return Err(TacticalError::InvalidFormation(unit_id.to_owned()));
        }
        self.units[unit_index].formation = formation;
        Ok(())
    }

    pub fn advance_ticks(&mut self, ticks: u32) {
        for _ in 0..ticks {
            self.advance_movement_phase();
            self.tick = self.tick.saturating_add(1);
            if self
                .tick
                .is_multiple_of(u64::from(TACTICAL_TICKS_PER_SECOND))
            {
                self.resolve_combat_pulse();
            }
            self.clear_invalid_engagement_targets();
        }
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

    fn advance_movement_phase(&mut self) {
        let snapshot = self.units.clone();
        for index in 0..self.units.len() {
            match snapshot[index].state {
                TacticalUnitState::Formed => self.advance_formed_unit(index, &snapshot),
                TacticalUnitState::Routed => self.advance_routed_unit(index, &snapshot),
                TacticalUnitState::Destroyed => {}
            }
        }
    }

    fn advance_formed_unit(&mut self, index: usize, snapshot: &[TacticalUnit]) {
        let unit = &snapshot[index];
        let target = unit
            .engagement_target
            .as_deref()
            .and_then(|target_id| snapshot.iter().find(|target| target.id == target_id))
            .filter(|target| target.state != TacticalUnitState::Destroyed);
        if let Some(target) = target
            && target.state == TacticalUnitState::Routed
            && !points_within_distance(unit.position, target.position, PURSUIT_DISTANCE_MM)
        {
            self.units[index].engagement_target = None;
            self.units[index].fatigue = self.units[index]
                .fatigue
                .saturating_sub(IDLE_FATIGUE_RECOVERY_PER_TICK);
            return;
        }

        let destination = unit
            .destination
            .or_else(|| target.map(|target| target.position));
        let Some(destination) = destination else {
            self.units[index].fatigue = self.units[index]
                .fatigue
                .saturating_sub(IDLE_FATIGUE_RECOVERY_PER_TICK);
            return;
        };

        let engagement_stop_distance = target
            .filter(|target| target.state == TacticalUnitState::Formed)
            .map_or(COMBAT_CONTACT_DISTANCE_MM, |_| unit.attack_range_mm);
        if unit.engagement_target.is_some()
            && points_within_distance(unit.position, destination, engagement_stop_distance)
        {
            self.units[index].fatigue = self.units[index]
                .fatigue
                .saturating_sub(IDLE_FATIGUE_RECOVERY_PER_TICK);
            return;
        }

        let base_movement_speed =
            if target.is_some_and(|target| target.state == TacticalUnitState::Routed) {
                unit.speed_mm_per_tick.saturating_mul(2)
            } else {
                unit.speed_mm_per_tick
            };
        let movement_speed = self.terrain().movement_speed_mm_per_tick(
            self.battlefield,
            unit.position,
            base_movement_speed,
        );
        let waypoint = self
            .terrain()
            .movement_waypoint(self.battlefield, unit.position, destination);
        let next = move_point_toward(unit.position, waypoint, movement_speed);
        debug_assert!(self.terrain().is_passable_at(self.battlefield, next));
        self.units[index].position = next;
        if self.units[index].destination == Some(destination) && next == destination {
            self.units[index].destination = None;
        }
        if next != unit.position {
            self.units[index].fatigue = self.units[index]
                .fatigue
                .saturating_add(MOVEMENT_FATIGUE_PER_TICK)
                .min(MAX_TACTICAL_FATIGUE);
        }
    }

    fn advance_routed_unit(&mut self, index: usize, snapshot: &[TacticalUnit]) {
        let unit = &snapshot[index];
        let nearest_enemy = snapshot
            .iter()
            .filter(|candidate| {
                candidate.side != unit.side && candidate.state == TacticalUnitState::Formed
            })
            .min_by(|left, right| {
                point_distance_squared(unit.position, left.position)
                    .cmp(&point_distance_squared(unit.position, right.position))
                    .then_with(|| left.id.cmp(&right.id))
            });

        let Some(enemy) = nearest_enemy else {
            return;
        };
        let route_speed = self.terrain().movement_speed_mm_per_tick(
            self.battlefield,
            unit.position,
            unit.speed_mm_per_tick.saturating_mul(2),
        );
        let desired = move_point_away(
            unit.position,
            enemy.position,
            route_speed,
            self.battlefield,
            unit.side,
        );
        let waypoint = self
            .terrain()
            .movement_waypoint(self.battlefield, unit.position, desired);
        let next = move_point_toward(unit.position, waypoint, route_speed);
        debug_assert!(self.terrain().is_passable_at(self.battlefield, next));
        self.units[index].position = next;
        if next != unit.position {
            self.units[index].fatigue = self.units[index]
                .fatigue
                .saturating_add(ROUT_FATIGUE_PER_TICK)
                .min(MAX_TACTICAL_FATIGUE);
        }
    }

    fn resolve_combat_pulse(&mut self) {
        let snapshot = self.units.clone();
        let mut pairs = BTreeSet::new();
        for unit in &snapshot {
            if unit.state != TacticalUnitState::Formed {
                continue;
            }
            let Some(target_id) = unit.engagement_target.as_deref() else {
                continue;
            };
            let Some(target) = snapshot.iter().find(|target| target.id == target_id) else {
                continue;
            };
            if target.state == TacticalUnitState::Destroyed || target.side == unit.side {
                continue;
            }
            let pair = if unit.id < target.id {
                (unit.id.clone(), target.id.clone())
            } else {
                (target.id.clone(), unit.id.clone())
            };
            pairs.insert(pair);
        }

        let formed_contacts = pairs
            .iter()
            .filter(|(left_id, right_id)| {
                let left = snapshot.iter().find(|unit| unit.id == *left_id).unwrap();
                let right = snapshot.iter().find(|unit| unit.id == *right_id).unwrap();
                left.state == TacticalUnitState::Formed
                    && right.state == TacticalUnitState::Formed
                    && points_within_distance(
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

        for attacker in &snapshot {
            if attacker.state != TacticalUnitState::Formed
                || attacker.attack_range_mm <= COMBAT_CONTACT_DISTANCE_MM
                || formed_contacts.contains_key(&attacker.id)
            {
                continue;
            }
            let Some(target_id) = attacker.engagement_target.as_deref() else {
                continue;
            };
            let Some(target) = snapshot.iter().find(|target| target.id == target_id) else {
                continue;
            };
            if target.state != TacticalUnitState::Formed || target.side == attacker.side {
                continue;
            }
            if points_within_distance(
                attacker.position,
                target.position,
                COMBAT_CONTACT_DISTANCE_MM,
            ) || !points_within_distance(
                attacker.position,
                target.position,
                attacker.attack_range_mm,
            ) {
                continue;
            }
            let losses = ranged_casualties(attacker, target);
            if losses > 0 {
                *casualties.entry(target.id.clone()).or_default() += u32::from(losses);
                engaged_units.insert(attacker.id.clone());
            }
        }

        let mut contacts_assigned = BTreeMap::<String, u16>::new();
        for (left_id, right_id) in pairs {
            let left = snapshot.iter().find(|unit| unit.id == left_id).unwrap();
            let right = snapshot.iter().find(|unit| unit.id == right_id).unwrap();
            match (left.state, right.state) {
                (TacticalUnitState::Formed, TacticalUnitState::Formed)
                    if points_within_distance(
                        left.position,
                        right.position,
                        COMBAT_CONTACT_DISTANCE_MM,
                    ) =>
                {
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
                    let left_losses = melee_casualties(right, left, right_frontage);
                    let right_losses = melee_casualties(left, right, left_frontage);
                    *casualties.entry(left.id.clone()).or_default() += u32::from(left_losses);
                    *casualties.entry(right.id.clone()).or_default() += u32::from(right_losses);
                    engaged_units.insert(left.id.clone());
                    engaged_units.insert(right.id.clone());
                }
                (TacticalUnitState::Formed, TacticalUnitState::Routed)
                    if left.engagement_target.as_deref() == Some(right.id.as_str())
                        && points_within_distance(
                            left.position,
                            right.position,
                            PURSUIT_DISTANCE_MM,
                        ) =>
                {
                    *casualties.entry(right.id.clone()).or_default() +=
                        u32::from(pursuit_casualties(left));
                    engaged_units.insert(left.id.clone());
                }
                (TacticalUnitState::Routed, TacticalUnitState::Formed)
                    if right.engagement_target.as_deref() == Some(left.id.as_str())
                        && points_within_distance(
                            left.position,
                            right.position,
                            PURSUIT_DISTANCE_MM,
                        ) =>
                {
                    *casualties.entry(left.id.clone()).or_default() +=
                        u32::from(pursuit_casualties(right));
                    engaged_units.insert(right.id.clone());
                }
                _ => {}
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
            self.units[index].soldiers -= applied;
            if self.units[index].soldiers == 0 {
                self.units[index].morale = 0;
                self.units[index].state = TacticalUnitState::Destroyed;
                self.units[index].destination = None;
                self.units[index].engagement_target = None;
                continue;
            }

            if self.units[index].state == TacticalUnitState::Formed {
                let shock = casualty_morale_shock(before, applied);
                self.units[index].morale = self.units[index].morale.saturating_sub(shock);
                if self.units[index].morale <= ROUT_MORALE_THRESHOLD {
                    self.units[index].state = TacticalUnitState::Routed;
                    self.units[index].destination = None;
                    self.units[index].engagement_target = None;
                }
            }
        }
    }

    fn clear_invalid_engagement_targets(&mut self) {
        let states: BTreeMap<String, TacticalUnitState> = self
            .units
            .iter()
            .map(|unit| (unit.id.clone(), unit.state))
            .collect();
        for unit in &mut self.units {
            if unit.state != TacticalUnitState::Formed {
                unit.engagement_target = None;
                continue;
            }
            let Some(target_id) = unit.engagement_target.as_deref() else {
                continue;
            };
            if states.get(target_id) == Some(&TacticalUnitState::Destroyed) {
                unit.engagement_target = None;
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TacticalError {
    InvalidBattlefield {
        width_mm: u32,
        depth_mm: u32,
    },
    EmptyUnitId,
    DuplicateUnitId(String),
    ZeroSoldiers(String),
    InvalidFormation(String),
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
}

impl fmt::Display for TacticalError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidBattlefield { width_mm, depth_mm } => write!(
                formatter,
                "battlefield dimensions must be positive, got {width_mm}x{depth_mm} mm"
            ),
            Self::EmptyUnitId => write!(formatter, "tactical unit IDs must not be empty"),
            Self::DuplicateUnitId(unit_id) => {
                write!(formatter, "tactical unit ID {unit_id} is duplicated")
            }
            Self::ZeroSoldiers(unit_id) => {
                write!(formatter, "tactical unit {unit_id} must contain soldiers")
            }
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

fn melee_casualties(attacker: &TacticalUnit, defender: &TacticalUnit, frontage: u16) -> u16 {
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
    let losses = (effective_frontage / MELEE_CASUALTY_DIVISOR).max(1);
    u16::try_from(losses.min(u32::from(defender.soldiers))).unwrap()
}

fn ranged_casualties(attacker: &TacticalUnit, defender: &TacticalUnit) -> u16 {
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
    let losses = (effective_frontage / RANGED_CASUALTY_DIVISOR).max(1);
    u16::try_from(losses.min(u32::from(defender.soldiers))).unwrap()
}

fn pursuit_casualties(pursuer: &TacticalUnit) -> u16 {
    let frontage = u32::from(pursuer.frontage_slots());
    let fatigue_factor = 1_000_u32.saturating_sub(u32::from(pursuer.fatigue) / 2);
    let effective_frontage = frontage.saturating_mul(fatigue_factor) / 1_000;
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
fn points_within_distance(left: BattlePoint, right: BattlePoint, distance_mm: u32) -> bool {
    let radius =
        i32::try_from(distance_mm).expect("tactical contact radius must fit physics Vec3i");
    let dx = i64::from(right.x_mm) - i64::from(left.x_mm);
    let dz = i64::from(right.y_mm) - i64::from(left.y_mm);
    let distance = u64::from(distance_mm);
    if dx.unsigned_abs() > distance || dz.unsigned_abs() > distance {
        return false;
    }

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
            if battle
                .terrain()
                .river_crossing_cells()
                .iter()
                .any(|cell| {
                    battle
                        .terrain()
                        .cell_bounds_mm(battlefield, cell.cell_x, cell.cell_z)
                        .is_some_and(|(x0, x1, z0, z1)| {
                            position.x_mm >= x0
                                && position.x_mm < x1
                                && position.y_mm >= z0
                                && position.y_mm < z1
                        })
                })
            {
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
            destination: BattlePoint::new(120_000, 140_000),
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
