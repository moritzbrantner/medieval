use std::fmt;

use serde::{Deserialize, Serialize};

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
    pub log: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Faction {
    pub id: String,
    pub name: String,
    pub treasury: u32,
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CampaignError {
    ArmyNotFound(String),
    ProvinceNotFound(String),
    NotActiveFaction {
        army_owner: String,
        active_faction: String,
    },
    ArmyAlreadyMoved(String),
    BattlePending,
    DestinationNotAdjacent {
        from: String,
        destination: String,
    },
}

impl fmt::Display for CampaignError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ArmyNotFound(army_id) => write!(formatter, "army {army_id} does not exist"),
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
            Self::DestinationNotAdjacent { from, destination } => {
                write!(formatter, "{destination} is not adjacent to {from}")
            }
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
        Ok(())
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
            },
            Faction {
                id: "france".into(),
                name: "Kingdom of France".into(),
                treasury: 1_200,
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
}
