use std::fmt;

use serde::{Deserialize, Serialize};

mod battle;
mod buildings;
pub use buildings::{
    BUILDINGS, BuildingCategory, BuildingId, BuildingLevelSpec, BuildingRequirement, BuildingSpec,
    ConstructionOption, ConstructionOrder, ProvinceBuilding, ProvinceConstruction,
};
mod campaign_deployment;
mod campaign_handoff;
mod campaign_reconciliation;
mod campaign_resolution;
mod charge;
mod deployment;
pub use charge::CavalryChargeState;
mod formation_orders;
pub use formation_orders::FormationOrder;
mod facing;
pub use facing::{CombatArc, Facing};
mod group_orders;
pub use group_orders::{GroupMovementOrder, MAX_QUEUED_WAYPOINTS, MovementMode, MovementWaypoint};
mod province_definitions;
mod recruitment;
pub use province_definitions::{
    BattlefieldDefinition, PROVINCE_DEFINITION_SCHEMA_VERSION, ProvinceDefinition,
    ProvinceDefinitionDocument, ProvinceDefinitionError, ProvinceDefinitions,
    ProvinceEconomyDefinition, SettlementDefinition, default_province_definitions,
};
pub use recruitment::{RecruitmentOption, RecruitmentPool, UnitUnlock};
mod save;
pub use deployment::siege::SiegeProfile;
mod settlement;
pub use settlement::{
    SETTLEMENT_LEVELS, SettlementLevel, SettlementLevelSpec, SettlementUpgradeOption,
    SettlementUpgradeOrder, SettlementUpgradeRequirement,
};
mod tactical;
mod tactical_opponent;
mod tactical_work;
pub use tactical_work::TacticalWorkCounters;
mod tactical_result;
mod terrain;
mod unit_stats;
pub use battle::{ArmyRoster, BattleOutcome, BattleReport};
pub use campaign_deployment::{
    FormationFootprint, ProvinceBattlefieldContext, TacticalBattlefieldProfile,
};
pub use campaign_handoff::{
    TacticalArmySeed, TacticalBattleSeed, TacticalForceSeed, TacticalUnitSeed,
};
pub use campaign_resolution::{
    TacticalCampaignReport, TacticalCampaignRetreat, TacticalCampaignSurrender,
};
pub use deployment::{DeploymentZone, standard_deployment_zone, standard_deployment_zones};
pub use save::{CAMPAIGN_SAVE_SCHEMA_VERSION, CampaignSave, SaveError};
pub use tactical::{
    BattlePoint, BattleSide, FlatBattlefield, Formation, MovementOrder, TACTICAL_TICKS_PER_SECOND,
    TacticalBattle, TacticalBattleState, TacticalError, TacticalFinishReason, TacticalUnit,
    TacticalUnitProvenance,
};
pub use tactical_result::{
    TACTICAL_BATTLE_RESULT_SCHEMA_VERSION, TacticalAmmunitionResult, TacticalArmyResult,
    TacticalBattleResult, TacticalResultError, TacticalSettlementCapture, TacticalUnitResult,
};
pub use terrain::{
    BattlefieldLocation, TACTICAL_FOREST_CELL_COUNT, TACTICAL_TERRAIN_GRID_SIZE,
    TacticalGroundCover, TacticalTerrain, TacticalTerrainCell, TacticalTerrainProfile,
};
pub use unit_stats::{MissileStats, UnitCombatProfile, UnitStats, UnitStatsVersion};

const INCOME_PER_WEALTH: u32 = 50;
pub(crate) const UNIT_KINDS: [UnitKind; 4] = [
    UnitKind::Levy,
    UnitKind::Spearmen,
    UnitKind::Archers,
    UnitKind::Knights,
];

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CampaignState {
    pub year: u16,
    pub turn: u32,
    pub active_faction: String,
    pub factions: Vec<Faction>,
    pub provinces: Vec<Province>,
    pub armies: Vec<Army>,
    pub pending_battle: Option<PendingBattle>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pending_tactical_result: Option<TacticalBattleResult>,
    pub recruitment_queue: Vec<RecruitmentOrder>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub settlement_upgrades: Vec<SettlementUpgradeOrder>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub construction_queue: Vec<ConstructionOrder>,
    pub battle_reports: Vec<BattleReport>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tactical_battle_reports: Vec<TacticalCampaignReport>,
    pub log: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Faction {
    pub id: String,
    pub name: String,
    pub treasury: u32,
    pub last_economy_turn: Option<u32>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Province {
    pub id: String,
    pub name: String,
    pub owner: String,
    pub wealth: u32,
    pub neighbors: Vec<String>,
    #[serde(default)]
    pub battlefield: ProvinceBattlefieldContext,
    /// Saves from before settlement levels load every province as a village.
    #[serde(default)]
    pub settlement_level: SettlementLevel,
    /// Standing buildings ordered by [`BuildingId`]; saves from before
    /// construction load without buildings.
    #[serde(default)]
    pub buildings: Vec<ProvinceBuilding>,
    /// Recruitable batches per unit; saves from before recruitment pools load
    /// with every unlocked pool full.
    #[serde(default)]
    pub recruitment_pool: RecruitmentPool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Army {
    pub id: String,
    pub owner: String,
    pub province: String,
    pub levy: u16,
    pub spearmen: u16,
    pub archers: u16,
    pub knights: u16,
    pub moved_this_turn: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingBattle {
    pub attacker_army_id: String,
    pub attacker_faction: String,
    pub from_province: String,
    pub target_province: String,
    pub defender_faction: String,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum UnitKind {
    Levy,
    Spearmen,
    Archers,
    Knights,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecruitmentOrder {
    pub faction_id: String,
    pub province_id: String,
    pub unit: UnitKind,
    pub label: String,
    pub cost: u32,
    pub soldiers: u16,
    pub ready_on_turn: u32,
}

#[derive(Copy, Clone)]
struct UnitSpec {
    label: &'static str,
    cost: u32,
    soldiers: u16,
}

impl UnitKind {
    #[must_use]
    pub fn label(self) -> &'static str {
        self.spec().label
    }

    #[must_use]
    pub fn recruitment_cost(self) -> u32 {
        self.spec().cost
    }

    fn spec(self) -> UnitSpec {
        match self {
            Self::Levy => UnitSpec {
                label: "Levy",
                cost: 120,
                soldiers: 40,
            },
            Self::Spearmen => UnitSpec {
                label: "Spearmen",
                cost: 220,
                soldiers: 30,
            },
            Self::Archers => UnitSpec {
                label: "Archers",
                cost: 260,
                soldiers: 20,
            },
            Self::Knights => UnitSpec {
                label: "Knights",
                cost: 500,
                soldiers: 10,
            },
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CampaignError {
    ArmyNotFound(String),
    FactionNotFound(String),
    ProvinceNotFound(String),
    NotActiveFaction {
        army_owner: String,
        active_faction: String,
    },
    ArmyAlreadyMoved(String),
    BattlePending,
    NoPendingBattle,
    InvalidTacticalResult(String),
    TacticalResultMismatch,
    TacticalCasualtiesAlreadyReconciled,
    DestinationNotAdjacent {
        from: String,
        destination: String,
    },
    ProvinceNotOwned {
        province: String,
        owner: String,
        active_faction: String,
    },
    UnitLocked {
        province: String,
        unit: UnitKind,
        requirement: String,
    },
    RecruitmentPoolEmpty {
        province: String,
        unit: UnitKind,
    },
    InsufficientTreasury {
        faction: String,
        cost: u32,
        treasury: u32,
    },
    SettlementNotControlled {
        province: String,
        owner: String,
        active_faction: String,
    },
    SettlementUpgradeAlreadyQueued {
        province: String,
        ready_on_turn: u32,
    },
    SettlementAtMaximumLevel(String),
    SettlementWealthTooLow {
        province: String,
        wealth: u32,
        required: u32,
    },
    ConstructionAlreadyQueued {
        province: String,
        building: String,
        ready_on_turn: u32,
    },
    BuildingAtMaximumLevel {
        province: String,
        building: String,
    },
    NoFreeBuildingSlot {
        province: String,
        slots: u8,
    },
    BuildingRequiresSettlement {
        province: String,
        building: String,
        required: SettlementLevel,
    },
    BuildingPrerequisiteMissing {
        province: String,
        building: String,
        required: String,
    },
}

impl fmt::Display for CampaignError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ArmyNotFound(army_id) => write!(formatter, "army {army_id} does not exist"),
            Self::FactionNotFound(faction_id) => {
                write!(formatter, "faction {faction_id} does not exist")
            }
            Self::ProvinceNotFound(province_id) => {
                write!(formatter, "province {province_id} does not exist")
            }
            Self::NotActiveFaction {
                army_owner,
                active_faction,
            } => write!(
                formatter,
                "{army_owner} cannot move while {active_faction} is the active faction"
            ),
            Self::ArmyAlreadyMoved(army_id) => {
                write!(formatter, "army {army_id} has already moved this turn")
            }
            Self::BattlePending => {
                write!(formatter, "resolve the pending battle before continuing")
            }
            Self::NoPendingBattle => write!(formatter, "there is no pending battle to resolve"),
            Self::InvalidTacticalResult(message) => {
                write!(formatter, "invalid tactical result: {message}")
            }
            Self::TacticalResultMismatch => write!(
                formatter,
                "tactical result does not match the pending campaign battle"
            ),
            Self::TacticalCasualtiesAlreadyReconciled => write!(
                formatter,
                "finish the reconciled tactical battle before resolving another result"
            ),
            Self::DestinationNotAdjacent { from, destination } => {
                write!(formatter, "{destination} is not adjacent to {from}")
            }
            Self::ProvinceNotOwned {
                province,
                owner,
                active_faction,
            } => write!(
                formatter,
                "{active_faction} cannot recruit in {province}, which is controlled by {owner}"
            ),
            Self::UnitLocked {
                province,
                unit,
                requirement,
            } => write!(
                formatter,
                "{} in {province} require {requirement}",
                unit.spec().label
            ),
            Self::RecruitmentPoolEmpty { province, unit } => write!(
                formatter,
                "no {} batch is available in {province} until its pool replenishes",
                unit.spec().label
            ),
            Self::InsufficientTreasury {
                faction,
                cost,
                treasury,
            } => write!(
                formatter,
                "{faction} needs {cost} gold but has only {treasury}"
            ),
            Self::SettlementNotControlled {
                province,
                owner,
                active_faction,
            } => write!(
                formatter,
                "{active_faction} cannot upgrade {province}, which is controlled by {owner}"
            ),
            Self::SettlementUpgradeAlreadyQueued {
                province,
                ready_on_turn,
            } => write!(
                formatter,
                "{province} is already upgrading and is ready on turn {ready_on_turn}"
            ),
            Self::SettlementAtMaximumLevel(province) => {
                write!(
                    formatter,
                    "{province} is already at the highest settlement level"
                )
            }
            Self::SettlementWealthTooLow {
                province,
                wealth,
                required,
            } => write!(
                formatter,
                "{province} needs wealth {required} to upgrade but has {wealth}"
            ),
            Self::ConstructionAlreadyQueued {
                province,
                building,
                ready_on_turn,
            } => write!(
                formatter,
                "{province} is already building {building}, ready on turn {ready_on_turn}"
            ),
            Self::BuildingAtMaximumLevel { province, building } => write!(
                formatter,
                "{building} in {province} is already at its highest level"
            ),
            Self::NoFreeBuildingSlot { province, slots } => write!(
                formatter,
                "{province} has no free building slot; all {slots} are in use"
            ),
            Self::BuildingRequiresSettlement {
                province,
                building,
                required,
            } => write!(
                formatter,
                "{building} in {province} requires a {}",
                required.label().to_lowercase()
            ),
            Self::BuildingPrerequisiteMissing {
                province,
                building,
                required,
            } => write!(formatter, "{building} in {province} requires {required}"),
        }
    }
}

impl std::error::Error for CampaignError {}

impl CampaignState {
    pub fn legal_destinations(&self, army_id: &str) -> Result<Vec<String>, CampaignError> {
        if self.pending_battle.is_some() {
            return Err(CampaignError::BattlePending);
        }

        let army = self
            .armies
            .iter()
            .find(|army| army.id == army_id)
            .ok_or_else(|| CampaignError::ArmyNotFound(army_id.to_owned()))?;

        if army.owner != self.active_faction {
            return Err(CampaignError::NotActiveFaction {
                army_owner: army.owner.clone(),
                active_faction: self.active_faction.clone(),
            });
        }

        if army.moved_this_turn {
            return Err(CampaignError::ArmyAlreadyMoved(army.id.clone()));
        }

        let province = self.province(&army.province)?;
        Ok(province.neighbors.clone())
    }

    pub fn move_army(&mut self, army_id: &str, destination: &str) -> Result<(), CampaignError> {
        if self.pending_battle.is_some() {
            return Err(CampaignError::BattlePending);
        }

        let army_index = self
            .armies
            .iter()
            .position(|army| army.id == army_id)
            .ok_or_else(|| CampaignError::ArmyNotFound(army_id.to_owned()))?;

        let (attacker_army_id, army_owner, from_province, moved_this_turn) = {
            let army = &self.armies[army_index];
            (
                army.id.clone(),
                army.owner.clone(),
                army.province.clone(),
                army.moved_this_turn,
            )
        };

        if army_owner != self.active_faction {
            return Err(CampaignError::NotActiveFaction {
                army_owner,
                active_faction: self.active_faction.clone(),
            });
        }

        if moved_this_turn {
            return Err(CampaignError::ArmyAlreadyMoved(attacker_army_id));
        }

        let source = self.province(&from_province)?;
        if !source
            .neighbors
            .iter()
            .any(|neighbor| neighbor == destination)
        {
            return Err(CampaignError::DestinationNotAdjacent {
                from: from_province,
                destination: destination.to_owned(),
            });
        }

        let destination_province = self.province(destination)?;
        let source_name = source.name.clone();
        let destination_name = destination_province.name.clone();
        let destination_owner = destination_province.owner.clone();

        self.armies[army_index].moved_this_turn = true;

        if destination_owner == army_owner {
            self.armies[army_index].province = destination.to_owned();
            self.log.push(format!(
                "Turn {}: {} moves from {} to {}.",
                self.turn, attacker_army_id, source_name, destination_name
            ));
        } else {
            self.pending_battle = Some(PendingBattle {
                attacker_army_id: attacker_army_id.clone(),
                attacker_faction: army_owner,
                from_province: self.armies[army_index].province.clone(),
                target_province: destination.to_owned(),
                defender_faction: destination_owner,
            });
            self.log.push(format!(
                "Turn {}: {} marches from {} into hostile {}. Battle pending.",
                self.turn, attacker_army_id, source_name, destination_name
            ));
        }

        Ok(())
    }

    pub fn end_turn(&mut self) -> Result<(), CampaignError> {
        if self.pending_battle.is_some() {
            return Err(CampaignError::BattlePending);
        }

        if self.factions.is_empty() {
            return Ok(());
        }

        let current_index = self
            .factions
            .iter()
            .position(|faction| faction.id == self.active_faction)
            .unwrap_or(0);
        let next_index = (current_index + 1) % self.factions.len();

        self.turn += 1;
        if next_index == 0 {
            self.year += 1;
        }

        self.active_faction = self.factions[next_index].id.clone();
        for army in self
            .armies
            .iter_mut()
            .filter(|army| army.owner == self.active_faction)
        {
            army.moved_this_turn = false;
        }

        self.log.push(format!(
            "Turn {}: {} begins its turn.",
            self.turn, self.factions[next_index].name
        ));
        self.apply_turn_start()?;
        Ok(())
    }

    fn apply_turn_start(&mut self) -> Result<(), CampaignError> {
        let faction_id = self.active_faction.clone();
        let faction_index = self
            .factions
            .iter()
            .position(|faction| faction.id == faction_id)
            .ok_or_else(|| CampaignError::FactionNotFound(faction_id.clone()))?;

        if self.factions[faction_index].last_economy_turn == Some(self.turn) {
            return Ok(());
        }

        self.complete_settlement_upgrades()?;
        self.complete_construction()?;
        self.replenish_recruitment_pools();
        let income = self.income_for(&faction_id);
        self.factions[faction_index].treasury =
            self.factions[faction_index].treasury.saturating_add(income);
        self.factions[faction_index].last_economy_turn = Some(self.turn);
        let faction_name = self.factions[faction_index].name.clone();
        self.log.push(format!(
            "Turn {}: {} receives {} gold in provincial income.",
            self.turn, faction_name, income
        ));

        let ready_orders: Vec<RecruitmentOrder> = self
            .recruitment_queue
            .iter()
            .filter(|order| order.faction_id == faction_id && order.ready_on_turn <= self.turn)
            .cloned()
            .collect();
        self.recruitment_queue
            .retain(|order| !(order.faction_id == faction_id && order.ready_on_turn <= self.turn));

        for order in ready_orders {
            self.complete_recruitment(order)?;
        }

        Ok(())
    }

    fn complete_recruitment(&mut self, order: RecruitmentOrder) -> Result<(), CampaignError> {
        let province_name = self.province(&order.province_id)?.name.clone();
        let army_index =
            if let Some(index) = self.armies.iter().position(|army| {
                army.owner == order.faction_id && army.province == order.province_id
            }) {
                index
            } else {
                self.armies.push(Army {
                    id: format!(
                        "{}-{}-recruits-t{}",
                        order.faction_id, order.province_id, self.turn
                    ),
                    owner: order.faction_id.clone(),
                    province: order.province_id.clone(),
                    levy: 0,
                    spearmen: 0,
                    archers: 0,
                    knights: 0,
                    moved_this_turn: false,
                });
                self.armies.len() - 1
            };

        let army = &mut self.armies[army_index];
        match order.unit {
            UnitKind::Levy => army.levy = army.levy.saturating_add(order.soldiers),
            UnitKind::Spearmen => army.spearmen = army.spearmen.saturating_add(order.soldiers),
            UnitKind::Archers => army.archers = army.archers.saturating_add(order.soldiers),
            UnitKind::Knights => army.knights = army.knights.saturating_add(order.soldiers),
        }

        self.log.push(format!(
            "Turn {}: {} {} complete recruitment in {}.",
            self.turn, order.soldiers, order.label, province_name
        ));
        Ok(())
    }

    fn income_for(&self, faction_id: &str) -> u32 {
        self.provinces
            .iter()
            .filter(|province| province.owner == faction_id)
            .map(|province| {
                province
                    .wealth
                    .saturating_mul(INCOME_PER_WEALTH)
                    .saturating_add(province.settlement_level.spec().income_bonus)
                    .saturating_add(province.building_income_bonus())
            })
            .fold(0_u32, u32::saturating_add)
    }

    fn faction(&self, faction_id: &str) -> Result<&Faction, CampaignError> {
        self.factions
            .iter()
            .find(|faction| faction.id == faction_id)
            .ok_or_else(|| CampaignError::FactionNotFound(faction_id.to_owned()))
    }

    fn faction_name(&self, faction_id: &str) -> String {
        self.factions
            .iter()
            .find(|faction| faction.id == faction_id)
            .map_or_else(|| faction_id.to_owned(), |faction| faction.name.clone())
    }

    fn province(&self, province_id: &str) -> Result<&Province, CampaignError> {
        self.provinces
            .iter()
            .find(|province| province.id == province_id)
            .ok_or_else(|| CampaignError::ProvinceNotFound(province_id.to_owned()))
    }
}

#[must_use]
pub fn new_campaign() -> CampaignState {
    new_campaign_with_province_definitions(default_province_definitions())
        .expect("packaged province definitions must match the starting campaign")
}

pub fn new_campaign_with_province_definitions(
    definitions: &ProvinceDefinitions,
) -> Result<CampaignState, ProvinceDefinitionError> {
    let factions = vec![
        Faction {
            id: "england".into(),
            name: "Kingdom of England".into(),
            treasury: 1_200,
            last_economy_turn: Some(1),
        },
        Faction {
            id: "france".into(),
            name: "Kingdom of France".into(),
            treasury: 1_200,
            last_economy_turn: None,
        },
    ];
    let armies = vec![
        Army {
            id: "england-main".into(),
            owner: "england".into(),
            province: "normandy".into(),
            levy: 120,
            spearmen: 80,
            archers: 40,
            knights: 20,
            moved_this_turn: false,
        },
        Army {
            id: "france-main".into(),
            owner: "france".into(),
            province: "paris".into(),
            levy: 120,
            spearmen: 80,
            archers: 40,
            knights: 20,
            moved_this_turn: false,
        },
    ];
    let mut provinces = definitions.build_provinces(&factions, &armies)?;
    for province in &mut provinces {
        province.recruitment_pool = province.full_recruitment_pool();
    }
    Ok(CampaignState {
        year: 1087,
        turn: 1,
        active_faction: "england".into(),
        factions,
        provinces,
        armies,
        pending_battle: None,
        pending_tactical_result: None,
        recruitment_queue: Vec::new(),
        settlement_upgrades: Vec::new(),
        construction_queue: Vec::new(),
        battle_reports: Vec::new(),
        tactical_battle_reports: Vec::new(),
        log: vec!["The campaign begins in 1087.".into()],
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn campaign_bootstrap_is_deterministic() {
        assert_eq!(new_campaign(), new_campaign());
    }

    #[test]
    fn a_full_round_advances_the_year() {
        let mut campaign = new_campaign();

        campaign.end_turn().unwrap();
        assert_eq!(campaign.year, 1087);
        assert_eq!(campaign.active_faction, "france");

        campaign.end_turn().unwrap();
        assert_eq!(campaign.year, 1088);
        assert_eq!(campaign.active_faction, "england");
        assert_eq!(campaign.turn, 3);
    }

    #[test]
    fn legal_destinations_come_from_authoritative_adjacency() {
        let campaign = new_campaign();

        assert_eq!(
            campaign.legal_destinations("england-main").unwrap(),
            vec!["wessex", "brittany", "paris"]
        );
    }

    #[test]
    fn inactive_faction_cannot_move() {
        let campaign = new_campaign();

        assert!(matches!(
            campaign.legal_destinations("france-main"),
            Err(CampaignError::NotActiveFaction { .. })
        ));
    }

    #[test]
    fn friendly_movement_relocates_once_per_turn() {
        let mut campaign = new_campaign();

        campaign.move_army("england-main", "wessex").unwrap();

        let army = campaign
            .armies
            .iter()
            .find(|army| army.id == "england-main")
            .unwrap();
        assert_eq!(army.province, "wessex");
        assert!(army.moved_this_turn);
        assert!(matches!(
            campaign.legal_destinations("england-main"),
            Err(CampaignError::ArmyAlreadyMoved(_))
        ));
    }

    #[test]
    fn non_adjacent_movement_is_rejected() {
        let mut campaign = new_campaign();

        assert!(matches!(
            campaign.move_army("england-main", "flanders"),
            Err(CampaignError::DestinationNotAdjacent { .. })
        ));
    }

    #[test]
    fn hostile_movement_creates_pending_battle_without_resolving_it() {
        let mut campaign = new_campaign();

        campaign.move_army("england-main", "brittany").unwrap();

        let army = campaign
            .armies
            .iter()
            .find(|army| army.id == "england-main")
            .unwrap();
        assert_eq!(army.province, "normandy");
        assert!(army.moved_this_turn);
        assert_eq!(
            campaign.pending_battle,
            Some(PendingBattle {
                attacker_army_id: "england-main".into(),
                attacker_faction: "england".into(),
                from_province: "normandy".into(),
                target_province: "brittany".into(),
                defender_faction: "france".into(),
            })
        );
        assert!(matches!(
            campaign.end_turn(),
            Err(CampaignError::BattlePending)
        ));
    }

    #[test]
    fn movement_resets_when_faction_becomes_active_again() {
        let mut campaign = new_campaign();

        campaign.move_army("england-main", "wessex").unwrap();
        campaign.end_turn().unwrap();
        campaign.end_turn().unwrap();

        assert_eq!(campaign.active_faction, "england");
        assert!(
            !campaign
                .armies
                .iter()
                .find(|army| army.id == "england-main")
                .unwrap()
                .moved_this_turn
        );
    }

    #[test]
    fn provincial_income_is_paid_once_per_faction_turn() {
        let mut campaign = new_campaign();

        campaign.end_turn().unwrap();
        let france = campaign.faction("france").unwrap();
        // Wealth 23 * 50 plus the Paris city bonus of 100.
        assert_eq!(france.treasury, 2_450);
        assert_eq!(france.last_economy_turn, Some(2));

        campaign.apply_turn_start().unwrap();
        assert_eq!(campaign.faction("france").unwrap().treasury, 2_450);
    }

    #[test]
    fn recruitment_deducts_cost_and_consumes_the_local_pool() {
        let mut campaign = new_campaign();
        let options = campaign.recruitment_options("normandy").unwrap();
        let levy = options
            .iter()
            .find(|option| option.unit == UnitKind::Levy)
            .unwrap();
        assert_eq!(levy.cost, 120);
        assert_eq!(levy.soldiers, 40);
        assert!(levy.available);

        campaign
            .queue_recruitment("normandy", UnitKind::Levy)
            .unwrap();
        assert_eq!(campaign.faction("england").unwrap().treasury, 1_080);
        assert_eq!(campaign.recruitment_queue.len(), 1);
        assert_eq!(campaign.provinces[1].recruitment_pool.levy, 1);
    }

    #[test]
    fn recruitment_options_return_authoritative_unavailable_reasons() {
        let mut campaign = new_campaign();
        campaign.factions[0].treasury = 0;

        let own_options = campaign.recruitment_options("normandy").unwrap();
        assert!(own_options.iter().all(|option| !option.available));
        assert!(own_options.iter().all(|option| option.reason.is_some()));

        let enemy_options = campaign.recruitment_options("paris").unwrap();
        assert!(enemy_options.iter().all(|option| !option.available));
        assert!(enemy_options.iter().all(|option| {
            option
                .reason
                .as_deref()
                .unwrap()
                .contains("does not control")
        }));
    }

    #[test]
    fn one_turn_recruitment_completes_once_when_faction_returns() {
        let mut campaign = new_campaign();
        campaign
            .queue_recruitment("normandy", UnitKind::Archers)
            .unwrap();

        campaign.end_turn().unwrap();
        campaign.end_turn().unwrap();

        let army = campaign
            .armies
            .iter()
            .find(|army| army.id == "england-main")
            .unwrap();
        assert_eq!(army.archers, 60);
        assert!(campaign.recruitment_queue.is_empty());

        campaign.apply_turn_start().unwrap();
        let army = campaign
            .armies
            .iter()
            .find(|army| army.id == "england-main")
            .unwrap();
        assert_eq!(army.archers, 60);
    }

    #[test]
    fn recruited_army_ids_remain_unique_after_armies_move() {
        let mut campaign = new_campaign();
        campaign
            .queue_recruitment("wessex", UnitKind::Levy)
            .unwrap();
        campaign.end_turn().unwrap();
        campaign.end_turn().unwrap();

        let first_id = campaign
            .armies
            .iter()
            .find(|army| army.owner == "england" && army.province == "wessex")
            .unwrap()
            .id
            .clone();
        campaign.move_army(&first_id, "normandy").unwrap();
        campaign
            .queue_recruitment("wessex", UnitKind::Levy)
            .unwrap();
        campaign.end_turn().unwrap();
        campaign.end_turn().unwrap();

        let second_id = campaign
            .armies
            .iter()
            .find(|army| army.owner == "england" && army.province == "wessex")
            .unwrap()
            .id
            .clone();
        assert_ne!(first_id, second_id);
    }
}
