//! Core-owned province buildings, prerequisites, and construction queues.
//!
//! Every province holds at most one level of each [`BuildingId`]. The
//! [`BuildingSpec`] table is the single source of building categories, level
//! costs, durations, settlement and building prerequisites, and effects.
//! Construction is queued as [`ConstructionOrder`]s that complete at the start
//! of the owning faction's turn, before that turn's income is paid.

use serde::{Deserialize, Serialize};

use crate::{CampaignError, CampaignState, Province, SettlementLevel};

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BuildingId {
    Farms,
    TownHall,
    Barracks,
    Walls,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BuildingCategory {
    Economy,
    Administration,
    Military,
    Defense,
}

/// A building level that must already stand in the province.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BuildingRequirement {
    pub building: BuildingId,
    pub level: u8,
}

/// Requirements and effects of one building level.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BuildingLevelSpec {
    pub level: u8,
    pub label: &'static str,
    /// Gold paid by the owning faction when construction is queued.
    pub cost: u32,
    /// Full faction rounds until construction completes.
    pub rounds: u32,
    /// Minimum current settlement level of the province.
    pub required_settlement: SettlementLevel,
    /// Other buildings that must already stand at the given level.
    pub prerequisites: &'static [BuildingRequirement],
    /// Gold added to the owner's provincial income every faction turn while
    /// this level stands (replaces, not adds to, lower levels' bonus).
    pub income_bonus: u32,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BuildingSpec {
    pub building: BuildingId,
    pub category: BuildingCategory,
    pub label: &'static str,
    /// What the building contributes to the campaign today.
    pub role: &'static str,
    pub levels: &'static [BuildingLevelSpec],
}

pub const BUILDINGS: [BuildingId; 4] = [
    BuildingId::Farms,
    BuildingId::TownHall,
    BuildingId::Barracks,
    BuildingId::Walls,
];

const TOWN_HALL_1: &[BuildingRequirement] = &[BuildingRequirement {
    building: BuildingId::TownHall,
    level: 1,
}];

const FARMS: BuildingSpec = BuildingSpec {
    building: BuildingId::Farms,
    category: BuildingCategory::Economy,
    label: "Farms",
    role: "Adds provincial income.",
    levels: &[
        BuildingLevelSpec {
            level: 1,
            label: "Farmland",
            cost: 250,
            rounds: 1,
            required_settlement: SettlementLevel::Village,
            prerequisites: &[],
            income_bonus: 40,
        },
        BuildingLevelSpec {
            level: 2,
            label: "Irrigated fields",
            cost: 600,
            rounds: 2,
            required_settlement: SettlementLevel::Town,
            prerequisites: TOWN_HALL_1,
            income_bonus: 100,
        },
    ],
};

const TOWN_HALL: BuildingSpec = BuildingSpec {
    building: BuildingId::TownHall,
    category: BuildingCategory::Administration,
    label: "Town hall",
    role: "Administers the province; required for advanced buildings.",
    levels: &[
        BuildingLevelSpec {
            level: 1,
            label: "Reeve's hall",
            cost: 300,
            rounds: 1,
            required_settlement: SettlementLevel::Town,
            prerequisites: &[],
            income_bonus: 0,
        },
        BuildingLevelSpec {
            level: 2,
            label: "Guildhall",
            cost: 800,
            rounds: 2,
            required_settlement: SettlementLevel::City,
            prerequisites: &[],
            income_bonus: 0,
        },
    ],
};

const BARRACKS: BuildingSpec = BuildingSpec {
    building: BuildingId::Barracks,
    category: BuildingCategory::Military,
    label: "Barracks",
    role: "Military infrastructure for future unit unlocks.",
    levels: &[
        BuildingLevelSpec {
            level: 1,
            label: "Muster field",
            cost: 300,
            rounds: 1,
            required_settlement: SettlementLevel::Village,
            prerequisites: &[],
            income_bonus: 0,
        },
        BuildingLevelSpec {
            level: 2,
            label: "Barracks",
            cost: 700,
            rounds: 2,
            required_settlement: SettlementLevel::Town,
            prerequisites: TOWN_HALL_1,
            income_bonus: 0,
        },
    ],
};

const WALLS: BuildingSpec = BuildingSpec {
    building: BuildingId::Walls,
    category: BuildingCategory::Defense,
    label: "Walls",
    role: "Fortification for future siege profiles.",
    levels: &[
        BuildingLevelSpec {
            level: 1,
            label: "Palisade",
            cost: 400,
            rounds: 2,
            required_settlement: SettlementLevel::Town,
            prerequisites: &[],
            income_bonus: 0,
        },
        BuildingLevelSpec {
            level: 2,
            label: "Stone walls",
            cost: 1_000,
            rounds: 3,
            required_settlement: SettlementLevel::City,
            prerequisites: TOWN_HALL_1,
            income_bonus: 0,
        },
    ],
};

impl BuildingId {
    #[must_use]
    pub fn spec(self) -> BuildingSpec {
        match self {
            Self::Farms => FARMS,
            Self::TownHall => TOWN_HALL,
            Self::Barracks => BARRACKS,
            Self::Walls => WALLS,
        }
    }

    #[must_use]
    pub fn label(self) -> &'static str {
        self.spec().label
    }

    #[must_use]
    pub fn max_level(self) -> u8 {
        self.spec().levels.len() as u8
    }

    /// The spec of `level` (1-based), or `None` outside `1..=max_level`.
    #[must_use]
    pub fn level(self, level: u8) -> Option<BuildingLevelSpec> {
        let index = usize::from(level).checked_sub(1)?;
        self.spec().levels.get(index).copied()
    }
}

/// A building standing in a province at a level of at least 1.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProvinceBuilding {
    pub building: BuildingId,
    pub level: u8,
}

impl Province {
    /// The standing level of `building`, `0` when it is not built.
    #[must_use]
    pub fn building_level(&self, building: BuildingId) -> u8 {
        self.buildings
            .iter()
            .find(|standing| standing.building == building)
            .map_or(0, |standing| standing.level)
    }

    /// Income added by the province's standing buildings.
    #[must_use]
    pub fn building_income_bonus(&self) -> u32 {
        self.buildings
            .iter()
            .filter_map(|standing| standing.building.level(standing.level))
            .map(|level| level.income_bonus)
            .fold(0, u32::saturating_add)
    }

    /// Why a building cannot stand at `level` given the province's settlement
    /// level and other buildings, ignoring slots, owner, and treasury.
    pub(crate) fn building_level_error(&self, level: BuildingLevelSpec) -> Option<CampaignError> {
        if self.settlement_level < level.required_settlement {
            return Some(CampaignError::BuildingRequiresSettlement {
                province: self.name.clone(),
                building: level.label.to_owned(),
                required: level.required_settlement,
            });
        }
        level
            .prerequisites
            .iter()
            .find(|requirement| self.building_level(requirement.building) < requirement.level)
            .map(|requirement| CampaignError::BuildingPrerequisiteMissing {
                province: self.name.clone(),
                building: level.label.to_owned(),
                required: building_level_label(requirement.building, requirement.level),
            })
    }
}

fn building_level_label(building: BuildingId, level: u8) -> String {
    building
        .level(level)
        .map_or_else(|| building.label().to_owned(), |spec| spec.label.to_owned())
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConstructionOrder {
    pub faction_id: String,
    pub province_id: String,
    pub building: BuildingId,
    pub target_level: u8,
    pub cost: u32,
    /// Turn the order was queued; `ready_on_turn` is exactly the target
    /// level's build duration later.
    pub queued_on_turn: u32,
    pub ready_on_turn: u32,
}

/// The authoritative construction choice for one building in one province.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConstructionOption {
    pub building: BuildingId,
    pub category: BuildingCategory,
    pub label: &'static str,
    pub role: &'static str,
    pub current_level: u8,
    pub current: Option<BuildingLevelSpec>,
    pub target: Option<BuildingLevelSpec>,
    pub ready_on_turn: Option<u32>,
    pub available: bool,
    pub reason: Option<String>,
}

/// The authoritative construction view for one province.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProvinceConstruction {
    pub province_id: String,
    pub building_slots: u8,
    pub used_slots: u8,
    pub queued: Option<ConstructionOrder>,
    pub options: Vec<ConstructionOption>,
}

impl CampaignState {
    pub fn construction_options(
        &self,
        province_id: &str,
    ) -> Result<ProvinceConstruction, CampaignError> {
        let province = self.province(province_id)?;
        let treasury = self.faction(&self.active_faction)?.treasury;
        let options = BUILDINGS
            .iter()
            .map(|&building| {
                let current_level = province.building_level(building);
                let target = building.level(current_level + 1);
                let reason = self
                    .construction_error(province_id, building, treasury)
                    .err()
                    .map(|error| self.construction_reason(&error));
                let spec = building.spec();
                ConstructionOption {
                    building,
                    category: spec.category,
                    label: spec.label,
                    role: spec.role,
                    current_level,
                    current: building.level(current_level),
                    target,
                    ready_on_turn: target.map(|level| self.ready_on_turn_after(level.rounds)),
                    available: reason.is_none(),
                    reason,
                }
            })
            .collect();
        Ok(ProvinceConstruction {
            province_id: province.id.clone(),
            building_slots: province.settlement_level.spec().building_slots,
            used_slots: province.buildings.len() as u8,
            queued: self
                .construction_queue
                .iter()
                .find(|order| order.province_id == province.id)
                .cloned(),
            options,
        })
    }

    pub fn queue_construction(
        &mut self,
        province_id: &str,
        building: BuildingId,
    ) -> Result<(), CampaignError> {
        let faction_index = self
            .factions
            .iter()
            .position(|faction| faction.id == self.active_faction)
            .ok_or_else(|| CampaignError::FactionNotFound(self.active_faction.clone()))?;
        let target =
            self.construction_error(province_id, building, self.factions[faction_index].treasury)?;
        let province_name = self.province(province_id)?.name.clone();
        let ready_on_turn = self.ready_on_turn_after(target.rounds);

        self.factions[faction_index].treasury -= target.cost;
        self.construction_queue.push(ConstructionOrder {
            faction_id: self.active_faction.clone(),
            province_id: province_id.to_owned(),
            building,
            target_level: target.level,
            cost: target.cost,
            queued_on_turn: self.turn,
            ready_on_turn,
        });
        self.log.push(format!(
            "Turn {}: {} begins building {} in {} for {} gold, ready on turn {}.",
            self.turn,
            self.factions[faction_index].name,
            target.label,
            province_name,
            target.cost,
            ready_on_turn
        ));
        Ok(())
    }

    /// Completes the active faction's due construction. Completed orders leave
    /// the queue, so repeating the call for the same turn is a no-op.
    pub(crate) fn complete_construction(&mut self) -> Result<(), CampaignError> {
        let faction_id = self.active_faction.clone();
        let turn = self.turn;
        let due = |order: &ConstructionOrder| {
            order.faction_id == faction_id && order.ready_on_turn <= turn
        };
        let ready: Vec<ConstructionOrder> = self
            .construction_queue
            .iter()
            .filter(|order| due(order))
            .cloned()
            .collect();
        self.construction_queue.retain(|order| !due(order));

        for order in ready {
            let province = self
                .provinces
                .iter_mut()
                .find(|province| province.id == order.province_id)
                .ok_or_else(|| CampaignError::ProvinceNotFound(order.province_id.clone()))?;
            if province.owner != order.faction_id
                || province.building_level(order.building) + 1 != order.target_level
            {
                continue;
            }
            match province
                .buildings
                .iter_mut()
                .find(|standing| standing.building == order.building)
            {
                Some(standing) => standing.level = order.target_level,
                None => {
                    province.buildings.push(ProvinceBuilding {
                        building: order.building,
                        level: order.target_level,
                    });
                    province.buildings.sort_by_key(|standing| standing.building);
                }
            }
            let province_name = province.name.clone();
            let label = order
                .building
                .level(order.target_level)
                .map_or_else(|| order.building.label(), |level| level.label);
            self.log.push(format!(
                "Turn {}: {label} is completed in {province_name}.",
                self.turn
            ));
        }
        Ok(())
    }

    /// Cancels a former owner's construction after the province changes hands.
    pub(crate) fn cancel_construction_after_capture(&mut self, province_id: &str, new_owner: &str) {
        let queued = self.construction_queue.len();
        self.construction_queue
            .retain(|order| order.province_id != province_id || order.faction_id == new_owner);
        if self.construction_queue.len() < queued {
            self.log.push(format!(
                "Turn {}: construction in {province_id} is cancelled after capture.",
                self.turn
            ));
        }
    }

    /// The turn on which an order queued now for `rounds` full faction rounds
    /// completes: the start of the active faction's turn that many rounds on.
    pub(crate) fn ready_on_turn_after(&self, rounds: u32) -> u32 {
        self.turn
            .saturating_add(rounds.saturating_mul(self.factions.len() as u32))
    }

    fn construction_error(
        &self,
        province_id: &str,
        building: BuildingId,
        treasury: u32,
    ) -> Result<BuildingLevelSpec, CampaignError> {
        if self.pending_battle.is_some() {
            return Err(CampaignError::BattlePending);
        }
        let province = self.province(province_id)?;
        if province.owner != self.active_faction {
            return Err(CampaignError::SettlementNotControlled {
                province: province.name.clone(),
                owner: province.owner.clone(),
                active_faction: self.active_faction.clone(),
            });
        }
        if let Some(order) = self
            .construction_queue
            .iter()
            .find(|order| order.province_id == province.id)
        {
            return Err(CampaignError::ConstructionAlreadyQueued {
                province: province.name.clone(),
                building: building_level_label(order.building, order.target_level),
                ready_on_turn: order.ready_on_turn,
            });
        }
        let current_level = province.building_level(building);
        let Some(target) = building.level(current_level + 1) else {
            return Err(CampaignError::BuildingAtMaximumLevel {
                province: province.name.clone(),
                building: building.label().to_owned(),
            });
        };
        let slots = province.settlement_level.spec().building_slots;
        if current_level == 0 && province.buildings.len() >= usize::from(slots) {
            return Err(CampaignError::NoFreeBuildingSlot {
                province: province.name.clone(),
                slots,
            });
        }
        if let Some(error) = province.building_level_error(target) {
            return Err(error);
        }
        if treasury < target.cost {
            return Err(CampaignError::InsufficientTreasury {
                faction: self.faction_name(&self.active_faction),
                cost: target.cost,
                treasury,
            });
        }
        Ok(target)
    }

    fn construction_reason(&self, error: &CampaignError) -> String {
        match error {
            CampaignError::BattlePending => {
                "Resolve the pending battle before building.".to_owned()
            }
            CampaignError::SettlementNotControlled { province, .. } => format!(
                "Only {} can build during this turn, and it does not control {province}.",
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
    use crate::{CampaignSave, new_campaign};

    fn province<'a>(campaign: &'a CampaignState, id: &str) -> &'a Province {
        campaign
            .provinces
            .iter()
            .find(|province| province.id == id)
            .unwrap()
    }

    fn option(construction: &ProvinceConstruction, building: BuildingId) -> &ConstructionOption {
        construction
            .options
            .iter()
            .find(|option| option.building == building)
            .unwrap()
    }

    #[test]
    fn building_table_covers_every_category_with_increasing_levels() {
        let categories: Vec<_> = BUILDINGS
            .iter()
            .map(|building| building.spec().category)
            .collect();
        for category in [
            BuildingCategory::Economy,
            BuildingCategory::Administration,
            BuildingCategory::Military,
            BuildingCategory::Defense,
        ] {
            assert!(categories.contains(&category));
        }
        for building in BUILDINGS {
            let spec = building.spec();
            assert_eq!(spec.building, building);
            assert!(building.max_level() >= 1);
            for (index, level) in spec.levels.iter().enumerate() {
                assert_eq!(usize::from(level.level), index + 1);
                assert!(level.rounds >= 1);
                if index > 0 {
                    let previous = spec.levels[index - 1];
                    assert!(level.cost > previous.cost);
                    assert!(level.required_settlement >= previous.required_settlement);
                }
                for requirement in level.prerequisites {
                    assert_ne!(requirement.building, building);
                    let required = requirement.building.level(requirement.level).unwrap();
                    assert!(required.required_settlement <= level.required_settlement);
                }
            }
            assert_eq!(building.level(0), None);
            assert_eq!(building.level(building.max_level() + 1), None);
        }
    }

    #[test]
    fn options_report_slots_targets_and_core_reasons() {
        let campaign = new_campaign();
        let wessex = campaign.construction_options("wessex").unwrap();
        assert_eq!(wessex.building_slots, 1);
        assert_eq!(wessex.used_slots, 0);
        assert!(option(&wessex, BuildingId::Farms).available);
        assert_eq!(option(&wessex, BuildingId::Farms).ready_on_turn, Some(3));
        let walls = option(&wessex, BuildingId::Walls);
        assert!(!walls.available);
        assert!(walls.reason.as_deref().unwrap().contains("town"));

        let paris = campaign.construction_options("paris").unwrap();
        assert!(paris.options.iter().all(|option| !option.available));
        assert!(
            option(&paris, BuildingId::Farms)
                .reason
                .as_deref()
                .unwrap()
                .contains("does not control Paris")
        );
    }

    #[test]
    fn construction_pays_cost_and_completes_once_at_the_owners_turn_start() {
        let mut campaign = new_campaign();
        campaign
            .queue_construction("wessex", BuildingId::Farms)
            .unwrap();
        assert_eq!(campaign.factions[0].treasury, 950);
        assert!(matches!(
            campaign.queue_construction("wessex", BuildingId::Barracks),
            Err(CampaignError::ConstructionAlreadyQueued {
                ready_on_turn: 3,
                ..
            })
        ));

        campaign.end_turn().unwrap();
        assert_eq!(
            province(&campaign, "wessex").building_level(BuildingId::Farms),
            0
        );
        campaign.end_turn().unwrap();
        assert_eq!(
            province(&campaign, "wessex").building_level(BuildingId::Farms),
            1
        );
        assert!(campaign.construction_queue.is_empty());
        // Income on the completion turn already includes the farms:
        // wealth 11 * 50 + Normandy town 50 + farmland 40.
        assert_eq!(campaign.factions[0].treasury, 950 + 640);

        let before = campaign.clone();
        campaign.apply_turn_start().unwrap();
        campaign.complete_construction().unwrap();
        assert_eq!(campaign, before);
    }

    #[test]
    fn slots_settlement_levels_and_prerequisites_gate_construction() {
        let mut campaign = new_campaign();
        campaign.factions[0].treasury = 100_000;
        campaign.provinces[0].buildings = vec![ProvinceBuilding {
            building: BuildingId::Farms,
            level: 1,
        }];
        // A village has one slot, already used by farms.
        assert!(matches!(
            campaign.queue_construction("wessex", BuildingId::Barracks),
            Err(CampaignError::NoFreeBuildingSlot { slots: 1, .. })
        ));
        // Upgrading a standing building needs no new slot but a town and a hall.
        assert!(matches!(
            campaign.queue_construction("wessex", BuildingId::Farms),
            Err(CampaignError::BuildingRequiresSettlement {
                required: SettlementLevel::Town,
                ..
            })
        ));
        campaign.provinces[0].settlement_level = SettlementLevel::Town;
        assert!(matches!(
            campaign.queue_construction("wessex", BuildingId::Farms),
            Err(CampaignError::BuildingPrerequisiteMissing { .. })
        ));
        let reason = option(
            &campaign.construction_options("wessex").unwrap(),
            BuildingId::Farms,
        )
        .reason
        .clone()
        .unwrap();
        assert!(reason.contains("Reeve's hall"), "{reason}");

        campaign
            .queue_construction("wessex", BuildingId::TownHall)
            .unwrap();
        campaign.end_turn().unwrap();
        campaign.end_turn().unwrap();
        campaign
            .queue_construction("wessex", BuildingId::Farms)
            .unwrap();
        assert_eq!(campaign.construction_queue[0].target_level, 2);

        campaign.construction_queue.clear();
        campaign.provinces[0].buildings[0].level = 2;
        assert!(matches!(
            campaign.queue_construction("wessex", BuildingId::Farms),
            Err(CampaignError::BuildingAtMaximumLevel { .. })
        ));
    }

    #[test]
    fn captured_provinces_keep_buildings_but_cancel_the_former_owners_construction() {
        let mut campaign = new_campaign();
        campaign.provinces[1].buildings = vec![ProvinceBuilding {
            building: BuildingId::Farms,
            level: 1,
        }];
        campaign
            .queue_construction("normandy", BuildingId::Barracks)
            .unwrap();
        campaign.provinces[1].owner = "france".into();
        campaign.cancel_construction_after_capture("normandy", "france");
        assert!(campaign.construction_queue.is_empty());
        assert_eq!(
            province(&campaign, "normandy").building_level(BuildingId::Farms),
            1
        );
    }

    #[test]
    fn construction_round_trips_through_saves_and_invalid_state_is_rejected() {
        let mut campaign = new_campaign();
        campaign.provinces[1].buildings = vec![ProvinceBuilding {
            building: BuildingId::TownHall,
            level: 1,
        }];
        campaign
            .queue_construction("normandy", BuildingId::Walls)
            .unwrap();
        let save = CampaignSave::from_campaign(campaign, "england").unwrap();
        assert_eq!(
            CampaignSave::from_json(&save.to_json().unwrap()).unwrap(),
            save
        );
        // Palisades take two rounds: turn 1 + 2 * 2 factions = turn 5.
        assert_eq!(save.campaign.construction_queue[0].ready_on_turn, 5);

        let mut skipped = save.clone();
        skipped.campaign.construction_queue[0].target_level = 2;
        assert!(skipped.validate().is_err());

        let mut foreign = save.clone();
        foreign.campaign.construction_queue[0].faction_id = "france".into();
        assert!(foreign.validate().is_err());

        let mut duplicate = save.clone();
        let mut second = duplicate.campaign.construction_queue[0].clone();
        second.building = BuildingId::Farms;
        second.target_level = 1;
        second.ready_on_turn = 3;
        duplicate.campaign.construction_queue.push(second);
        assert!(duplicate.validate().is_err());

        let mut off_phase = save.clone();
        off_phase.campaign.construction_queue[0].ready_on_turn = 4;
        assert!(off_phase.validate().is_err());

        let mut full = save.clone();
        full.campaign.provinces[1].buildings.push(ProvinceBuilding {
            building: BuildingId::Farms,
            level: 1,
        });
        assert!(full.validate().is_err(), "a town has only two slots");

        let mut village = save.clone();
        village.campaign.construction_queue.clear();
        village.campaign.provinces[0].buildings = vec![ProvinceBuilding {
            building: BuildingId::Walls,
            level: 1,
        }];
        assert!(village.validate().is_err(), "walls need a town");

        let mut missing_hall = save.clone();
        missing_hall.campaign.construction_queue.clear();
        missing_hall.campaign.provinces[1].settlement_level = SettlementLevel::City;
        missing_hall.campaign.provinces[1].buildings = vec![ProvinceBuilding {
            building: BuildingId::Walls,
            level: 2,
        }];
        assert!(missing_hall.validate().is_err());

        let mut out_of_range = save.clone();
        out_of_range.campaign.provinces[1].buildings[0].level = 3;
        assert!(out_of_range.validate().is_err());

        let mut repeated = save;
        repeated.campaign.construction_queue.clear();
        repeated.campaign.provinces[1]
            .buildings
            .push(ProvinceBuilding {
                building: BuildingId::TownHall,
                level: 1,
            });
        assert!(repeated.validate().is_err());
    }

    #[test]
    fn pre_construction_saves_load_without_buildings_and_cannot_smuggle_them() {
        let mut campaign = new_campaign();
        campaign
            .queue_construction("wessex", BuildingId::Farms)
            .unwrap();
        let save = CampaignSave::from_campaign(campaign, "england").unwrap();
        let current = serde_json::to_value(&save).unwrap();

        let mut relabeled = current.clone();
        relabeled["schemaVersion"] = serde_json::json!(3);
        assert!(CampaignSave::from_json(&relabeled.to_string()).is_err());

        let mut queued = relabeled.clone();
        for province in queued["campaign"]["provinces"].as_array_mut().unwrap() {
            province.as_object_mut().unwrap().remove("buildings");
        }
        assert!(CampaignSave::from_json(&queued.to_string()).is_err());

        let mut legacy = queued;
        legacy["campaign"]
            .as_object_mut()
            .unwrap()
            .remove("constructionQueue");
        let loaded = CampaignSave::from_json(&legacy.to_string()).unwrap();
        assert_eq!(loaded.schema_version, crate::CAMPAIGN_SAVE_SCHEMA_VERSION);
        assert!(
            loaded
                .campaign
                .provinces
                .iter()
                .all(|province| province.buildings.is_empty())
        );

        let mut missing = current;
        missing["campaign"]["provinces"][0]
            .as_object_mut()
            .unwrap()
            .remove("buildings");
        assert!(CampaignSave::from_json(&missing.to_string()).is_err());
    }
}
