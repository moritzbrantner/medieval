//! Accessors, builders and order bookkeeping of a single [`TacticalUnit`].

use super::{
    BattlePoint, BattleSide, COMBAT_CONTACT_DISTANCE_MM, Formation, MAX_TACTICAL_MORALE,
    MissileAmmunition, TacticalUnit, TacticalUnitProvenance, TacticalUnitState,
};
use crate::UnitKind;
use crate::siege_engine::SiegeEngineTarget;

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
            gate_attack: false,
            ladder_order: None,
            siege_engine_fire: None,
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

    pub(super) fn interrupt_charge(&mut self) {
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

    /// Whether the unit holds an order to assault the siege gate.
    #[must_use]
    pub const fn is_attacking_gate(&self) -> bool {
        self.gate_attack
    }

    /// The wall point this unit is ordered to place its ladder at.
    #[must_use]
    pub const fn ladder_order(&self) -> Option<BattlePoint> {
        self.ladder_order
    }

    /// Whether the unit may place and climb siege ladders: attacker foot
    /// units. Mounted knights and defenders never use ladders.
    #[must_use]
    pub fn can_use_ladders(&self) -> bool {
        self.side == BattleSide::Attacker && self.unit_kind() != Some(UnitKind::Knights)
    }

    /// The target of this unit's standing catapult fire order.
    #[must_use]
    pub const fn siege_engine_fire_target(&self) -> Option<&SiegeEngineTarget> {
        self.siege_engine_fire.as_ref()
    }

    /// Ends every standing siege order (gate assault, ladder placement or
    /// engine fire).
    pub(super) fn clear_siege_orders(&mut self) {
        self.gate_attack = false;
        self.ladder_order = None;
        self.siege_engine_fire = None;
    }

    #[cfg(test)]
    pub(crate) const fn with_gate_attack_order(mut self) -> Self {
        self.gate_attack = true;
        self
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

    pub(super) const fn is_pursuit_target(&self) -> bool {
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
