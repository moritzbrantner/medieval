use crate::{Army, Faction, Province, ProvinceBattlefieldContext};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
    sync::OnceLock,
};

/// Version 2 adds the required `settlement.level`. Version 1 documents remain
/// supported; they cannot declare a level and every settlement is a village.
pub const PROVINCE_DEFINITION_SCHEMA_VERSION: u32 = 2;
const DEFAULT_DEFINITIONS: &str = include_str!("../data/provinces-v2.json");

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BattlefieldDefinition {
    pub id: String,
    #[serde(deserialize_with = "deserialize_battlefield_context")]
    pub context: ProvinceBattlefieldContext,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SettlementDefinition {
    pub fortified: bool,
    /// Required from schema version 2; absent (village) in version 1.
    #[serde(default)]
    pub level: crate::SettlementLevel,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub extensions: BTreeMap<String, serde_json::Value>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProvinceEconomyDefinition {
    pub wealth: u32,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub extensions: BTreeMap<String, serde_json::Value>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProvinceDefinition {
    pub id: String,
    pub name: String,
    pub starting_owner: String,
    pub neighbors: Vec<String>,
    pub battlefield_profile: String,
    pub settlement: SettlementDefinition,
    pub base_economy: ProvinceEconomyDefinition,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub extensions: BTreeMap<String, serde_json::Value>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProvinceDefinitionDocument {
    pub schema_version: u32,
    pub battlefield_profiles: Vec<BattlefieldDefinition>,
    pub provinces: Vec<ProvinceDefinition>,
}

/// Validated configuration; callers cannot mutate it after validation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProvinceDefinitions {
    document: ProvinceDefinitionDocument,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProvinceDefinitionError {
    InvalidJson(String),
    UnsupportedVersion(u32),
    InvalidDefinition(String),
}
impl fmt::Display for ProvinceDefinitionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidJson(message) => write!(f, "invalid province definition JSON: {message}"),
            Self::UnsupportedVersion(version) => write!(
                f,
                "unsupported province definition schema version {version}"
            ),
            Self::InvalidDefinition(message) => write!(f, "invalid province definition: {message}"),
        }
    }
}
impl std::error::Error for ProvinceDefinitionError {}

impl ProvinceDefinitions {
    pub fn from_json(json: &str) -> Result<Self, ProvinceDefinitionError> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Header {
            schema_version: u32,
        }
        let header: Header = serde_json::from_str(json)
            .map_err(|error| ProvinceDefinitionError::InvalidJson(error.to_string()))?;
        if !(1..=PROVINCE_DEFINITION_SCHEMA_VERSION).contains(&header.schema_version) {
            return Err(ProvinceDefinitionError::UnsupportedVersion(
                header.schema_version,
            ));
        }
        let raw: serde_json::Value = serde_json::from_str(json)
            .map_err(|error| ProvinceDefinitionError::InvalidJson(error.to_string()))?;
        let document = serde_json::from_value(raw.clone())
            .map_err(|error| ProvinceDefinitionError::InvalidJson(error.to_string()))?;
        if let Some(provinces) = raw.get("provinces").and_then(serde_json::Value::as_array) {
            for province in provinces {
                let declared = province
                    .get("settlement")
                    .is_some_and(|settlement| settlement.get("level").is_some());
                if declared != (header.schema_version >= 2) {
                    return Err(ProvinceDefinitionError::InvalidJson(format!(
                        "schema version {} {} settlement.level",
                        header.schema_version,
                        if declared {
                            "cannot declare"
                        } else {
                            "requires"
                        }
                    )));
                }
            }
        }
        Self::validate(document)
    }
    pub fn validate(document: ProvinceDefinitionDocument) -> Result<Self, ProvinceDefinitionError> {
        if !(1..=PROVINCE_DEFINITION_SCHEMA_VERSION).contains(&document.schema_version) {
            return Err(ProvinceDefinitionError::UnsupportedVersion(
                document.schema_version,
            ));
        }
        if document.schema_version == 1
            && document
                .provinces
                .iter()
                .any(|province| province.settlement.level != crate::SettlementLevel::Village)
        {
            return invalid("schema version 1 settlements are villages");
        }
        if document.provinces.is_empty() {
            return invalid("province list is empty");
        }
        let mut profiles = BTreeMap::new();
        for profile in &document.battlefield_profiles {
            validate_id(&profile.id)?;
            if profiles
                .insert(profile.id.as_str(), profile.context)
                .is_some()
            {
                return invalid(format!("duplicate battlefield profile {}", profile.id));
            }
        }
        let mut provinces = BTreeMap::new();
        let mut total_wealth = 0_u64;
        for province in &document.provinces {
            validate_id(&province.id)?;
            validate_id(&province.starting_owner)?;
            if province.name.trim().is_empty() {
                return invalid(format!(
                    "province {} has an empty display name",
                    province.id
                ));
            }
            if provinces.insert(province.id.as_str(), province).is_some() {
                return invalid(format!("duplicate province ID {}", province.id));
            }
            let context = profiles
                .get(province.battlefield_profile.as_str())
                .ok_or_else(|| {
                    ProvinceDefinitionError::InvalidDefinition(format!(
                        "province {} references unknown battlefield profile {}",
                        province.id, province.battlefield_profile
                    ))
                })?;
            if context.fortified != province.settlement.fortified {
                return invalid(format!(
                    "province {} settlement disagrees with battlefield fortification",
                    province.id
                ));
            }
            total_wealth = total_wealth
                .checked_add(u64::from(province.base_economy.wealth))
                .ok_or_else(|| {
                    ProvinceDefinitionError::InvalidDefinition("base economy overflows".into())
                })?;
            for extensions in [
                &province.extensions,
                &province.settlement.extensions,
                &province.base_economy.extensions,
            ] {
                for key in extensions.keys() {
                    validate_id(key)?;
                }
            }
        }
        if total_wealth > u64::from(u32::MAX / crate::INCOME_PER_WEALTH) {
            return invalid("combined base economy exceeds campaign income range");
        }
        for province in &document.provinces {
            let mut neighbors = BTreeSet::new();
            for id in &province.neighbors {
                if !neighbors.insert(id) {
                    return invalid(format!("province {} repeats neighbor {id}", province.id));
                }
                if id == &province.id {
                    return invalid(format!("province {id} borders itself"));
                }
                let neighbor = provinces.get(id.as_str()).ok_or_else(|| {
                    ProvinceDefinitionError::InvalidDefinition(format!(
                        "province {} references unknown neighbor {id}",
                        province.id
                    ))
                })?;
                if !neighbor.neighbors.contains(&province.id) {
                    return invalid(format!(
                        "province border {} -> {id} is not reciprocal",
                        province.id
                    ));
                }
            }
        }
        let mut reachable = BTreeSet::new();
        let mut frontier = vec![document.provinces[0].id.as_str()];
        while let Some(id) = frontier.pop() {
            if reachable.insert(id) {
                frontier.extend(provinces[id].neighbors.iter().map(String::as_str));
            }
        }
        if reachable.len() != provinces.len() {
            return invalid("province adjacency graph is disconnected");
        }
        // Version 1 documents migrate to the current shape (explicit village
        // levels), so the retained document always round-trips.
        let mut document = document;
        document.schema_version = PROVINCE_DEFINITION_SCHEMA_VERSION;
        Ok(Self { document })
    }
    #[must_use]
    pub fn document(&self) -> &ProvinceDefinitionDocument {
        &self.document
    }

    pub fn build_provinces(
        &self,
        factions: &[Faction],
        armies: &[Army],
    ) -> Result<Vec<Province>, ProvinceDefinitionError> {
        let faction_ids = factions
            .iter()
            .map(|faction| faction.id.as_str())
            .collect::<BTreeSet<_>>();
        if faction_ids.len() != factions.len() {
            return invalid("starting factions contain duplicate IDs");
        }
        let mut provinces = Vec::with_capacity(self.document.provinces.len());
        for definition in &self.document.provinces {
            if !faction_ids.contains(definition.starting_owner.as_str()) {
                return invalid(format!(
                    "province {} references unknown starting owner {}",
                    definition.id, definition.starting_owner
                ));
            }
            let context = self
                .document
                .battlefield_profiles
                .iter()
                .find(|profile| profile.id == definition.battlefield_profile)
                .expect("battlefield references were validated")
                .context;
            provinces.push(Province {
                id: definition.id.clone(),
                name: definition.name.clone(),
                owner: definition.starting_owner.clone(),
                wealth: definition.base_economy.wealth,
                neighbors: definition.neighbors.clone(),
                battlefield: context,
                settlement_level: definition.settlement.level,
            });
        }
        for army in armies {
            let province = provinces
                .iter()
                .find(|province| province.id == army.province)
                .ok_or_else(|| {
                    ProvinceDefinitionError::InvalidDefinition(format!(
                        "starting army {} references unknown province {}",
                        army.id, army.province
                    ))
                })?;
            if !faction_ids.contains(army.owner.as_str()) || army.owner != province.owner {
                return invalid(format!(
                    "starting army {} ownership disagrees with province {}",
                    army.id, province.id
                ));
            }
        }
        Ok(provinces)
    }
}

#[must_use]
pub fn default_province_definitions() -> &'static ProvinceDefinitions {
    static DEFINITIONS: OnceLock<ProvinceDefinitions> = OnceLock::new();
    DEFINITIONS.get_or_init(|| {
        ProvinceDefinitions::from_json(DEFAULT_DEFINITIONS)
            .expect("packaged province definitions must be valid")
    })
}
fn validate_id(id: &str) -> Result<(), ProvinceDefinitionError> {
    if id.is_empty()
        || !id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"-_:./".contains(&byte))
    {
        return invalid(format!("{id:?} is not a stable machine ID"));
    }
    Ok(())
}
fn invalid<T>(message: impl Into<String>) -> Result<T, ProvinceDefinitionError> {
    Err(ProvinceDefinitionError::InvalidDefinition(message.into()))
}

fn deserialize_battlefield_context<'de, D>(
    deserializer: D,
) -> Result<ProvinceBattlefieldContext, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase", deny_unknown_fields)]
    struct Context {
        location: crate::BattlefieldLocation,
        fortified: bool,
    }
    let context = Context::deserialize(deserializer)?;
    Ok(ProvinceBattlefieldContext {
        location: context.location,
        fortified: context.fortified,
    })
}
