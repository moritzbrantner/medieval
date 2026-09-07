use std::{collections::HashSet, fmt};

use medieval_core::CampaignState;
use serde::{Deserialize, Serialize};

pub const SAVE_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CampaignSave {
    pub schema_version: u32,
    pub player_faction: String,
    pub campaign: CampaignState,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct VersionProbe {
    schema_version: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SaveError {
    InvalidJson(String),
    UnsupportedVersion { found: u32, supported: u32 },
    InvalidState(String),
}

impl fmt::Display for SaveError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidJson(message) => write!(formatter, "invalid campaign save: {message}"),
            Self::UnsupportedVersion { found, supported } => write!(
                formatter,
                "unsupported campaign save version {found}; this build supports version {supported}"
            ),
            Self::InvalidState(message) => write!(formatter, "invalid campaign state: {message}"),
        }
    }
}

impl std::error::Error for SaveError {}

pub fn encode_campaign_save(
    campaign: &CampaignState,
    player_faction: &str,
) -> Result<String, SaveError> {
    let save = CampaignSave {
        schema_version: SAVE_SCHEMA_VERSION,
        player_faction: player_faction.to_owned(),
        campaign: campaign.clone(),
    };
    validate_campaign_save(&save)?;
    serde_json::to_string_pretty(&save).map_err(|error| SaveError::InvalidJson(error.to_string()))
}

pub fn decode_campaign_save(contents: &str) -> Result<CampaignSave, SaveError> {
    let version: VersionProbe = serde_json::from_str(contents)
        .map_err(|error| SaveError::InvalidJson(error.to_string()))?;
    if version.schema_version != SAVE_SCHEMA_VERSION {
        return Err(SaveError::UnsupportedVersion {
            found: version.schema_version,
            supported: SAVE_SCHEMA_VERSION,
        });
    }

    let save: CampaignSave = serde_json::from_str(contents)
        .map_err(|error| SaveError::InvalidJson(error.to_string()))?;
    validate_campaign_save(&save)?;
    Ok(save)
}

fn validate_campaign_save(save: &CampaignSave) -> Result<(), SaveError> {
    if save.schema_version != SAVE_SCHEMA_VERSION {
        return Err(SaveError::UnsupportedVersion {
            found: save.schema_version,
            supported: SAVE_SCHEMA_VERSION,
        });
    }

    let faction_ids = unique_ids(
        save.campaign.factions.iter().map(|faction| faction.id.as_str()),
        "faction",
    )?;
    if faction_ids.is_empty() {
        return Err(SaveError::InvalidState(
            "campaign contains no factions".to_owned(),
        ));
    }
    if !faction_ids.contains(save.player_faction.as_str()) {
        return Err(SaveError::InvalidState(format!(
            "player faction {} does not exist",
            save.player_faction
        )));
    }
    if !faction_ids.contains(save.campaign.active_faction.as_str()) {
        return Err(SaveError::InvalidState(format!(
            "active faction {} does not exist",
            save.campaign.active_faction
        )));
    }

    let province_ids = unique_ids(
        save.campaign
            .provinces
            .iter()
            .map(|province| province.id.as_str()),
        "province",
    )?;
    for province in &save.campaign.provinces {
        if !faction_ids.contains(province.owner.as_str()) {
            return Err(SaveError::InvalidState(format!(
                "province {} references unknown owner {}",
                province.id, province.owner
            )));
        }
        for neighbor in &province.neighbors {
            if !province_ids.contains(neighbor.as_str()) {
                return Err(SaveError::InvalidState(format!(
                    "province {} references unknown neighbor {}",
                    province.id, neighbor
                )));
            }
        }
    }

    let army_ids = unique_ids(
        save.campaign.armies.iter().map(|army| army.id.as_str()),
        "army",
    )?;
    for army in &save.campaign.armies {
        if !faction_ids.contains(army.owner.as_str()) {
            return Err(SaveError::InvalidState(format!(
                "army {} references unknown owner {}",
                army.id, army.owner
            )));
        }
        if !province_ids.contains(army.province.as_str()) {
            return Err(SaveError::InvalidState(format!(
                "army {} references unknown province {}",
                army.id, army.province
            )));
        }
    }

    for order in &save.campaign.recruitment_queue {
        if !faction_ids.contains(order.faction_id.as_str()) {
            return Err(SaveError::InvalidState(format!(
                "recruitment order references unknown faction {}",
                order.faction_id
            )));
        }
        if !province_ids.contains(order.province_id.as_str()) {
            return Err(SaveError::InvalidState(format!(
                "recruitment order references unknown province {}",
                order.province_id
            )));
        }
    }

    if let Some(battle) = &save.campaign.pending_battle {
        if !army_ids.contains(battle.attacker_army_id.as_str()) {
            return Err(SaveError::InvalidState(format!(
                "pending battle references unknown attacker army {}",
                battle.attacker_army_id
            )));
        }
        if !faction_ids.contains(battle.attacker_faction.as_str())
            || !faction_ids.contains(battle.defender_faction.as_str())
        {
            return Err(SaveError::InvalidState(
                "pending battle references an unknown faction".to_owned(),
            ));
        }
        if !province_ids.contains(battle.from_province.as_str())
            || !province_ids.contains(battle.target_province.as_str())
        {
            return Err(SaveError::InvalidState(
                "pending battle references an unknown province".to_owned(),
            ));
        }

        let attacker = save
            .campaign
            .armies
            .iter()
            .find(|army| army.id == battle.attacker_army_id)
            .expect("validated attacker army id must exist");
        if attacker.owner != battle.attacker_faction {
            return Err(SaveError::InvalidState(format!(
                "pending battle attacker {} is owned by {}, not {}",
                attacker.id, attacker.owner, battle.attacker_faction
            )));
        }
    }

    Ok(())
}

fn unique_ids<'a>(
    ids: impl Iterator<Item = &'a str>,
    kind: &str,
) -> Result<HashSet<&'a str>, SaveError> {
    let mut unique = HashSet::new();
    for id in ids {
        if id.is_empty() {
            return Err(SaveError::InvalidState(format!(
                "{kind} id cannot be empty"
            )));
        }
        if !unique.insert(id) {
            return Err(SaveError::InvalidState(format!(
                "duplicate {kind} id {id}"
            )));
        }
    }
    Ok(unique)
}

#[cfg(test)]
mod tests {
    use medieval_core::new_campaign;

    use super::*;

    #[test]
    fn versioned_save_round_trip_preserves_campaign_and_player() {
        let mut campaign = new_campaign();
        campaign.move_army("england-main", "wessex").unwrap();

        let encoded = encode_campaign_save(&campaign, "england").unwrap();
        let decoded = decode_campaign_save(&encoded).unwrap();

        assert_eq!(decoded.schema_version, SAVE_SCHEMA_VERSION);
        assert_eq!(decoded.player_faction, "england");
        assert_eq!(decoded.campaign, campaign);
    }

    #[test]
    fn unsupported_version_fails_before_full_schema_decode() {
        let encoded = encode_campaign_save(&new_campaign(), "england").unwrap();
        let future = encoded
            .replace("\"schemaVersion\": 1", "\"schemaVersion\": 2")
            .replace("\"playerFaction\"", "\"futureField\": true,\n  \"playerFaction\"");

        assert_eq!(
            decode_campaign_save(&future),
            Err(SaveError::UnsupportedVersion {
                found: 2,
                supported: SAVE_SCHEMA_VERSION,
            })
        );
    }

    #[test]
    fn unknown_player_faction_fails_closed() {
        assert!(matches!(
            encode_campaign_save(&new_campaign(), "vikings"),
            Err(SaveError::InvalidState(message)) if message.contains("player faction vikings")
        ));
    }

    #[test]
    fn dangling_campaign_references_fail_closed() {
        let mut campaign = new_campaign();
        campaign.provinces[0].owner = "missing-faction".to_owned();

        assert!(matches!(
            encode_campaign_save(&campaign, "england"),
            Err(SaveError::InvalidState(message)) if message.contains("unknown owner")
        ));
    }

    #[test]
    fn malformed_json_is_rejected() {
        assert!(matches!(
            decode_campaign_save("{not json}"),
            Err(SaveError::InvalidJson(_))
        ));
    }
}
