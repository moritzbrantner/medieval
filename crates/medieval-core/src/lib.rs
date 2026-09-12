use std::fmt;

use serde::{Deserialize, Serialize};

mod battle;
mod deployment;
mod save;
mod tactical;
mod terrain;
pub use battle::{ArmyRoster, BattleOutcome, BattleReport};
pub use deployment::{DeploymentZone, standard_deployment_zone, standard_deployment_zones};
pub use save::{CAMPAIGN_SAVE_SCHEMA_VERSION, CampaignSave, SaveError};
pub use tactical::{
    BattlePoint, BattleSide, FlatBattlefield, Formation, MovementOrder, TACTICAL_TICKS_PER_SECOND,
    TacticalBattle, TacticalError, TacticalUnit,
};
pub use terrain::{
    TACTICAL_FOREST_CELL_COUNT, TACTICAL_TERRAIN_GRID_SIZE, TacticalGroundCover, TacticalTerrain,
    TacticalTerrainCell, TacticalTerrainProfile,
};

const INCOME_PER_WEALTH: u32 = 50;
const UNIT_KINDS: [UnitKind; 4] = [
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
    pub recruitment_queue: Vec<RecruitmentOrder>,
    pub battle_reports: Vec<BattleReport>,
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
pub struct RecruitmentOption {
    pub unit: UnitKind,
    pub label: String,
    pub cost: u32,
    pub soldiers: u16,
    pub available: bool,
    pub reason: Option<String>,
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
    DestinationNotAdjacent {
        from: String,
        destination: String,
    },
    ProvinceNotOwned {
        province: String,
        owner: String,
        active_faction: String,
    },
    RecruitmentAlreadyQueued {
        province: String,
        unit: UnitKind,
    },
    InsufficientTreasury {
        faction: String,
        cost: u32,
        treasury: u32,
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
            Self::RecruitmentAlreadyQueued { province, unit } => write!(
                formatter,
                "{} is already queued in {province}",
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

    pub fn recruitment_options(
        &self,
        province_id: &str,
    ) -> Result<Vec<RecruitmentOption>, CampaignError> {
        let province = self.province(province_id)?;
        let faction = self.faction(&self.active_faction)?;

        Ok(UNIT_KINDS
            .iter()
            .copied()
            .map(|unit| {
                let spec = unit.spec();
                let reason = self.recruitment_unavailable_reason(province, unit, faction.treasury);
                RecruitmentOption {
                    unit,
                    label: spec.label.to_owned(),
                    cost: spec.cost,
                    soldiers: spec.soldiers,
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
        if self.pending_battle.is_some() {
            return Err(CampaignError::BattlePending);
        }

        let province = self.province(province_id)?;
        let province_name = province.name.clone();
        let province_owner = province.owner.clone();
        let active_faction = self.active_faction.clone();

        if province_owner != active_faction {
            return Err(CampaignError::ProvinceNotOwned {
                province: province_name,
                owner: province_owner,
                active_faction,
            });
        }

        if self.recruitment_queue.iter().any(|order| {
            order.faction_id == self.active_faction
                && order.province_id == province_id
                && order.unit == unit
        }) {
            return Err(CampaignError::RecruitmentAlreadyQueued {
                province: province_name,
                unit,
            });
        }

        let faction_index = self
            .factions
            .iter()
            .position(|faction| faction.id == self.active_faction)
            .ok_or_else(|| CampaignError::FactionNotFound(self.active_faction.clone()))?;
        let spec = unit.spec();
        let treasury = self.factions[faction_index].treasury;

        if treasury < spec.cost {
            return Err(CampaignError::InsufficientTreasury {
                faction: self.factions[faction_index].name.clone(),
                cost: spec.cost,
                treasury,
            });
        }

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
            .map(|province| province.wealth.saturating_mul(INCOME_PER_WEALTH))
            .sum()
    }

    fn recruitment_unavailable_reason(
        &self,
        province: &Province,
        unit: UnitKind,
        treasury: u32,
    ) -> Option<String> {
        if self.pending_battle.is_some() {
            return Some("Resolve the pending battle before recruiting.".to_owned());
        }
        if province.owner != self.active_faction {
            return Some(format!(
                "Only {} can recruit during this turn, and it does not control {}.",
                self.faction_name(&self.active_faction),
                province.name
            ));
        }
        if self.recruitment_queue.iter().any(|order| {
            order.faction_id == self.active_faction
                && order.province_id == province.id
                && order.unit == unit
        }) {
            return Some(format!(
                "{} is already queued in {}.",
                unit.spec().label,
                province.name
            ));
        }

        let spec = unit.spec();
        if treasury < spec.cost {
            return Some(format!(
                "Need {} gold; the treasury has {}.",
                spec.cost, treasury
            ));
        }
        None
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
    CampaignState {
        year: 1087,
        turn: 1,
        active_faction: "england".into(),
        factions: vec![
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
        ],
        provinces: vec![
            Province {
                id: "wessex".into(),
                name: "Wessex".into(),
                owner: "england".into(),
                wealth: 5,
                neighbors: vec!["normandy".into()],
            },
            Province {
                id: "normandy".into(),
                name: "Normandy".into(),
                owner: "england".into(),
                wealth: 6,
                neighbors: vec!["wessex".into(), "brittany".into(), "paris".into()],
            },
            Province {
                id: "brittany".into(),
                name: "Brittany".into(),
                owner: "france".into(),
                wealth: 4,
                neighbors: vec!["normandy".into(), "anjou".into()],
            },
            Province {
                id: "anjou".into(),
                name: "Anjou".into(),
                owner: "france".into(),
                wealth: 4,
                neighbors: vec!["brittany".into(), "paris".into()],
            },
            Province {
                id: "paris".into(),
                name: "Paris".into(),
                owner: "france".into(),
                wealth: 8,
                neighbors: vec!["normandy".into(), "anjou".into(), "flanders".into()],
            },
            Province {
                id: "flanders".into(),
                name: "Flanders".into(),
                owner: "france".into(),
                wealth: 7,
                neighbors: vec!["paris".into()],
            },
        ],
        armies: vec![
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
        ],
        pending_battle: None,
        recruitment_queue: Vec::new(),
        battle_reports: Vec::new(),
        log: vec!["The campaign begins in 1087.".into()],
    }
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
        assert_eq!(france.treasury, 2_350);
        assert_eq!(france.last_economy_turn, Some(2));

        campaign.apply_turn_start().unwrap();
        assert_eq!(campaign.faction("france").unwrap().treasury, 2_350);
    }

    #[test]
    fn recruitment_deducts_cost_and_rejects_duplicate_queueing() {
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
        assert!(matches!(
            campaign.queue_recruitment("normandy", UnitKind::Levy),
            Err(CampaignError::RecruitmentAlreadyQueued { .. })
        ));
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
            .queue_recruitment("normandy", UnitKind::Spearmen)
            .unwrap();

        campaign.end_turn().unwrap();
        campaign.end_turn().unwrap();

        let army = campaign
            .armies
            .iter()
            .find(|army| army.id == "england-main")
            .unwrap();
        assert_eq!(army.spearmen, 110);
        assert!(campaign.recruitment_queue.is_empty());

        campaign.apply_turn_start().unwrap();
        let army = campaign
            .armies
            .iter()
            .find(|army| army.id == "england-main")
            .unwrap();
        assert_eq!(army.spearmen, 110);
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
            .queue_recruitment("wessex", UnitKind::Archers)
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
