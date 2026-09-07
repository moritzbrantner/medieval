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
}

impl CampaignState {
    pub fn end_turn(&mut self) {
        if self.factions.is_empty() {
            return;
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
        self.log.push(format!(
            "Turn {}: {} begins its turn.",
            self.turn, self.factions[next_index].name
        ));
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
            },
            Army {
                id: "france-main".into(),
                owner: "france".into(),
                province: "paris".into(),
                levy: 120,
                spearmen: 80,
                archers: 40,
                knights: 20,
            },
        ],
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

        campaign.end_turn();
        assert_eq!(campaign.year, 1087);
        assert_eq!(campaign.active_faction, "france");

        campaign.end_turn();
        assert_eq!(campaign.year, 1088);
        assert_eq!(campaign.active_faction, "england");
        assert_eq!(campaign.turn, 3);
    }
}
