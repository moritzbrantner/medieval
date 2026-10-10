//! Core-owned unit unlocks and bounded local recruitment pools.
//!
//! Every [`UnitKind`] has one explicit [`UnitUnlock`] source: a minimum
//! settlement level and, for trained troops, a barracks level standing in the
//! province. A province holds a [`RecruitmentPool`] of recruitable batches per
//! unit. Queueing a batch consumes one from the pool; at the start of the
//! owning faction's turn, after settlement upgrades and construction complete,
//! every unlocked pool regains one batch up to its capacity.

use serde::{Deserialize, Serialize};

use crate::{
    BuildingId, BuildingRequirement, CampaignError, CampaignState, Province, RecruitmentOrder,
    SettlementLevel, UNIT_KINDS, UnitKind,
};

/// The explicit local source that makes a unit recruitable in a province.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UnitUnlock {
    /// Minimum current settlement level of the province.
    pub settlement: SettlementLevel,
    /// Military building that must stand in the province, if any.
    pub building: Option<BuildingRequirement>,
}

/// Batches each unit can be recruited from in one province.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecruitmentPool {
    pub levy: u8,
    pub spearmen: u8,
    pub archers: u8,
    pub knights: u8,
}

impl RecruitmentPool {
    #[must_use]
    pub fn available(&self, unit: UnitKind) -> u8 {
        match unit {
            UnitKind::Levy => self.levy,
            UnitKind::Spearmen => self.spearmen,
            UnitKind::Archers => self.archers,
            UnitKind::Knights => self.knights,
        }
    }

    fn slot_mut(&mut self, unit: UnitKind) -> &mut u8 {
        match unit {
            UnitKind::Levy => &mut self.levy,
            UnitKind::Spearmen => &mut self.spearmen,
            UnitKind::Archers => &mut self.archers,
            UnitKind::Knights => &mut self.knights,
        }
    }
}

const fn barracks(level: u8) -> Option<BuildingRequirement> {
    Some(BuildingRequirement {
        building: BuildingId::Barracks,
        level,
    })
}

fn settlement_tier(level: SettlementLevel) -> u8 {
    match level {
        SettlementLevel::Village => 0,
        SettlementLevel::Town => 1,
        SettlementLevel::City => 2,
        SettlementLevel::MajorCity => 3,
    }
}

impl UnitKind {
    /// The single local source that unlocks recruitment of this unit.
    #[must_use]
    pub fn unlock(self) -> UnitUnlock {
        match self {
            Self::Levy => UnitUnlock {
                settlement: SettlementLevel::Village,
                building: None,
            },
            Self::Spearmen => UnitUnlock {
                settlement: SettlementLevel::Village,
                building: barracks(1),
            },
            Self::Archers => UnitUnlock {
                settlement: SettlementLevel::Town,
                building: None,
            },
            Self::Knights => UnitUnlock {
                settlement: SettlementLevel::Town,
                building: barracks(2),
            },
        }
    }
}

impl UnitUnlock {
    /// A player-facing description of the unlock source.
    #[must_use]
    pub fn describe(self) -> String {
        let settlement = self.settlement.label().to_lowercase();
        match self.building.and_then(|required| {
            required
                .building
                .level(required.level)
                .map(|level| level.label)
        }) {
            Some(building) => format!("a {settlement} with a {building}"),
            None => format!("a {settlement}"),
        }
    }
}

impl Province {
    /// Whether the province's settlement and buildings unlock `unit`.
    #[must_use]
    pub fn unlocks_unit(&self, unit: UnitKind) -> bool {
        let unlock = unit.unlock();
        self.settlement_level >= unlock.settlement
            && unlock
                .building
                .is_none_or(|required| self.building_level(required.building) >= required.level)
    }

    /// Maximum batches of `unit` the province's pool holds: zero while the
    /// unit is locked, otherwise one batch plus one for every settlement level
    /// and barracks level beyond the unit's unlock source.
    #[must_use]
    pub fn recruitment_capacity(&self, unit: UnitKind) -> u8 {
        if !self.unlocks_unit(unit) {
            return 0;
        }
        let unlock = unit.unlock();
        let settlement_depth =
            settlement_tier(self.settlement_level) - settlement_tier(unlock.settlement);
        let barracks_depth = self.building_level(BuildingId::Barracks)
            - unlock.building.map_or(0, |required| required.level);
        1 + settlement_depth + barracks_depth
    }

    /// The pool with every unit at its current capacity.
    #[must_use]
    pub fn full_recruitment_pool(&self) -> RecruitmentPool {
        let mut pool = RecruitmentPool::default();
        for unit in UNIT_KINDS {
            *pool.slot_mut(unit) = self.recruitment_capacity(unit);
        }
        pool
    }
}

/// The authoritative recruitment choice for one unit in one province.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecruitmentOption {
    pub unit: UnitKind,
    pub label: String,
    pub cost: u32,
    pub soldiers: u16,
    /// The local source that unlocks this unit.
    pub unlock: UnitUnlock,
    /// Player-facing description of [`Self::unlock`].
    pub unlock_label: String,
    pub unlocked: bool,
    /// Batches that can be queued now.
    pub pool: u8,
    /// Batches the pool holds when full; it regains one each owner turn.
    pub pool_capacity: u8,
    pub available: bool,
    pub reason: Option<String>,
}

impl CampaignState {
    pub fn recruitment_options(
        &self,
        province_id: &str,
    ) -> Result<Vec<RecruitmentOption>, CampaignError> {
        let province = self.province(province_id)?;
        let treasury = self.faction(&self.active_faction)?.treasury;

        Ok(UNIT_KINDS
            .iter()
            .copied()
            .map(|unit| {
                let spec = unit.spec();
                let reason = self
                    .recruitment_error(province, unit, treasury)
                    .err()
                    .map(|error| self.recruitment_reason(&error));
                RecruitmentOption {
                    unit,
                    label: spec.label.to_owned(),
                    cost: spec.cost,
                    soldiers: spec.soldiers,
                    unlock: unit.unlock(),
                    unlock_label: unit.unlock().describe(),
                    unlocked: province.unlocks_unit(unit),
                    pool: province.recruitment_pool.available(unit),
                    pool_capacity: province.recruitment_capacity(unit),
                    available: reason.is_none(),
                    reason,
                }
            })
            .collect())
    }

    pub fn queue_recruitment(
        &mut self,
        province_id: &str,
        unit: UnitKind,
    ) -> Result<(), CampaignError> {
        let faction_index = self
            .factions
            .iter()
            .position(|faction| faction.id == self.active_faction)
            .ok_or_else(|| CampaignError::FactionNotFound(self.active_faction.clone()))?;
        let province_index = self
            .provinces
            .iter()
            .position(|province| province.id == province_id)
            .ok_or_else(|| CampaignError::ProvinceNotFound(province_id.to_owned()))?;
        self.recruitment_error(
            &self.provinces[province_index],
            unit,
            self.factions[faction_index].treasury,
        )?;

        let spec = unit.spec();
        let province = &mut self.provinces[province_index];
        *province.recruitment_pool.slot_mut(unit) -= 1;
        let province_name = province.name.clone();
        self.factions[faction_index].treasury -= spec.cost;
        let ready_on_turn = self.turn.saturating_add(self.factions.len() as u32);
        self.recruitment_queue.push(RecruitmentOrder {
            faction_id: self.active_faction.clone(),
            province_id: province_id.to_owned(),
            unit,
            label: spec.label.to_owned(),
            cost: spec.cost,
            soldiers: spec.soldiers,
            ready_on_turn,
        });
        self.log.push(format!(
            "Turn {}: {} queues {} {} in {} for {} gold.",
            self.turn,
            self.factions[faction_index].name,
            spec.soldiers,
            spec.label,
            province_name,
            spec.cost
        ));
        Ok(())
    }

    /// Refills the active faction's pools by one batch per unlocked unit, up to
    /// capacity. Runs once per faction turn inside the economy-guarded
    /// turn-start step.
    pub(crate) fn replenish_recruitment_pools(&mut self) {
        let faction_id = self.active_faction.clone();
        for province in self
            .provinces
            .iter_mut()
            .filter(|province| province.owner == faction_id)
        {
            for unit in UNIT_KINDS {
                let capacity = province.recruitment_capacity(unit);
                let slot = province.recruitment_pool.slot_mut(unit);
                *slot = slot.saturating_add(1).min(capacity);
            }
        }
    }

    fn recruitment_error(
        &self,
        province: &Province,
        unit: UnitKind,
        treasury: u32,
    ) -> Result<(), CampaignError> {
        if self.pending_battle.is_some() {
            return Err(CampaignError::BattlePending);
        }
        if province.owner != self.active_faction {
            return Err(CampaignError::ProvinceNotOwned {
                province: province.name.clone(),
                owner: province.owner.clone(),
                active_faction: self.active_faction.clone(),
            });
        }
        if !province.unlocks_unit(unit) {
            return Err(CampaignError::UnitLocked {
                province: province.name.clone(),
                unit,
                requirement: unit.unlock().describe(),
            });
        }
        if province.recruitment_pool.available(unit) == 0 {
            return Err(CampaignError::RecruitmentPoolEmpty {
                province: province.name.clone(),
                unit,
            });
        }
        let cost = unit.spec().cost;
        if treasury < cost {
            return Err(CampaignError::InsufficientTreasury {
                faction: self.faction_name(&self.active_faction),
                cost,
                treasury,
            });
        }
        Ok(())
    }

    fn recruitment_reason(&self, error: &CampaignError) -> String {
        match error {
            CampaignError::BattlePending => {
                "Resolve the pending battle before recruiting.".to_owned()
            }
            CampaignError::ProvinceNotOwned { province, .. } => format!(
                "Only {} can recruit during this turn, and it does not control {province}.",
                self.faction_name(&self.active_faction)
            ),
            CampaignError::InsufficientTreasury { cost, treasury, .. } => {
                format!("Need {cost} gold; the treasury has {treasury}.")
            }
            other => {
                let message = other.to_string();
                let mut characters = message.chars();
                characters.next().map_or_else(String::new, |first| {
                    format!("{}{}.", first.to_uppercase(), characters.as_str())
                })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CampaignSave, ProvinceBuilding, new_campaign};

    fn province<'a>(campaign: &'a CampaignState, id: &str) -> &'a Province {
        campaign
            .provinces
            .iter()
            .find(|province| province.id == id)
            .unwrap()
    }

    fn option(campaign: &CampaignState, province_id: &str, unit: UnitKind) -> RecruitmentOption {
        campaign
            .recruitment_options(province_id)
            .unwrap()
            .into_iter()
            .find(|option| option.unit == unit)
            .unwrap()
    }

    fn set_barracks(campaign: &mut CampaignState, province_id: &str, level: u8) {
        let province = campaign
            .provinces
            .iter_mut()
            .find(|province| province.id == province_id)
            .unwrap();
        province
            .buildings
            .retain(|standing| standing.building != BuildingId::Barracks);
        province.buildings.push(ProvinceBuilding {
            building: BuildingId::Barracks,
            level,
        });
        province.buildings.sort_by_key(|standing| standing.building);
    }

    #[test]
    fn every_unit_has_an_explicit_local_unlock_source() {
        let campaign = new_campaign();
        let wessex = province(&campaign, "wessex");
        let normandy = province(&campaign, "normandy");
        assert_eq!(wessex.settlement_level, SettlementLevel::Village);
        assert_eq!(normandy.settlement_level, SettlementLevel::Town);

        assert!(wessex.unlocks_unit(UnitKind::Levy));
        assert!(!wessex.unlocks_unit(UnitKind::Archers));
        assert!(normandy.unlocks_unit(UnitKind::Archers));
        for province in [wessex, normandy] {
            assert!(!province.unlocks_unit(UnitKind::Spearmen));
            assert!(!province.unlocks_unit(UnitKind::Knights));
        }

        let mut campaign = campaign;
        set_barracks(&mut campaign, "normandy", 1);
        assert!(province(&campaign, "normandy").unlocks_unit(UnitKind::Spearmen));
        assert!(!province(&campaign, "normandy").unlocks_unit(UnitKind::Knights));
        set_barracks(&mut campaign, "normandy", 2);
        assert!(province(&campaign, "normandy").unlocks_unit(UnitKind::Knights));
    }

    #[test]
    fn locked_units_report_the_core_owned_unlock_source() {
        let campaign = new_campaign();
        let knights = option(&campaign, "normandy", UnitKind::Knights);
        assert!(!knights.available);
        assert!(!knights.unlocked);
        assert_eq!(knights.pool_capacity, 0);
        assert_eq!(
            knights.reason.as_deref(),
            Some("Knights in Normandy require a town with a Barracks.")
        );
        let archers = option(&campaign, "wessex", UnitKind::Archers);
        assert_eq!(
            archers.reason.as_deref(),
            Some("Archers in Wessex require a town.")
        );

        let mut campaign = campaign;
        assert_eq!(
            campaign.queue_recruitment("normandy", UnitKind::Spearmen),
            Err(CampaignError::UnitLocked {
                province: "Normandy".into(),
                unit: UnitKind::Spearmen,
                requirement: "a village with a Muster field".into(),
            })
        );
    }

    #[test]
    fn pool_capacity_grows_with_settlement_and_barracks_depth() {
        let mut campaign = new_campaign();
        let normandy = province(&campaign, "normandy");
        assert_eq!(normandy.recruitment_capacity(UnitKind::Levy), 2);
        assert_eq!(normandy.recruitment_capacity(UnitKind::Archers), 1);
        let paris = province(&campaign, "paris");
        assert_eq!(paris.recruitment_capacity(UnitKind::Levy), 3);
        assert_eq!(paris.recruitment_capacity(UnitKind::Archers), 2);

        set_barracks(&mut campaign, "normandy", 2);
        let normandy = province(&campaign, "normandy");
        assert_eq!(normandy.recruitment_capacity(UnitKind::Levy), 4);
        assert_eq!(normandy.recruitment_capacity(UnitKind::Spearmen), 3);
        assert_eq!(normandy.recruitment_capacity(UnitKind::Archers), 3);
        assert_eq!(normandy.recruitment_capacity(UnitKind::Knights), 1);
    }

    #[test]
    fn new_campaigns_start_with_full_pools_and_queueing_consumes_batches() {
        let mut campaign = new_campaign();
        for province in &campaign.provinces {
            assert_eq!(province.recruitment_pool, province.full_recruitment_pool());
        }

        campaign
            .queue_recruitment("normandy", UnitKind::Levy)
            .unwrap();
        campaign
            .queue_recruitment("normandy", UnitKind::Levy)
            .unwrap();
        assert_eq!(campaign.recruitment_queue.len(), 2);
        let levy = option(&campaign, "normandy", UnitKind::Levy);
        assert_eq!((levy.pool, levy.pool_capacity), (0, 2));
        assert_eq!(
            levy.reason.as_deref(),
            Some("No Levy batch is available in Normandy until its pool replenishes.")
        );
        assert_eq!(
            campaign.queue_recruitment("normandy", UnitKind::Levy),
            Err(CampaignError::RecruitmentPoolEmpty {
                province: "Normandy".into(),
                unit: UnitKind::Levy,
            })
        );
    }

    #[test]
    fn pools_replenish_one_batch_per_owner_turn_up_to_capacity() {
        let mut campaign = new_campaign();
        campaign
            .queue_recruitment("normandy", UnitKind::Levy)
            .unwrap();
        campaign
            .queue_recruitment("normandy", UnitKind::Levy)
            .unwrap();
        campaign.end_turn().unwrap();
        // France's turn start does not touch England's pools.
        assert_eq!(province(&campaign, "normandy").recruitment_pool.levy, 0);
        campaign.end_turn().unwrap();
        assert_eq!(province(&campaign, "normandy").recruitment_pool.levy, 1);
        campaign.end_turn().unwrap();
        campaign.end_turn().unwrap();
        assert_eq!(province(&campaign, "normandy").recruitment_pool.levy, 2);
        campaign.end_turn().unwrap();
        campaign.end_turn().unwrap();
        assert_eq!(province(&campaign, "normandy").recruitment_pool.levy, 2);
    }

    #[test]
    fn a_completed_barracks_unlocks_spearmen_with_a_fresh_batch() {
        let mut campaign = new_campaign();
        campaign
            .queue_construction("normandy", BuildingId::Barracks)
            .unwrap();
        campaign.end_turn().unwrap();
        campaign.end_turn().unwrap();
        assert_eq!(
            province(&campaign, "normandy").building_level(BuildingId::Barracks),
            1
        );
        let spearmen = option(&campaign, "normandy", UnitKind::Spearmen);
        assert!(spearmen.available, "{:?}", spearmen.reason);
        assert_eq!((spearmen.pool, spearmen.pool_capacity), (1, 2));
        campaign
            .queue_recruitment("normandy", UnitKind::Spearmen)
            .unwrap();
    }

    #[test]
    fn pools_round_trip_and_overfull_pools_are_rejected() {
        let mut campaign = new_campaign();
        campaign
            .queue_recruitment("normandy", UnitKind::Archers)
            .unwrap();
        let save = CampaignSave::from_campaign(campaign, "england").unwrap();
        let loaded = CampaignSave::from_json(&save.to_json().unwrap()).unwrap();
        assert_eq!(loaded, save);
        assert_eq!(
            province(&loaded.campaign, "normandy")
                .recruitment_pool
                .archers,
            0
        );

        let mut overfull = save.clone();
        overfull.campaign.provinces[0].recruitment_pool.knights = 1;
        assert!(overfull.validate().is_err());
    }

    #[test]
    fn pre_pool_saves_load_with_full_pools() {
        let mut campaign = new_campaign();
        campaign
            .queue_recruitment("normandy", UnitKind::Archers)
            .unwrap();
        let save = CampaignSave::from_campaign(campaign, "england").unwrap();
        let mut document: serde_json::Value =
            serde_json::from_str(&save.to_json().unwrap()).unwrap();
        document["schemaVersion"] = serde_json::json!(4);
        for province in document["campaign"]["provinces"].as_array_mut().unwrap() {
            province.as_object_mut().unwrap().remove("recruitmentPool");
        }
        let migrated = CampaignSave::from_json(&document.to_string()).unwrap();
        for province in &migrated.campaign.provinces {
            assert_eq!(province.recruitment_pool, province.full_recruitment_pool());
        }

        document["schemaVersion"] = serde_json::json!(5);
        assert!(CampaignSave::from_json(&document.to_string()).is_err());
    }
}
