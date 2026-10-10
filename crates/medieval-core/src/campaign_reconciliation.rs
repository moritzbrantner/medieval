use crate::{
    Army, BattleSide, CampaignError, CampaignState, TacticalArmyResult, TacticalArmySeed,
    TacticalBattleResult, TacticalBattleSeed, UNIT_KINDS, UnitKind,
};

impl CampaignState {
    /// Commits exact tactical survivors while retaining the pending battle for
    /// strategic capture/retreat. An identical retry is a validated no-op.
    pub fn reconcile_tactical_casualties(
        &mut self,
        result: &TacticalBattleResult,
    ) -> Result<(), CampaignError> {
        validate_result(result)?;
        if let Some(applied) = &self.pending_tactical_result {
            if applied != result {
                return Err(CampaignError::TacticalCasualtiesAlreadyReconciled);
            }
            return self.validate_reconciled_tactical_state();
        }
        self.validate_pending_result_identity(result)?;
        let expected = self.pending_tactical_battle_seed()?;
        if normalized_seed(&expected) != normalized_seed(&result.seed) {
            return Err(CampaignError::TacticalResultMismatch);
        }

        // Build the complete replacement before touching live campaign state.
        let mut next = self.clone();
        for source in &result.armies {
            let army = next
                .armies
                .iter_mut()
                .find(|army| army.id == source.source_army_id)
                .ok_or(CampaignError::TacticalResultMismatch)?;
            if army.owner != source.faction_id
                || army.province != source_province(result, source.side)
            {
                return Err(CampaignError::TacticalResultMismatch);
            }
            for kind in UNIT_KINDS {
                let survivors = surviving_soldiers(source, kind);
                let soldiers =
                    u16::try_from(survivors).map_err(|_| CampaignError::TacticalResultMismatch)?;
                match kind {
                    UnitKind::Levy => army.levy = soldiers,
                    UnitKind::Spearmen => army.spearmen = soldiers,
                    UnitKind::Archers => army.archers = soldiers,
                    UnitKind::Knights => army.knights = soldiers,
                }
            }
        }
        next.armies.retain(|army| {
            !result.armies.iter().any(|source| {
                source.source_army_id == army.id
                    && source.units.iter().all(|unit| unit.surviving_soldiers == 0)
            })
        });
        next.pending_tactical_result = Some(result.clone());
        next.validate_reconciled_tactical_state()?;
        *self = next;
        Ok(())
    }

    pub(crate) fn validate_reconciled_tactical_state(&self) -> Result<(), CampaignError> {
        let Some(result) = &self.pending_tactical_result else {
            return Ok(());
        };
        validate_result(result)?;
        self.validate_pending_result_identity(result)?;
        for source in &result.armies {
            let army = self
                .armies
                .iter()
                .find(|army| army.id == source.source_army_id);
            if source.units.iter().all(|unit| unit.surviving_soldiers == 0) {
                if army.is_some() {
                    return Err(CampaignError::TacticalResultMismatch);
                }
                continue;
            }
            let army = army.ok_or(CampaignError::TacticalResultMismatch)?;
            if army.owner != source.faction_id
                || army.province != source_province(result, source.side)
                || UNIT_KINDS.into_iter().any(|kind| {
                    u64::from(army_soldiers(army, kind)) != surviving_soldiers(source, kind)
                })
            {
                return Err(CampaignError::TacticalResultMismatch);
            }
        }
        let mut actual_defenders: Vec<_> = self
            .armies
            .iter()
            .filter(|army| {
                army.owner == result.seed.defender.faction_id
                    && army.province == result.seed.target_province
            })
            .map(|army| army.id.as_str())
            .collect();
        actual_defenders.sort();
        let expected_defenders: Vec<_> = result
            .armies
            .iter()
            .filter(|army| {
                army.side == BattleSide::Defender
                    && army.units.iter().any(|unit| unit.surviving_soldiers > 0)
            })
            .map(|army| army.source_army_id.as_str())
            .collect();
        if actual_defenders != expected_defenders {
            return Err(CampaignError::TacticalResultMismatch);
        }
        Ok(())
    }

    fn validate_pending_result_identity(
        &self,
        result: &TacticalBattleResult,
    ) -> Result<(), CampaignError> {
        let pending = self
            .pending_battle
            .as_ref()
            .ok_or(CampaignError::NoPendingBattle)?;
        let seed = &result.seed;
        let province = self
            .provinces
            .iter()
            .find(|province| province.id == pending.target_province)
            .ok_or(CampaignError::TacticalResultMismatch)?;
        if seed.turn != self.turn
            || seed.attacker_army_id != pending.attacker_army_id
            || seed.from_province != pending.from_province
            || seed.target_province != pending.target_province
            || seed.attacker.faction_id != pending.attacker_faction
            || seed.defender.faction_id != pending.defender_faction
            || seed.attacker.source_army_ids != [pending.attacker_army_id.clone()]
            || seed.battlefield_profile != province.tactical_battlefield_profile()
            || province.owner != pending.defender_faction
        {
            return Err(CampaignError::TacticalResultMismatch);
        }
        Ok(())
    }
}

fn validate_result(result: &TacticalBattleResult) -> Result<(), CampaignError> {
    result
        .validate()
        .map_err(|error| CampaignError::InvalidTacticalResult(error.to_string()))
}

fn normalized_seed(seed: &TacticalBattleSeed) -> TacticalBattleSeed {
    let mut seed = seed.clone();
    for force in [&mut seed.attacker, &mut seed.defender] {
        if force.source_armies.is_none() {
            force.source_armies = match force.source_army_ids.as_slice() {
                [id] => Some(vec![TacticalArmySeed {
                    army_id: id.clone(),
                    units: force.units.clone(),
                }]),
                [] if force.units.is_empty() => Some(Vec::new()),
                _ => None,
            };
        }
    }
    seed
}

fn source_province(result: &TacticalBattleResult, side: BattleSide) -> &str {
    match side {
        BattleSide::Attacker => &result.seed.from_province,
        BattleSide::Defender => &result.seed.target_province,
    }
}

fn surviving_soldiers(source: &TacticalArmyResult, kind: UnitKind) -> u64 {
    source
        .units
        .iter()
        .find(|unit| unit.kind == kind)
        .map_or(0, |unit| unit.surviving_soldiers)
}

fn army_soldiers(army: &Army, kind: UnitKind) -> u16 {
    match kind {
        UnitKind::Levy => army.levy,
        UnitKind::Spearmen => army.spearmen,
        UnitKind::Archers => army.archers,
        UnitKind::Knights => army.knights,
    }
}
