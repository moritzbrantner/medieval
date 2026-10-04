use std::{collections::BTreeSet, fmt};

use serde::{Deserialize, Serialize};

use crate::{
    BattleSide, TacticalArmySeed, TacticalBattle, TacticalBattleSeed, TacticalBattleState,
    TacticalFinishReason, TacticalForceSeed, UnitKind,
};

pub const TACTICAL_BATTLE_RESULT_SCHEMA_VERSION: u32 = 1;

/// The reverse campaign handoff. Counts are exact source-army totals, never
/// inferred from renderer instances or proportionally assigned after combat.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TacticalBattleResult {
    pub schema_version: u32,
    pub seed: TacticalBattleSeed,
    pub winner: Option<BattleSide>,
    pub reason: TacticalFinishReason,
    pub finishing_tick: u64,
    pub armies: Vec<TacticalArmyResult>,
    pub settlement_capture: Option<TacticalSettlementCapture>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TacticalArmyResult {
    pub source_army_id: String,
    pub faction_id: String,
    pub side: BattleSide,
    pub units: Vec<TacticalUnitResult>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TacticalUnitResult {
    pub kind: UnitKind,
    pub initial_soldiers: u64,
    pub surviving_soldiers: u64,
    pub casualties: u64,
    /// Routed and escaped soldiers are subsets of survivors and may overlap.
    pub routed_soldiers: u64,
    pub escaped_soldiers: u64,
    /// Pursuit casualties are a subset of total casualties.
    pub pursuit_casualties: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TacticalSettlementCapture {
    pub province_id: String,
    pub faction_id: String,
    pub side: BattleSide,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TacticalResultError {
    BattleNotFinished,
    MissingCampaignSeed,
    MissingArmyProvenance,
    UnsupportedVersion(u32),
    InvalidDocument(String),
    InvalidProvenance(String),
    ConservationViolation(String),
}

impl fmt::Display for TacticalResultError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BattleNotFinished => write!(formatter, "the tactical battle is still running"),
            Self::MissingCampaignSeed => write!(formatter, "the battle has no campaign seed"),
            Self::MissingArmyProvenance => {
                write!(formatter, "source army provenance is incomplete")
            }
            Self::UnsupportedVersion(version) => {
                write!(formatter, "unsupported tactical result version {version}")
            }
            Self::InvalidDocument(message) => {
                write!(formatter, "invalid tactical result document: {message}")
            }
            Self::InvalidProvenance(message) => {
                write!(formatter, "invalid tactical result provenance: {message}")
            }
            Self::ConservationViolation(message) => write!(
                formatter,
                "tactical result violates soldier conservation: {message}"
            ),
        }
    }
}

impl std::error::Error for TacticalResultError {}

impl TacticalBattle {
    pub fn campaign_result(&self) -> Result<TacticalBattleResult, TacticalResultError> {
        let TacticalBattleState::Finished {
            winner,
            reason,
            finishing_tick,
        } = self.state()
        else {
            return Err(TacticalResultError::BattleNotFinished);
        };
        let seed = self
            .campaign_seed()
            .ok_or(TacticalResultError::MissingCampaignSeed)?
            .clone();
        let mut armies = result_rosters(&seed)?;
        let mut deployed_initial = armies.clone();
        for army in &mut deployed_initial {
            for unit in &mut army.units {
                unit.initial_soldiers = 0;
            }
        }
        let mut unit_ids = BTreeSet::new();
        for unit in self.units() {
            if !unit_ids.insert(unit.id()) {
                return Err(TacticalResultError::ConservationViolation(
                    unit.id().to_owned(),
                ));
            }
            let provenance = unit
                .campaign_provenance()
                .ok_or(TacticalResultError::MissingArmyProvenance)?;
            let army_index = armies
                .iter()
                .position(|army| {
                    army.side == unit.side() && army.source_army_id == provenance.source_army_id
                })
                .ok_or_else(|| TacticalResultError::InvalidProvenance(unit.id().to_owned()))?;
            let kind_index = armies[army_index]
                .units
                .iter()
                .position(|entry| Some(entry.kind) == unit.unit_kind())
                .ok_or_else(|| TacticalResultError::InvalidProvenance(unit.id().to_owned()))?;
            let initial = u64::from(provenance.initial_soldiers);
            let survivors = u64::from(unit.soldiers());
            let casualties = initial
                .checked_sub(survivors)
                .ok_or_else(|| TacticalResultError::ConservationViolation(unit.id().to_owned()))?;
            if initial == 0
                || unit.is_destroyed() != (survivors == 0)
                || u64::from(unit.pursuit_casualties()) > casualties
            {
                return Err(TacticalResultError::ConservationViolation(
                    unit.id().to_owned(),
                ));
            }
            let entry = &mut armies[army_index].units[kind_index];
            checked_add(&mut entry.surviving_soldiers, survivors)?;
            checked_add(&mut entry.casualties, casualties)?;
            if unit.is_routed() {
                checked_add(&mut entry.routed_soldiers, survivors)?;
            }
            if unit.is_escaped() {
                checked_add(&mut entry.escaped_soldiers, survivors)?;
            }
            checked_add(
                &mut entry.pursuit_casualties,
                u64::from(unit.pursuit_casualties()),
            )?;
            checked_add(
                &mut deployed_initial[army_index].units[kind_index].initial_soldiers,
                initial,
            )?;
        }
        for (army, deployed) in armies.iter().zip(deployed_initial) {
            for (unit, initial) in army.units.iter().zip(deployed.units) {
                if unit.initial_soldiers != initial.initial_soldiers {
                    return Err(TacticalResultError::ConservationViolation(
                        army.source_army_id.clone(),
                    ));
                }
            }
        }
        let settlement_capture = capture_outcome(&seed, winner, reason)?;
        if settlement_capture.is_some()
            && self
                .siege_snapshot()
                .and_then(|siege| siege.capture.captured_by)
                != winner
        {
            return Err(TacticalResultError::InvalidProvenance(
                "siege capture does not match the terminal state".into(),
            ));
        }
        let result = TacticalBattleResult {
            schema_version: TACTICAL_BATTLE_RESULT_SCHEMA_VERSION,
            seed,
            winner,
            reason,
            finishing_tick,
            armies,
            settlement_capture,
        };
        result.validate()?;
        Ok(result)
    }
}

impl TacticalBattleResult {
    pub fn validate(&self) -> Result<(), TacticalResultError> {
        if self.schema_version != TACTICAL_BATTLE_RESULT_SCHEMA_VERSION {
            return Err(TacticalResultError::UnsupportedVersion(self.schema_version));
        }
        let draw_reason = matches!(
            self.reason,
            TacticalFinishReason::MutualDefeat | TacticalFinishReason::MutualWithdrawal
        );
        if draw_reason != self.winner.is_none() {
            return Err(TacticalResultError::InvalidProvenance(
                "winner does not match the terminal reason".into(),
            ));
        }
        let expected = result_rosters(&self.seed)?;
        if self.armies.len() != expected.len() {
            return Err(TacticalResultError::InvalidProvenance(
                "source armies differ from seed".into(),
            ));
        }
        for (army, expected) in self.armies.iter().zip(expected) {
            if army.source_army_id != expected.source_army_id
                || army.side != expected.side
                || army.faction_id != expected.faction_id
                || army.units.len() != expected.units.len()
            {
                return Err(TacticalResultError::InvalidProvenance(
                    army.source_army_id.clone(),
                ));
            }
            for (unit, expected) in army.units.iter().zip(expected.units) {
                if unit.kind != expected.kind || unit.initial_soldiers != expected.initial_soldiers
                {
                    return Err(TacticalResultError::InvalidProvenance(
                        army.source_army_id.clone(),
                    ));
                }
                if unit.surviving_soldiers.checked_add(unit.casualties)
                    != Some(unit.initial_soldiers)
                    || unit.routed_soldiers > unit.surviving_soldiers
                    || unit.escaped_soldiers > unit.surviving_soldiers
                    || unit.pursuit_casualties > unit.casualties
                {
                    return Err(TacticalResultError::ConservationViolation(
                        army.source_army_id.clone(),
                    ));
                }
            }
        }
        self.validate_terminal_outcome()?;
        if self.settlement_capture != capture_outcome(&self.seed, self.winner, self.reason)? {
            return Err(TacticalResultError::InvalidProvenance(
                "settlement capture differs from terminal outcome".into(),
            ));
        }
        Ok(())
    }

    fn validate_terminal_outcome(&self) -> Result<(), TacticalResultError> {
        let units = |side| {
            self.armies
                .iter()
                .filter(move |army| army.side == side)
                .flat_map(|army| &army.units)
        };
        // A side that withdrew has no formed units. Before withdrawal, absence
        // of escape plus non-routed survivors identifies the formed force.
        let formed = |side| {
            units(side).all(|unit| unit.escaped_soldiers == 0)
                && units(side).any(|unit| unit.surviving_soldiers > unit.routed_soldiers)
        };
        let defeated = |side| {
            units(side).all(|unit| {
                unit.escaped_soldiers == 0 && unit.surviving_soldiers == unit.routed_soldiers
            })
        };
        let withdrawn = |side| {
            units(side).any(|unit| unit.initial_soldiers > 0)
                && units(side).all(|unit| unit.surviving_soldiers == unit.escaped_soldiers)
        };
        let opposite = |side| match side {
            BattleSide::Attacker => BattleSide::Defender,
            BattleSide::Defender => BattleSide::Attacker,
        };
        let consistent = match (self.reason, self.winner) {
            (TacticalFinishReason::ForceDefeated, Some(winner)) => {
                formed(winner) && defeated(opposite(winner))
            }
            (TacticalFinishReason::SiegeCapture, Some(winner)) => formed(winner),
            (TacticalFinishReason::Withdrawal, Some(winner)) => {
                formed(winner) && withdrawn(opposite(winner))
            }
            (TacticalFinishReason::MutualDefeat, None) => {
                defeated(BattleSide::Attacker) && defeated(BattleSide::Defender)
            }
            (TacticalFinishReason::MutualWithdrawal, None) => {
                withdrawn(BattleSide::Attacker) && withdrawn(BattleSide::Defender)
            }
            _ => false,
        };
        if !consistent {
            return Err(TacticalResultError::InvalidProvenance(
                "terminal reason and winner contradict army states".into(),
            ));
        }
        Ok(())
    }

    pub fn from_json(json: &str) -> Result<Self, TacticalResultError> {
        let result: Self = serde_json::from_str(json)
            .map_err(|error| TacticalResultError::InvalidDocument(error.to_string()))?;
        result.validate()?;
        Ok(result)
    }

    pub fn to_json(&self) -> Result<String, TacticalResultError> {
        self.validate()?;
        serde_json::to_string_pretty(self)
            .map_err(|error| TacticalResultError::InvalidDocument(error.to_string()))
    }
}

fn result_rosters(
    seed: &TacticalBattleSeed,
) -> Result<Vec<TacticalArmyResult>, TacticalResultError> {
    if seed.turn == 0
        || seed.attacker.source_army_ids != [seed.attacker_army_id.clone()]
        || seed.attacker.faction_id.is_empty()
        || seed.defender.faction_id.is_empty()
        || seed.attacker.faction_id == seed.defender.faction_id
        || seed.from_province.is_empty()
        || seed.target_province.is_empty()
        || seed.from_province == seed.target_province
    {
        return Err(TacticalResultError::InvalidProvenance(
            "seed does not describe a campaign conflict".into(),
        ));
    }
    let mut armies = Vec::new();
    for (side, force) in [
        (BattleSide::Attacker, &seed.attacker),
        (BattleSide::Defender, &seed.defender),
    ] {
        let mut canonical = force.clone();
        crate::campaign_handoff::canonicalize_force(&mut canonical, side)
            .map_err(|error| TacticalResultError::InvalidProvenance(error.to_string()))?;
        if &canonical != force {
            return Err(TacticalResultError::InvalidProvenance(
                "seed is not canonical".into(),
            ));
        }
        let sources = source_rosters(force)?;
        for source in sources {
            if armies
                .iter()
                .any(|army: &TacticalArmyResult| army.source_army_id == source.army_id)
            {
                return Err(TacticalResultError::InvalidProvenance(source.army_id));
            }
            armies.push(TacticalArmyResult {
                source_army_id: source.army_id,
                faction_id: force.faction_id.clone(),
                side,
                units: source
                    .units
                    .into_iter()
                    .map(|unit| TacticalUnitResult {
                        kind: unit.kind,
                        initial_soldiers: unit.soldiers,
                        surviving_soldiers: 0,
                        casualties: 0,
                        routed_soldiers: 0,
                        escaped_soldiers: 0,
                        pursuit_casualties: 0,
                    })
                    .collect(),
            });
        }
    }
    Ok(armies)
}

fn source_rosters(force: &TacticalForceSeed) -> Result<Vec<TacticalArmySeed>, TacticalResultError> {
    match &force.source_armies {
        Some(armies) => Ok(armies.clone()),
        None if force.source_army_ids.len() == 1 => Ok(vec![TacticalArmySeed {
            army_id: force.source_army_ids[0].clone(),
            units: force.units.clone(),
        }]),
        None if force.source_army_ids.is_empty() && force.units.is_empty() => Ok(Vec::new()),
        None => Err(TacticalResultError::MissingArmyProvenance),
    }
}

fn capture_outcome(
    seed: &TacticalBattleSeed,
    winner: Option<BattleSide>,
    reason: TacticalFinishReason,
) -> Result<Option<TacticalSettlementCapture>, TacticalResultError> {
    if reason != TacticalFinishReason::SiegeCapture {
        return Ok(None);
    }
    let side = winner.ok_or_else(|| {
        TacticalResultError::InvalidProvenance("siege capture has no winner".into())
    })?;
    if !matches!(
        seed.battlefield_profile,
        crate::TacticalBattlefieldProfile::Siege { .. }
    ) {
        return Err(TacticalResultError::InvalidProvenance(
            "field battle cannot capture a settlement".into(),
        ));
    }
    let faction_id = match side {
        BattleSide::Attacker => &seed.attacker.faction_id,
        BattleSide::Defender => &seed.defender.faction_id,
    };
    Ok(Some(TacticalSettlementCapture {
        province_id: seed.target_province.clone(),
        faction_id: faction_id.clone(),
        side,
    }))
}

fn checked_add(count: &mut u64, value: u64) -> Result<(), TacticalResultError> {
    *count = count
        .checked_add(value)
        .ok_or_else(|| TacticalResultError::ConservationViolation("count overflow".into()))?;
    Ok(())
}
