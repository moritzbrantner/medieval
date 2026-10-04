use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::{
    BattleSide, CampaignError, CampaignState, TacticalArmyResult, TacticalBattleResult,
    TacticalUnitSeed,
};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TacticalCampaignReport {
    pub result: TacticalBattleResult,
    pub captured_province: Option<String>,
    pub retreats: Vec<TacticalCampaignRetreat>,
    pub surrenders: Vec<TacticalCampaignSurrender>,
    pub campaign_winner: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TacticalCampaignRetreat {
    pub source_army_id: String,
    pub from_province: String,
    pub to_province: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TacticalCampaignSurrender {
    pub source_army_id: String,
    pub units: Vec<TacticalUnitSeed>,
}

impl CampaignState {
    pub fn finish_reconciled_tactical_battle(
        &mut self,
    ) -> Result<TacticalCampaignReport, CampaignError> {
        let result = self
            .pending_tactical_result
            .clone()
            .ok_or(CampaignError::NoPendingBattle)?;
        self.apply_tactical_battle_result(&result)
    }

    /// Atomically applies casualties and the strategic outcome. Replaying an
    /// already recorded result returns its report without changing the campaign.
    pub fn apply_tactical_battle_result(
        &mut self,
        result: &TacticalBattleResult,
    ) -> Result<TacticalCampaignReport, CampaignError> {
        result
            .validate()
            .map_err(|error| CampaignError::InvalidTacticalResult(error.to_string()))?;
        if let Some(report) = self
            .tactical_battle_reports
            .iter()
            .find(|report| same_battle(&report.result, result))
        {
            report.validate_for_campaign(self)?;
            if &report.result != result {
                return Err(CampaignError::TacticalResultMismatch);
            }
            return Ok(report.clone());
        }
        let mut next = self.clone();
        next.reconcile_tactical_casualties(result)?;
        let captured_province = (result.winner == Some(BattleSide::Attacker))
            .then(|| result.seed.target_province.clone());
        let mut retreats = Vec::new();
        let mut surrenders = Vec::new();
        for source in &result.armies {
            if !source.units.iter().any(|unit| unit.surviving_soldiers > 0) {
                continue;
            }
            if needs_retreat(result, source.side) {
                let preferred = (source.side == BattleSide::Attacker)
                    .then_some(result.seed.from_province.as_str());
                if let Some(destination) = next.tactical_retreat_destination(
                    &result.seed.target_province,
                    &source.faction_id,
                    preferred,
                ) {
                    let army = next
                        .armies
                        .iter_mut()
                        .find(|army| army.id == source.source_army_id)
                        .ok_or(CampaignError::TacticalResultMismatch)?;
                    army.province = destination.clone();
                    army.moved_this_turn = true;
                    retreats.push(TacticalCampaignRetreat {
                        source_army_id: source.source_army_id.clone(),
                        from_province: result.seed.target_province.clone(),
                        to_province: destination,
                    });
                } else {
                    next.armies.retain(|army| army.id != source.source_army_id);
                    surrenders.push(TacticalCampaignSurrender {
                        source_army_id: source.source_army_id.clone(),
                        units: surviving_roster(source),
                    });
                }
            } else if let Some(province) = &captured_province {
                let army = next
                    .armies
                    .iter_mut()
                    .find(|army| army.id == source.source_army_id)
                    .ok_or(CampaignError::TacticalResultMismatch)?;
                army.province = province.clone();
                army.moved_this_turn = true;
            }
        }
        if let Some(province_id) = &captured_province {
            let province = next
                .provinces
                .iter_mut()
                .find(|province| &province.id == province_id)
                .ok_or(CampaignError::TacticalResultMismatch)?;
            province.owner = result.seed.attacker.faction_id.clone();
            let queued = next.recruitment_queue.len();
            next.recruitment_queue.retain(|order| {
                !(order.province_id == *province_id
                    && order.faction_id == result.seed.defender.faction_id)
            });
            let cancelled = queued - next.recruitment_queue.len();
            if cancelled > 0 {
                next.log.push(format!("Turn {}: {cancelled} queued recruitment order(s) in {province_id} are cancelled after capture.", next.turn));
            }
        }
        next.pending_battle = None;
        next.pending_tactical_result = None;
        let report = TacticalCampaignReport {
            result: result.clone(),
            captured_province,
            retreats,
            surrenders,
            campaign_winner: next.winner(),
        };
        report.validate_for_campaign(&next)?;
        next.tactical_battle_reports.push(report.clone());
        next.log.push(format!(
            "Turn {}: tactical battle in {} ends with {:?}.",
            next.turn, result.seed.target_province, result.reason
        ));
        if let Some(winner) = &report.campaign_winner {
            next.log.push(format!(
                "Turn {}: {} has won the campaign.",
                next.turn,
                next.faction_name(winner)
            ));
        }
        *self = next;
        Ok(report)
    }

    fn tactical_retreat_destination(
        &self,
        battlefield: &str,
        faction_id: &str,
        preferred: Option<&str>,
    ) -> Option<String> {
        let target = self
            .provinces
            .iter()
            .find(|province| province.id == battlefield)?;
        let mut legal: Vec<_> = target
            .neighbors
            .iter()
            .filter(|id| {
                self.provinces
                    .iter()
                    .any(|province| province.id == **id && province.owner == faction_id)
            })
            .map(String::as_str)
            .collect();
        legal.sort_unstable();
        let destination = preferred
            .filter(|id| legal.contains(id))
            .or_else(|| legal.first().copied())?;
        Some(destination.to_owned())
    }
}

impl TacticalCampaignReport {
    pub(crate) fn validate_for_campaign(
        &self,
        campaign: &CampaignState,
    ) -> Result<(), CampaignError> {
        self.validate()?;
        if self.result.seed.turn > campaign.turn {
            return Err(CampaignError::TacticalResultMismatch);
        }
        if campaign.pending_battle.as_ref().is_some_and(|pending| {
            self.result.seed.turn == campaign.turn
                && self.result.seed.attacker_army_id == pending.attacker_army_id
        }) || campaign
            .tactical_battle_reports
            .iter()
            .filter(|report| same_battle(&report.result, &self.result))
            .count()
            > 1
        {
            return Err(CampaignError::TacticalResultMismatch);
        }
        let target = campaign
            .provinces
            .iter()
            .find(|province| province.id == self.result.seed.target_province)
            .ok_or(CampaignError::TacticalResultMismatch)?;
        if self
            .retreats
            .iter()
            .any(|retreat| !target.neighbors.contains(&retreat.to_province))
        {
            return Err(CampaignError::TacticalResultMismatch);
        }
        Ok(())
    }

    pub fn validate(&self) -> Result<(), CampaignError> {
        self.result
            .validate()
            .map_err(|error| CampaignError::InvalidTacticalResult(error.to_string()))?;
        let captured = (self.result.winner == Some(BattleSide::Attacker))
            .then(|| self.result.seed.target_province.clone());
        if self.captured_province != captured {
            return Err(CampaignError::TacticalResultMismatch);
        }
        let mut resolved = BTreeSet::new();
        for retreat in &self.retreats {
            let source = self
                .result
                .armies
                .iter()
                .find(|army| army.source_army_id == retreat.source_army_id)
                .ok_or(CampaignError::TacticalResultMismatch)?;
            if !resolved.insert(source.source_army_id.as_str())
                || !needs_retreat(&self.result, source.side)
                || surviving_roster(source).is_empty()
                || retreat.from_province != self.result.seed.target_province
                || retreat.to_province.is_empty()
                || retreat.to_province == retreat.from_province
            {
                return Err(CampaignError::TacticalResultMismatch);
            }
        }
        for surrender in &self.surrenders {
            let source = self
                .result
                .armies
                .iter()
                .find(|army| army.source_army_id == surrender.source_army_id)
                .ok_or(CampaignError::TacticalResultMismatch)?;
            if !resolved.insert(source.source_army_id.as_str())
                || !needs_retreat(&self.result, source.side)
                || surrender.units.is_empty()
                || surrender.units != surviving_roster(source)
            {
                return Err(CampaignError::TacticalResultMismatch);
            }
        }
        if self.result.armies.iter().any(|army| {
            needs_retreat(&self.result, army.side)
                && !surviving_roster(army).is_empty()
                && !resolved.contains(army.source_army_id.as_str())
        }) {
            return Err(CampaignError::TacticalResultMismatch);
        }
        Ok(())
    }
}

fn needs_retreat(result: &TacticalBattleResult, side: BattleSide) -> bool {
    match result.winner {
        Some(winner) => side != winner,
        None => side == BattleSide::Attacker,
    }
}

fn surviving_roster(source: &TacticalArmyResult) -> Vec<TacticalUnitSeed> {
    source
        .units
        .iter()
        .filter_map(|unit| {
            (unit.surviving_soldiers > 0).then_some(TacticalUnitSeed {
                kind: unit.kind,
                soldiers: unit.surviving_soldiers,
            })
        })
        .collect()
}

fn same_battle(left: &TacticalBattleResult, right: &TacticalBattleResult) -> bool {
    left.seed.turn == right.seed.turn && left.seed.attacker_army_id == right.seed.attacker_army_id
}
