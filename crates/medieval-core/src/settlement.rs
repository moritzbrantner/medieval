//! Core-owned settlement levels and deterministic upgrade progression.
//!
//! Every province carries one [`SettlementLevel`]. The level's
//! [`SettlementLevelSpec`] is the single source for its income bonus and
//! building capacity, and for the requirements of upgrading into it. Upgrades
//! are queued as [`SettlementUpgradeOrder`]s and complete at the start of the
//! owning faction's turn, before that turn's income is paid.

use serde::{Deserialize, Serialize};

use crate::{CampaignError, CampaignState};

#[derive(
    Copy, Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(rename_all = "camelCase")]
pub enum SettlementLevel {
    #[default]
    Village,
    Town,
    City,
    MajorCity,
}

/// Explicit requirements for upgrading a settlement into a level.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SettlementUpgradeRequirement {
    /// Gold paid by the owning faction when the upgrade is queued.
    pub cost: u32,
    /// Full faction rounds until the upgrade completes.
    pub rounds: u32,
    /// Minimum province wealth required to queue the upgrade.
    pub minimum_wealth: u32,
}

/// Documented gameplay effects of a settlement level.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SettlementLevelSpec {
    pub level: SettlementLevel,
    pub label: &'static str,
    /// Gold added to the owner's provincial income every faction turn.
    pub income_bonus: u32,
    /// Building capacity reserved for the construction system.
    pub building_slots: u8,
    /// Requirements to upgrade into this level; `None` for the lowest level.
    pub upgrade: Option<SettlementUpgradeRequirement>,
}

pub const SETTLEMENT_LEVELS: [SettlementLevel; 4] = [
    SettlementLevel::Village,
    SettlementLevel::Town,
    SettlementLevel::City,
    SettlementLevel::MajorCity,
];

impl SettlementLevel {
    #[must_use]
    pub fn spec(self) -> SettlementLevelSpec {
        match self {
            Self::Village => SettlementLevelSpec {
                level: self,
                label: "Village",
                income_bonus: 0,
                building_slots: 1,
                upgrade: None,
            },
            Self::Town => SettlementLevelSpec {
                level: self,
                label: "Town",
                income_bonus: 50,
                building_slots: 2,
                upgrade: Some(SettlementUpgradeRequirement {
                    cost: 400,
                    rounds: 1,
                    minimum_wealth: 4,
                }),
            },
            Self::City => SettlementLevelSpec {
                level: self,
                label: "City",
                income_bonus: 100,
                building_slots: 3,
                upgrade: Some(SettlementUpgradeRequirement {
                    cost: 800,
                    rounds: 2,
                    minimum_wealth: 6,
                }),
            },
            Self::MajorCity => SettlementLevelSpec {
                level: self,
                label: "Major city",
                income_bonus: 200,
                building_slots: 4,
                upgrade: Some(SettlementUpgradeRequirement {
                    cost: 1_600,
                    rounds: 3,
                    minimum_wealth: 8,
                }),
            },
        }
    }

    #[must_use]
    pub fn label(self) -> &'static str {
        self.spec().label
    }

    #[must_use]
    pub fn next(self) -> Option<Self> {
        match self {
            Self::Village => Some(Self::Town),
            Self::Town => Some(Self::City),
            Self::City => Some(Self::MajorCity),
            Self::MajorCity => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SettlementUpgradeOrder {
    pub faction_id: String,
    pub province_id: String,
    pub target_level: SettlementLevel,
    pub cost: u32,
    pub ready_on_turn: u32,
}

/// The authoritative settlement view for one province: the current level's
/// effects and whether the active faction may upgrade it now.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SettlementUpgradeOption {
    pub province_id: String,
    pub current: SettlementLevelSpec,
    pub target: Option<SettlementLevelSpec>,
    pub queued: Option<SettlementUpgradeOrder>,
    pub ready_on_turn: Option<u32>,
    pub available: bool,
    pub reason: Option<String>,
}

impl CampaignState {
    pub fn settlement_upgrade_option(
        &self,
        province_id: &str,
    ) -> Result<SettlementUpgradeOption, CampaignError> {
        let province = self.province(province_id)?;
        let faction = self.faction(&self.active_faction)?;
        let current = province.settlement_level;
        let target = current.next();
        let queued = self
            .settlement_upgrades
            .iter()
            .find(|order| order.province_id == province.id)
            .cloned();
        let reason = self
            .settlement_upgrade_error(province_id, faction.treasury)
            .err()
            .map(|error| self.settlement_upgrade_reason(&error));
        Ok(SettlementUpgradeOption {
            province_id: province.id.clone(),
            current: current.spec(),
            target: target.map(SettlementLevel::spec),
            queued,
            ready_on_turn: target.map(|level| self.settlement_ready_on_turn(level)),
            available: reason.is_none(),
            reason,
        })
    }

    pub fn queue_settlement_upgrade(&mut self, province_id: &str) -> Result<(), CampaignError> {
        let faction_index = self
            .factions
            .iter()
            .position(|faction| faction.id == self.active_faction)
            .ok_or_else(|| CampaignError::FactionNotFound(self.active_faction.clone()))?;
        let (target, requirement) =
            self.settlement_upgrade_error(province_id, self.factions[faction_index].treasury)?;
        let province_name = self.province(province_id)?.name.clone();
        let ready_on_turn = self.settlement_ready_on_turn(target);

        self.factions[faction_index].treasury -= requirement.cost;
        self.settlement_upgrades.push(SettlementUpgradeOrder {
            faction_id: self.active_faction.clone(),
            province_id: province_id.to_owned(),
            target_level: target,
            cost: requirement.cost,
            ready_on_turn,
        });
        self.log.push(format!(
            "Turn {}: {} begins upgrading {} to a {} for {} gold, ready on turn {}.",
            self.turn,
            self.factions[faction_index].name,
            province_name,
            target.label().to_lowercase(),
            requirement.cost,
            ready_on_turn
        ));
        Ok(())
    }

    /// Completes the active faction's due settlement upgrades. Completed orders
    /// leave the queue, so repeating the call for the same turn is a no-op.
    pub(crate) fn complete_settlement_upgrades(&mut self) -> Result<(), CampaignError> {
        let faction_id = self.active_faction.clone();
        let turn = self.turn;
        let due = |order: &SettlementUpgradeOrder| {
            order.faction_id == faction_id && order.ready_on_turn <= turn
        };
        let ready: Vec<SettlementUpgradeOrder> = self
            .settlement_upgrades
            .iter()
            .filter(|order| due(order))
            .cloned()
            .collect();
        self.settlement_upgrades.retain(|order| !due(order));

        for order in ready {
            let province = self
                .provinces
                .iter_mut()
                .find(|province| province.id == order.province_id)
                .ok_or_else(|| CampaignError::ProvinceNotFound(order.province_id.clone()))?;
            if province.owner != order.faction_id
                || province.settlement_level.next() != Some(order.target_level)
            {
                continue;
            }
            province.settlement_level = order.target_level;
            let province_name = province.name.clone();
            self.log.push(format!(
                "Turn {}: {} is now a {}.",
                self.turn,
                province_name,
                order.target_level.label().to_lowercase()
            ));
        }
        Ok(())
    }

    /// Cancels a former owner's upgrade after the province changes hands.
    pub(crate) fn cancel_settlement_upgrades_after_capture(
        &mut self,
        province_id: &str,
        new_owner: &str,
    ) {
        let queued = self.settlement_upgrades.len();
        self.settlement_upgrades
            .retain(|order| order.province_id != province_id || order.faction_id == new_owner);
        if self.settlement_upgrades.len() < queued {
            self.log.push(format!(
                "Turn {}: the settlement upgrade in {province_id} is cancelled after capture.",
                self.turn
            ));
        }
    }

    fn settlement_ready_on_turn(&self, target: SettlementLevel) -> u32 {
        let rounds = target.spec().upgrade.map_or(0, |upgrade| upgrade.rounds);
        self.turn
            .saturating_add(rounds.saturating_mul(self.factions.len() as u32))
    }

    fn settlement_upgrade_error(
        &self,
        province_id: &str,
        treasury: u32,
    ) -> Result<(SettlementLevel, SettlementUpgradeRequirement), CampaignError> {
        if self.pending_battle.is_some() {
            return Err(CampaignError::BattlePending);
        }
        let province = self.province(province_id)?;
        if province.owner != self.active_faction {
            return Err(CampaignError::SettlementNotControlled {
                province: province.name.clone(),
                owner: province.owner.clone(),
                active_faction: self.active_faction.clone(),
            });
        }
        if let Some(order) = self
            .settlement_upgrades
            .iter()
            .find(|order| order.province_id == province.id)
        {
            return Err(CampaignError::SettlementUpgradeAlreadyQueued {
                province: province.name.clone(),
                ready_on_turn: order.ready_on_turn,
            });
        }
        let Some(target) = province.settlement_level.next() else {
            return Err(CampaignError::SettlementAtMaximumLevel(
                province.name.clone(),
            ));
        };
        let requirement = target
            .spec()
            .upgrade
            .expect("every level above the lowest has upgrade requirements");
        if province.wealth < requirement.minimum_wealth {
            return Err(CampaignError::SettlementWealthTooLow {
                province: province.name.clone(),
                wealth: province.wealth,
                required: requirement.minimum_wealth,
            });
        }
        if treasury < requirement.cost {
            return Err(CampaignError::InsufficientTreasury {
                faction: self.faction_name(&self.active_faction),
                cost: requirement.cost,
                treasury,
            });
        }
        Ok((target, requirement))
    }

    fn settlement_upgrade_reason(&self, error: &CampaignError) -> String {
        match error {
            CampaignError::BattlePending => {
                "Resolve the pending battle before upgrading.".to_owned()
            }
            CampaignError::SettlementNotControlled { province, .. } => format!(
                "Only {} can build during this turn, and it does not control {province}.",
                self.faction_name(&self.active_faction)
            ),
            CampaignError::InsufficientTreasury { cost, treasury, .. } => {
                format!("Need {cost} gold; the treasury has {treasury}.")
            }
            other => format!("{other}."),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CampaignSave, new_campaign};

    fn province_level(campaign: &CampaignState, id: &str) -> SettlementLevel {
        campaign
            .provinces
            .iter()
            .find(|province| province.id == id)
            .unwrap()
            .settlement_level
    }

    #[test]
    fn levels_progress_in_order_with_increasing_requirements_and_effects() {
        let mut previous = SettlementLevel::Village.spec();
        assert!(previous.upgrade.is_none());
        for level in &SETTLEMENT_LEVELS[1..] {
            let spec = level.spec();
            let upgrade = spec.upgrade.unwrap();
            let previous_upgrade = previous.upgrade.unwrap_or(SettlementUpgradeRequirement {
                cost: 0,
                rounds: 0,
                minimum_wealth: 0,
            });
            assert_eq!(previous.level.next(), Some(*level));
            assert!(spec.income_bonus > previous.income_bonus);
            assert!(spec.building_slots > previous.building_slots);
            assert!(upgrade.cost > previous_upgrade.cost);
            assert!(upgrade.rounds >= previous_upgrade.rounds);
            assert!(upgrade.minimum_wealth > previous_upgrade.minimum_wealth);
            previous = spec;
        }
        assert_eq!(SettlementLevel::MajorCity.next(), None);
    }

    #[test]
    fn packaged_provinces_start_with_explicit_levels() {
        let campaign = new_campaign();
        assert_eq!(province_level(&campaign, "normandy"), SettlementLevel::Town);
        assert_eq!(province_level(&campaign, "paris"), SettlementLevel::City);
        assert_eq!(
            province_level(&campaign, "wessex"),
            SettlementLevel::Village
        );
    }

    #[test]
    fn option_reports_current_effects_target_and_requirements() {
        let campaign = new_campaign();
        let option = campaign.settlement_upgrade_option("wessex").unwrap();
        assert_eq!(option.current, SettlementLevel::Village.spec());
        assert_eq!(option.target, Some(SettlementLevel::Town.spec()));
        assert_eq!(option.ready_on_turn, Some(3));
        assert!(option.available);
        assert!(option.reason.is_none());

        let enemy = campaign.settlement_upgrade_option("paris").unwrap();
        assert!(!enemy.available);
        assert!(enemy.reason.unwrap().contains("does not control Paris"));
    }

    #[test]
    fn upgrade_pays_cost_and_completes_once_at_the_owners_turn_start() {
        let mut campaign = new_campaign();
        campaign.queue_settlement_upgrade("wessex").unwrap();
        assert_eq!(campaign.factions[0].treasury, 800);
        assert!(matches!(
            campaign.queue_settlement_upgrade("wessex"),
            Err(CampaignError::SettlementUpgradeAlreadyQueued {
                ready_on_turn: 3,
                ..
            })
        ));
        let queued = campaign.settlement_upgrade_option("wessex").unwrap();
        assert!(!queued.available);
        assert_eq!(queued.queued.unwrap().target_level, SettlementLevel::Town);

        campaign.end_turn().unwrap();
        assert_eq!(
            province_level(&campaign, "wessex"),
            SettlementLevel::Village
        );
        campaign.end_turn().unwrap();
        assert_eq!(province_level(&campaign, "wessex"), SettlementLevel::Town);
        assert!(campaign.settlement_upgrades.is_empty());

        // Income on the completion turn already includes the new town bonus:
        // wealth 11 * 50 + Normandy town 50 + Wessex town 50.
        assert_eq!(campaign.factions[0].treasury, 800 + 650);

        let before = campaign.clone();
        campaign.apply_turn_start().unwrap();
        campaign.complete_settlement_upgrades().unwrap();
        assert_eq!(campaign, before);
    }

    #[test]
    fn upgrades_require_wealth_treasury_and_a_higher_level() {
        let mut campaign = new_campaign();
        campaign.provinces[0].wealth = 3;
        assert!(matches!(
            campaign.queue_settlement_upgrade("wessex"),
            Err(CampaignError::SettlementWealthTooLow { required: 4, .. })
        ));

        campaign.provinces[0].wealth = 5;
        campaign.factions[0].treasury = 399;
        assert!(matches!(
            campaign.queue_settlement_upgrade("wessex"),
            Err(CampaignError::InsufficientTreasury { cost: 400, .. })
        ));

        campaign.provinces[0].settlement_level = SettlementLevel::MajorCity;
        assert!(matches!(
            campaign.queue_settlement_upgrade("wessex"),
            Err(CampaignError::SettlementAtMaximumLevel(_))
        ));
        let option = campaign.settlement_upgrade_option("wessex").unwrap();
        assert_eq!(option.target, None);
        assert_eq!(option.ready_on_turn, None);
        assert!(!option.available);
    }

    #[test]
    fn captured_provinces_keep_their_level_but_cancel_the_former_owners_upgrade() {
        let mut campaign = new_campaign();
        campaign.queue_settlement_upgrade("wessex").unwrap();
        campaign.provinces[0].owner = "france".into();
        campaign.cancel_settlement_upgrades_after_capture("wessex", "france");
        assert!(campaign.settlement_upgrades.is_empty());
        assert_eq!(
            province_level(&campaign, "wessex"),
            SettlementLevel::Village
        );
    }

    #[test]
    fn queued_upgrades_round_trip_through_saves_and_invalid_orders_are_rejected() {
        let mut campaign = new_campaign();
        campaign.queue_settlement_upgrade("normandy").unwrap();
        let save = CampaignSave::from_campaign(campaign, "england").unwrap();
        assert_eq!(
            CampaignSave::from_json(&save.to_json().unwrap()).unwrap(),
            save
        );

        let mut skipped = save.clone();
        skipped.campaign.settlement_upgrades[0].target_level = SettlementLevel::MajorCity;
        assert!(skipped.validate().is_err());

        let mut foreign = save.clone();
        foreign.campaign.settlement_upgrades[0].faction_id = "france".into();
        assert!(foreign.validate().is_err());

        let mut duplicate = save;
        let order = duplicate.campaign.settlement_upgrades[0].clone();
        duplicate.campaign.settlement_upgrades.push(order);
        assert!(duplicate.validate().is_err());
    }
}
