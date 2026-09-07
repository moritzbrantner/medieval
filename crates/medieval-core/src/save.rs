use std::{collections::HashSet, fmt};

use serde::{Deserialize, Serialize};

use crate::CampaignState;

pub const CAMPAIGN_SAVE_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CampaignSave {
    pub schema_version: u32,
    pub player_faction: String,
    pub campaign: CampaignState,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SaveError {
    InvalidJson(String),
    UnsupportedVersion(u32),
    InvalidState(String),
    Serialization(String),
}

impl fmt::Display for SaveError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidJson(message) => write!(formatter, "invalid save document: {message}"),
            Self::UnsupportedVersion(version) => {
                write!(
                    formatter,
                    "unsupported campaign save schema version {version}"
                )
            }
            Self::InvalidState(message) => {
                write!(formatter, "invalid campaign save state: {message}")
            }
            Self::Serialization(message) => {
                write!(formatter, "could not serialize campaign save: {message}")
            }
        }
    }
}

impl std::error::Error for SaveError {}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SaveHeader {
    schema_version: u32,
}

impl CampaignSave {
    pub fn from_campaign(
        campaign: CampaignState,
        player_faction: impl Into<String>,
    ) -> Result<Self, SaveError> {
        let save = Self {
            schema_version: CAMPAIGN_SAVE_SCHEMA_VERSION,
            player_faction: player_faction.into(),
            campaign,
        };
        save.validate()?;
        Ok(save)
    }

    pub fn from_json(json: &str) -> Result<Self, SaveError> {
        let header: SaveHeader = serde_json::from_str(json)
            .map_err(|error| SaveError::InvalidJson(error.to_string()))?;
        if header.schema_version != CAMPAIGN_SAVE_SCHEMA_VERSION {
            return Err(SaveError::UnsupportedVersion(header.schema_version));
        }

        let save: Self = serde_json::from_str(json)
            .map_err(|error| SaveError::InvalidJson(error.to_string()))?;
        save.validate()?;
        Ok(save)
    }

    pub fn to_json(&self) -> Result<String, SaveError> {
        self.validate()?;
        serde_json::to_string_pretty(self)
            .map_err(|error| SaveError::Serialization(error.to_string()))
    }

    pub fn validate(&self) -> Result<(), SaveError> {
        if self.schema_version != CAMPAIGN_SAVE_SCHEMA_VERSION {
            return Err(SaveError::UnsupportedVersion(self.schema_version));
        }

        let campaign = &self.campaign;
        if campaign.turn == 0 {
            return invalid("turn must be at least 1");
        }
        if campaign.factions.is_empty() {
            return invalid("campaign must contain at least one faction");
        }
        if campaign.provinces.is_empty() {
            return invalid("campaign must contain at least one province");
        }

        let faction_ids = unique_ids(
            campaign.factions.iter().map(|faction| faction.id.as_str()),
            "faction",
        )?;
        if !faction_ids.contains(campaign.active_faction.as_str()) {
            return invalid(format!(
                "active faction {} does not exist",
                campaign.active_faction
            ));
        }
        if !faction_ids.contains(self.player_faction.as_str()) {
            return invalid(format!(
                "player faction {} does not exist",
                self.player_faction
            ));
        }
        for faction in &campaign.factions {
            if faction
                .last_economy_turn
                .is_some_and(|processed_turn| processed_turn > campaign.turn)
            {
                return invalid(format!(
                    "faction {} has economy state from future turn {}",
                    faction.id,
                    faction.last_economy_turn.unwrap_or_default()
                ));
            }
        }

        let province_ids = unique_ids(
            campaign
                .provinces
                .iter()
                .map(|province| province.id.as_str()),
            "province",
        )?;
        for province in &campaign.provinces {
            if !faction_ids.contains(province.owner.as_str()) {
                return invalid(format!(
                    "province {} references unknown owner {}",
                    province.id, province.owner
                ));
            }
            for neighbor in &province.neighbors {
                if neighbor == &province.id {
                    return invalid(format!("province {} borders itself", province.id));
                }
                if !province_ids.contains(neighbor.as_str()) {
                    return invalid(format!(
                        "province {} references unknown neighbor {}",
                        province.id, neighbor
                    ));
                }
                let reciprocal = campaign.provinces.iter().any(|candidate| {
                    candidate.id == *neighbor && candidate.neighbors.contains(&province.id)
                });
                if !reciprocal {
                    return invalid(format!(
                        "province border {} -> {} is not reciprocal",
                        province.id, neighbor
                    ));
                }
            }
        }

        let army_ids = unique_ids(campaign.armies.iter().map(|army| army.id.as_str()), "army")?;
        for army in &campaign.armies {
            if !faction_ids.contains(army.owner.as_str()) {
                return invalid(format!(
                    "army {} references unknown owner {}",
                    army.id, army.owner
                ));
            }
            if !province_ids.contains(army.province.as_str()) {
                return invalid(format!(
                    "army {} references unknown province {}",
                    army.id, army.province
                ));
            }
        }

        if let Some(pending) = &campaign.pending_battle {
            if !faction_ids.contains(pending.attacker_faction.as_str())
                || !faction_ids.contains(pending.defender_faction.as_str())
            {
                return invalid("pending battle references an unknown faction");
            }
            if !province_ids.contains(pending.from_province.as_str())
                || !province_ids.contains(pending.target_province.as_str())
            {
                return invalid("pending battle references an unknown province");
            }
            let attacker = campaign
                .armies
                .iter()
                .find(|army| army.id == pending.attacker_army_id)
                .ok_or_else(|| {
                    SaveError::InvalidState(format!(
                        "pending battle references unknown attacker army {}",
                        pending.attacker_army_id
                    ))
                })?;
            if attacker.owner != pending.attacker_faction
                || attacker.province != pending.from_province
            {
                return invalid("pending battle attacker does not match its army state");
            }
            let target_owner = campaign
                .provinces
                .iter()
                .find(|province| province.id == pending.target_province)
                .map(|province| province.owner.as_str());
            if target_owner != Some(pending.defender_faction.as_str()) {
                return invalid("pending battle defender does not control the target province");
            }
        }

        for order in &campaign.recruitment_queue {
            if !faction_ids.contains(order.faction_id.as_str()) {
                return invalid(format!(
                    "recruitment order references unknown faction {}",
                    order.faction_id
                ));
            }
            let province = campaign
                .provinces
                .iter()
                .find(|province| province.id == order.province_id)
                .ok_or_else(|| {
                    SaveError::InvalidState(format!(
                        "recruitment order references unknown province {}",
                        order.province_id
                    ))
                })?;
            if province.owner != order.faction_id {
                return invalid(format!(
                    "recruitment order faction {} does not control {}",
                    order.faction_id, order.province_id
                ));
            }
            if order.ready_on_turn <= campaign.turn {
                return invalid(format!(
                    "recruitment order in {} is already due on turn {}",
                    order.province_id, order.ready_on_turn
                ));
            }
        }

        for report in &campaign.battle_reports {
            if !faction_ids.contains(report.attacker_faction.as_str())
                || !faction_ids.contains(report.defender_faction.as_str())
            {
                return invalid("battle report references an unknown faction");
            }
            if !province_ids.contains(report.from_province.as_str())
                || !province_ids.contains(report.target_province.as_str())
                || report
                    .defender_retreat_province
                    .as_ref()
                    .is_some_and(|province| !province_ids.contains(province.as_str()))
            {
                return invalid("battle report references an unknown province");
            }
        }

        if campaign
            .pending_battle
            .as_ref()
            .is_some_and(|pending| !army_ids.contains(pending.attacker_army_id.as_str()))
        {
            return invalid("pending battle attacker army does not exist");
        }

        Ok(())
    }
}

fn unique_ids<'a>(
    ids: impl IntoIterator<Item = &'a str>,
    kind: &str,
) -> Result<HashSet<&'a str>, SaveError> {
    let mut unique = HashSet::new();
    for id in ids {
        if id.is_empty() {
            return invalid(format!("{kind} id must not be empty"));
        }
        if !unique.insert(id) {
            return invalid(format!("duplicate {kind} id {id}"));
        }
    }
    Ok(unique)
}

fn invalid<T>(message: impl Into<String>) -> Result<T, SaveError> {
    Err(SaveError::InvalidState(message.into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{UnitKind, new_campaign};

    fn representative_save() -> CampaignSave {
        let mut campaign = new_campaign();
        campaign
            .queue_recruitment("normandy", UnitKind::Spearmen)
            .unwrap();
        campaign.move_army("england-main", "paris").unwrap();
        CampaignSave::from_campaign(campaign, "england").unwrap()
    }

    fn as_value(save: &CampaignSave) -> serde_json::Value {
        serde_json::from_str(&save.to_json().unwrap()).unwrap()
    }

    #[test]
    fn round_trip_preserves_the_complete_campaign_exactly() {
        let save = representative_save();
        let loaded = CampaignSave::from_json(&save.to_json().unwrap()).unwrap();

        assert_eq!(loaded, save);
    }

    #[test]
    fn serialization_is_deterministic_for_identical_state() {
        let first = representative_save();
        let second = first.clone();

        assert_eq!(first.to_json().unwrap(), second.to_json().unwrap());
    }

    #[test]
    fn document_exposes_an_explicit_schema_version() {
        let value = as_value(&representative_save());

        assert_eq!(value["schemaVersion"], CAMPAIGN_SAVE_SCHEMA_VERSION);
        assert_eq!(value["playerFaction"], "england");
    }

    #[test]
    fn future_schema_is_rejected_before_future_payload_is_interpreted() {
        let error =
            CampaignSave::from_json(r#"{"schemaVersion":99,"futureShape":true}"#).unwrap_err();

        assert_eq!(error, SaveError::UnsupportedVersion(99));
    }

    #[test]
    fn malformed_json_is_rejected() {
        assert!(matches!(
            CampaignSave::from_json("{broken"),
            Err(SaveError::InvalidJson(_))
        ));
    }

    #[test]
    fn unknown_player_faction_is_rejected() {
        let campaign = new_campaign();

        assert!(matches!(
            CampaignSave::from_campaign(campaign, "vikings"),
            Err(SaveError::InvalidState(_))
        ));
    }

    #[test]
    fn duplicate_faction_ids_are_rejected() {
        let mut save = representative_save();
        save.campaign.factions[1].id = save.campaign.factions[0].id.clone();

        assert!(matches!(save.validate(), Err(SaveError::InvalidState(_))));
    }

    #[test]
    fn unknown_active_faction_is_rejected() {
        let mut save = representative_save();
        save.campaign.active_faction = "missing".into();

        assert!(matches!(save.validate(), Err(SaveError::InvalidState(_))));
    }

    #[test]
    fn corrupt_province_owner_and_neighbor_are_rejected() {
        let mut owner_save = representative_save();
        owner_save.campaign.provinces[0].owner = "missing".into();
        assert!(matches!(
            owner_save.validate(),
            Err(SaveError::InvalidState(_))
        ));

        let mut neighbor_save = representative_save();
        neighbor_save.campaign.provinces[0].neighbors = vec!["missing".into()];
        assert!(matches!(
            neighbor_save.validate(),
            Err(SaveError::InvalidState(_))
        ));
    }

    #[test]
    fn one_way_province_borders_are_rejected() {
        let mut save = representative_save();
        save.campaign.provinces[1]
            .neighbors
            .retain(|neighbor| neighbor != "wessex");

        assert!(matches!(save.validate(), Err(SaveError::InvalidState(_))));
    }

    #[test]
    fn corrupt_army_owner_and_location_are_rejected() {
        let mut owner_save = representative_save();
        owner_save.campaign.armies[0].owner = "missing".into();
        assert!(matches!(
            owner_save.validate(),
            Err(SaveError::InvalidState(_))
        ));

        let mut province_save = representative_save();
        province_save.campaign.armies[0].province = "missing".into();
        assert!(matches!(
            province_save.validate(),
            Err(SaveError::InvalidState(_))
        ));
    }

    #[test]
    fn corrupt_pending_battle_is_rejected() {
        let mut save = representative_save();
        save.campaign
            .pending_battle
            .as_mut()
            .unwrap()
            .attacker_faction = "france".into();

        assert!(matches!(save.validate(), Err(SaveError::InvalidState(_))));
    }

    #[test]
    fn corrupt_recruitment_order_is_rejected() {
        let mut save = representative_save();
        save.campaign.recruitment_queue[0].province_id = "paris".into();

        assert!(matches!(save.validate(), Err(SaveError::InvalidState(_))));
    }

    #[test]
    fn already_due_recruitment_order_is_rejected() {
        let mut save = representative_save();
        save.campaign.recruitment_queue[0].ready_on_turn = save.campaign.turn;

        assert!(matches!(save.validate(), Err(SaveError::InvalidState(_))));
    }
}
