use serde::{Deserialize, Serialize};

use crate::{Army, CampaignError, CampaignState, UnitKind};

const DEFENDER_MODIFIER_PERCENT: u32 = 8;
const AI_ATTACK_SCORE: u64 = 1_000_000;
const AI_REINFORCE_SCORE: u64 = 100_000;
const AI_FRIENDLY_MOVE_SCORE: u64 = 10_000;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArmyRoster {
    pub levy: u32,
    pub spearmen: u32,
    pub archers: u32,
    pub knights: u32,
}

impl ArmyRoster {
    #[must_use]
    pub fn soldiers(&self) -> u32 {
        self.levy
            .saturating_add(self.spearmen)
            .saturating_add(self.archers)
            .saturating_add(self.knights)
    }

    fn strength(&self) -> u64 {
        u64::from(self.levy)
            + u64::from(self.spearmen).saturating_mul(2)
            + u64::from(self.archers).saturating_mul(2)
            + u64::from(self.knights).saturating_mul(5)
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BattleOutcome {
    AttackerVictory,
    DefenderVictory,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BattleReport {
    pub seed: u64,
    pub turn: u32,
    pub attacker_army_id: String,
    pub attacker_faction: String,
    pub defender_faction: String,
    pub from_province: String,
    pub target_province: String,
    pub defender_modifier_percent: u32,
    pub attacker_score: u64,
    pub defender_score: u64,
    pub attacker_before: ArmyRoster,
    pub attacker_after: ArmyRoster,
    pub defender_before: ArmyRoster,
    pub defender_after: ArmyRoster,
    pub attacker_casualty_percent: u32,
    pub defender_casualty_percent: u32,
    pub outcome: BattleOutcome,
    pub captured: bool,
    pub defender_retreat_province: Option<String>,
}

impl CampaignState {
    pub fn resolve_pending_battle(&mut self, seed: u64) -> Result<BattleReport, CampaignError> {
        let pending = self
            .pending_battle
            .clone()
            .ok_or(CampaignError::NoPendingBattle)?;

        let attacker_index = self
            .armies
            .iter()
            .position(|army| army.id == pending.attacker_army_id)
            .ok_or_else(|| CampaignError::ArmyNotFound(pending.attacker_army_id.clone()))?;

        let defender_indices: Vec<usize> = self
            .armies
            .iter()
            .enumerate()
            .filter(|(_, army)| {
                army.owner == pending.defender_faction && army.province == pending.target_province
            })
            .map(|(index, _)| index)
            .collect();

        let attacker_before = roster_from_army(&self.armies[attacker_index]);
        let defender_before = roster_from_indices(&self.armies, &defender_indices);

        let attacker_score = rolled_score(attacker_before.strength(), seed, 0xA771_A771);
        let defender_raw = rolled_score(defender_before.strength(), seed, 0xD3F3_D3F3);
        let defender_score =
            defender_raw.saturating_mul(u64::from(100 + DEFENDER_MODIFIER_PERCENT)) / 100;

        let outcome = if attacker_score > defender_score {
            BattleOutcome::AttackerVictory
        } else {
            BattleOutcome::DefenderVictory
        };

        let (attacker_casualty_percent, defender_casualty_percent) = match outcome {
            BattleOutcome::AttackerVictory => (
                casualty_percent(seed, 0xA11A, 20, 36),
                casualty_percent(seed, 0xD11D, 65, 86),
            ),
            BattleOutcome::DefenderVictory => (
                casualty_percent(seed, 0xA22A, 65, 86),
                casualty_percent(seed, 0xD22D, 20, 36),
            ),
        };

        apply_casualties(&mut self.armies[attacker_index], attacker_casualty_percent);
        for index in &defender_indices {
            apply_casualties(&mut self.armies[*index], defender_casualty_percent);
        }

        let attacker_after = roster_from_army(&self.armies[attacker_index]);
        let defender_after = roster_from_indices(&self.armies, &defender_indices);

        let defender_retreat_province =
            if outcome == BattleOutcome::AttackerVictory && defender_after.soldiers() > 0 {
                self.provinces
                    .iter()
                    .find(|province| province.id == pending.target_province)
                    .and_then(|target| {
                        target.neighbors.iter().find(|neighbor_id| {
                            self.provinces.iter().any(|province| {
                                province.id == **neighbor_id
                                    && province.owner == pending.defender_faction
                            })
                        })
                    })
                    .cloned()
            } else {
                None
            };

        let captured = outcome == BattleOutcome::AttackerVictory;
        if captured {
            self.armies[attacker_index].province = pending.target_province.clone();
            if let Some(province) = self
                .provinces
                .iter_mut()
                .find(|province| province.id == pending.target_province)
            {
                province.owner = pending.attacker_faction.clone();
            }

            let queued_before_capture = self.recruitment_queue.len();
            self.recruitment_queue.retain(|order| {
                !(order.faction_id == pending.defender_faction
                    && order.province_id == pending.target_province)
            });
            let cancelled_orders =
                queued_before_capture.saturating_sub(self.recruitment_queue.len());
            if cancelled_orders > 0 {
                self.log.push(format!(
                    "Turn {}: {} queued recruitment order(s) in {} are cancelled after capture.",
                    self.turn, cancelled_orders, pending.target_province
                ));
            }

            if let Some(retreat) = &defender_retreat_province {
                for index in &defender_indices {
                    self.armies[*index].province = retreat.clone();
                    self.armies[*index].moved_this_turn = true;
                }
            }
        }

        if captured && defender_retreat_province.is_none() {
            self.armies.retain(|army| {
                !(army.owner == pending.defender_faction
                    && army.province == pending.target_province)
            });
        }
        self.armies.retain(|army| army_soldiers(army) > 0);

        let report = BattleReport {
            seed,
            turn: self.turn,
            attacker_army_id: pending.attacker_army_id.clone(),
            attacker_faction: pending.attacker_faction.clone(),
            defender_faction: pending.defender_faction.clone(),
            from_province: pending.from_province.clone(),
            target_province: pending.target_province.clone(),
            defender_modifier_percent: DEFENDER_MODIFIER_PERCENT,
            attacker_score,
            defender_score,
            attacker_before,
            attacker_after,
            defender_before,
            defender_after,
            attacker_casualty_percent,
            defender_casualty_percent,
            outcome,
            captured,
            defender_retreat_province,
        };

        self.pending_battle = None;
        self.battle_reports.push(report.clone());
        self.log.push(match report.outcome {
            BattleOutcome::AttackerVictory => format!(
                "Turn {}: {} captures {} after battle seed {}.",
                self.turn, report.attacker_faction, report.target_province, seed
            ),
            BattleOutcome::DefenderVictory => format!(
                "Turn {}: {} holds {} after battle seed {}.",
                self.turn, report.defender_faction, report.target_province, seed
            ),
        });

        if let Some(winner) = self.winner() {
            self.log.push(format!(
                "Turn {}: {} has won the campaign.",
                self.turn,
                self.faction_name(&winner)
            ));
        }

        Ok(report)
    }

    /// Configure a freshly-created campaign so the chosen faction receives the
    /// first human turn without applying a synthetic economy tick.
    pub fn select_player_faction(&mut self, faction_id: &str) -> Result<(), CampaignError> {
        self.faction(faction_id)?;
        self.active_faction = faction_id.to_owned();
        for faction in &mut self.factions {
            faction.last_economy_turn = (faction.id == faction_id).then_some(self.turn);
        }
        for army in &mut self.armies {
            army.moved_this_turn = false;
        }
        self.pending_battle = None;
        self.log.push(format!(
            "Turn {}: {} is chosen as the player faction.",
            self.turn,
            self.faction_name(faction_id)
        ));
        Ok(())
    }

    /// Returns the authoritative winning faction, if the campaign is over.
    /// A faction wins by controlling every province or by being the only
    /// faction that still controls a province or fields an army.
    #[must_use]
    pub fn winner(&self) -> Option<String> {
        self.factions
            .iter()
            .find(|faction| {
                !self.provinces.is_empty()
                    && self
                        .provinces
                        .iter()
                        .all(|province| province.owner == faction.id)
            })
            .map(|faction| faction.id.clone())
            .or_else(|| {
                let living: Vec<&str> = self
                    .factions
                    .iter()
                    .filter(|faction| {
                        self.provinces
                            .iter()
                            .any(|province| province.owner == faction.id)
                            || self.armies.iter().any(|army| army.owner == faction.id)
                    })
                    .map(|faction| faction.id.as_str())
                    .collect();
                (living.len() == 1).then(|| living[0].to_owned())
            })
    }

    /// Play one complete deterministic AI faction turn using the same public
    /// recruitment, movement, battle-resolution, and end-turn commands that a
    /// human turn uses. The caller supplies the human faction only to identify
    /// which active faction must never be automated.
    pub fn play_ai_turn(&mut self, human_faction: &str, seed: u64) -> Result<(), CampaignError> {
        self.faction(human_faction)?;
        if self.active_faction == human_faction || self.winner().is_some() {
            return Ok(());
        }

        let ai_faction = self.active_faction.clone();
        self.log.push(format!(
            "Turn {}: {} plans its deterministic opponent turn with seed {}.",
            self.turn,
            self.faction_name(&ai_faction),
            seed
        ));

        self.ai_recruit(seed)?;
        self.ai_move(seed)?;

        if self.pending_battle.is_some() {
            self.resolve_pending_battle(mix(seed, 0xB477_1E5E_DA7A_5EED))?;
        }

        if self.winner().is_none() {
            self.end_turn()?;
        }
        Ok(())
    }

    fn ai_recruit(&mut self, seed: u64) -> Result<(), CampaignError> {
        let faction_id = self.active_faction.clone();
        let mut province_candidates: Vec<(u64, String)> = self
            .provinces
            .iter()
            .filter(|province| province.owner == faction_id)
            .map(|province| {
                let frontier = province.neighbors.iter().any(|neighbor_id| {
                    self.provinces
                        .iter()
                        .any(|neighbor| neighbor.id == *neighbor_id && neighbor.owner != faction_id)
                });
                let score = u64::from(frontier) * AI_REINFORCE_SCORE
                    + u64::from(province.wealth).saturating_mul(100)
                    + mix(seed, hash_text(&province.id)) % 100;
                (score, province.id.clone())
            })
            .collect();
        province_candidates
            .sort_by(|left, right| right.0.cmp(&left.0).then_with(|| left.1.cmp(&right.1)));

        for (_, province_id) in province_candidates {
            let options: Vec<UnitKind> = self
                .recruitment_options(&province_id)?
                .into_iter()
                .filter(|option| option.available)
                .map(|option| option.unit)
                .collect();
            if options.is_empty() {
                continue;
            }

            let option_index = usize::try_from(
                mix(seed, hash_text(&province_id) ^ 0xA11C_E001) % options.len() as u64,
            )
            .unwrap_or(0);
            self.queue_recruitment(&province_id, options[option_index])?;
            break;
        }
        Ok(())
    }

    fn ai_move(&mut self, seed: u64) -> Result<(), CampaignError> {
        let faction_id = self.active_faction.clone();
        let mut army_ids: Vec<String> = self
            .armies
            .iter()
            .filter(|army| army.owner == faction_id && !army.moved_this_turn)
            .map(|army| army.id.clone())
            .collect();
        army_ids.sort();

        let mut choices: Vec<(u64, String, String)> = Vec::new();
        for army_id in army_ids {
            for destination in self.legal_destinations(&army_id)? {
                let province = self.province(&destination)?;
                let hostile = province.owner != faction_id;
                let friendly_frontier = !hostile
                    && province.neighbors.iter().any(|neighbor_id| {
                        self.provinces.iter().any(|neighbor| {
                            neighbor.id == *neighbor_id && neighbor.owner != faction_id
                        })
                    });
                let strategic_score = if hostile {
                    AI_ATTACK_SCORE
                } else if friendly_frontier {
                    AI_REINFORCE_SCORE
                } else {
                    AI_FRIENDLY_MOVE_SCORE
                };
                let tie_break = mix(
                    seed,
                    hash_text(&format!("{army_id}:{destination}")) ^ 0xA11C_0A0E,
                ) % 1_000;
                let score =
                    strategic_score + u64::from(province.wealth).saturating_mul(100) + tie_break;
                choices.push((score, army_id.clone(), destination));
            }
        }

        choices.sort_by(|left, right| {
            right
                .0
                .cmp(&left.0)
                .then_with(|| left.1.cmp(&right.1))
                .then_with(|| left.2.cmp(&right.2))
        });

        if let Some((_, army_id, destination)) = choices.first() {
            self.move_army(army_id, destination)?;
        }
        Ok(())
    }
}

fn roster_from_army(army: &Army) -> ArmyRoster {
    ArmyRoster {
        levy: u32::from(army.levy),
        spearmen: u32::from(army.spearmen),
        archers: u32::from(army.archers),
        knights: u32::from(army.knights),
    }
}

fn roster_from_indices(armies: &[Army], indices: &[usize]) -> ArmyRoster {
    indices
        .iter()
        .fold(ArmyRoster::default(), |mut total, index| {
            let roster = roster_from_army(&armies[*index]);
            total.levy = total.levy.saturating_add(roster.levy);
            total.spearmen = total.spearmen.saturating_add(roster.spearmen);
            total.archers = total.archers.saturating_add(roster.archers);
            total.knights = total.knights.saturating_add(roster.knights);
            total
        })
}

fn apply_casualties(army: &mut Army, casualty_percent: u32) {
    army.levy = survivors(army.levy, casualty_percent);
    army.spearmen = survivors(army.spearmen, casualty_percent);
    army.archers = survivors(army.archers, casualty_percent);
    army.knights = survivors(army.knights, casualty_percent);
}

fn survivors(count: u16, casualty_percent: u32) -> u16 {
    let casualties = u32::from(count).saturating_mul(casualty_percent.min(100)) / 100;
    count.saturating_sub(u16::try_from(casualties).unwrap_or(count))
}

fn army_soldiers(army: &Army) -> u32 {
    u32::from(army.levy)
        .saturating_add(u32::from(army.spearmen))
        .saturating_add(u32::from(army.archers))
        .saturating_add(u32::from(army.knights))
}

fn rolled_score(strength: u64, seed: u64, salt: u64) -> u64 {
    let factor = 90 + (mix(seed, salt) % 21);
    strength.saturating_mul(factor)
}

fn casualty_percent(seed: u64, salt: u64, min: u32, max_exclusive: u32) -> u32 {
    let width = u64::from(max_exclusive.saturating_sub(min).max(1));
    min.saturating_add(u32::try_from(mix(seed, salt) % width).unwrap_or(0))
}

fn hash_text(text: &str) -> u64 {
    text.as_bytes()
        .iter()
        .fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
            (hash ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01B3)
        })
}

fn mix(seed: u64, salt: u64) -> u64 {
    let mut value = seed ^ salt ^ 0x9E37_79B9_7F4A_7C15;
    value ^= value >> 12;
    value ^= value << 25;
    value ^= value >> 27;
    value.wrapping_mul(0x2545_F491_4F6C_DD1D)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{UnitKind, new_campaign};

    fn contested_campaign() -> CampaignState {
        let mut campaign = new_campaign();
        campaign.move_army("england-main", "paris").unwrap();
        campaign
    }

    #[test]
    fn same_seed_replays_to_identical_report_and_state() {
        let campaign = contested_campaign();
        let mut first = campaign.clone();
        let mut second = campaign;

        let first_report = first.resolve_pending_battle(42).unwrap();
        let second_report = second.resolve_pending_battle(42).unwrap();

        assert_eq!(first_report, second_report);
        assert_eq!(first, second);
    }

    #[test]
    fn casualties_never_create_soldiers() {
        let mut campaign = contested_campaign();
        let report = campaign.resolve_pending_battle(7).unwrap();

        assert!(report.attacker_after.soldiers() <= report.attacker_before.soldiers());
        assert!(report.defender_after.soldiers() <= report.defender_before.soldiers());
    }

    #[test]
    fn casualty_rounding_does_not_overstate_small_group_losses() {
        assert_eq!(survivors(1, 20), 1);
        assert_eq!(survivors(4, 20), 4);
        assert_eq!(survivors(5, 20), 4);
        assert_eq!(survivors(1, 100), 0);
    }

    #[test]
    fn attacker_victory_captures_the_target_province() {
        let base = contested_campaign();
        let (mut campaign, report) = (0..10_000)
            .find_map(|seed| {
                let mut attempt = base.clone();
                let report = attempt.resolve_pending_battle(seed).unwrap();
                (report.outcome == BattleOutcome::AttackerVictory).then_some((attempt, report))
            })
            .expect("expected at least one attacker-winning seed");

        let paris = campaign
            .provinces
            .iter()
            .find(|province| province.id == "paris")
            .unwrap();
        assert_eq!(paris.owner, "england");
        assert!(report.captured);
        assert!(campaign.pending_battle.is_none());
        assert_eq!(campaign.battle_reports.last(), Some(&report));

        campaign.battle_reports.clear();
        assert!(campaign.resolve_pending_battle(1).is_err());
    }

    #[test]
    fn capture_cancels_former_owners_recruitment_in_the_province() {
        let mut base = new_campaign();
        base.end_turn().unwrap();
        base.queue_recruitment("paris", UnitKind::Levy).unwrap();
        base.end_turn().unwrap();
        base.move_army("england-main", "paris").unwrap();

        let mut campaign = (0..10_000)
            .find_map(|seed| {
                let mut attempt = base.clone();
                let report = attempt.resolve_pending_battle(seed).unwrap();
                (report.outcome == BattleOutcome::AttackerVictory).then_some(attempt)
            })
            .expect("expected at least one attacker-winning seed");

        assert!(
            campaign
                .recruitment_queue
                .iter()
                .all(|order| { order.faction_id != "france" || order.province_id != "paris" })
        );

        campaign.end_turn().unwrap();
        assert!(
            !campaign
                .armies
                .iter()
                .any(|army| army.owner == "france" && army.province == "paris")
        );
    }

    #[test]
    fn player_can_choose_either_starting_faction_without_synthetic_income() {
        let mut campaign = new_campaign();
        campaign.select_player_faction("france").unwrap();

        assert_eq!(campaign.active_faction, "france");
        assert_eq!(campaign.turn, 1);
        assert_eq!(campaign.year, 1087);
        assert_eq!(campaign.faction("france").unwrap().treasury, 1_200);
        assert_eq!(
            campaign.faction("france").unwrap().last_economy_turn,
            Some(1)
        );
        assert_eq!(campaign.faction("england").unwrap().last_economy_turn, None);
    }

    #[test]
    fn choosing_an_unknown_player_faction_fails_closed() {
        let mut campaign = new_campaign();
        let before = campaign.clone();

        assert!(matches!(
            campaign.select_player_faction("vikings"),
            Err(CampaignError::FactionNotFound(_))
        ));
        assert_eq!(campaign, before);
    }

    #[test]
    fn winner_is_none_while_both_factions_remain_viable() {
        assert_eq!(new_campaign().winner(), None);
    }

    #[test]
    fn controlling_every_province_is_an_authoritative_win() {
        let mut campaign = new_campaign();
        for province in &mut campaign.provinces {
            province.owner = "england".into();
        }

        assert_eq!(campaign.winner().as_deref(), Some("england"));
    }

    #[test]
    fn eliminating_every_province_and_army_is_an_authoritative_loss() {
        let mut campaign = new_campaign();
        for province in &mut campaign.provinces {
            province.owner = "france".into();
        }
        campaign.armies.retain(|army| army.owner != "england");

        assert_eq!(campaign.winner().as_deref(), Some("france"));
    }

    #[test]
    fn ai_turn_is_reproducible_for_same_state_and_seed() {
        let mut base = new_campaign();
        base.end_turn().unwrap();
        let mut first = base.clone();
        let mut second = base;

        first.play_ai_turn("england", 81).unwrap();
        second.play_ai_turn("england", 81).unwrap();

        assert_eq!(first, second);
    }

    #[test]
    fn ai_uses_recruitment_rules_and_never_spends_below_zero() {
        let mut campaign = new_campaign();
        campaign.end_turn().unwrap();
        let treasury_before = campaign.faction("france").unwrap().treasury;

        campaign.play_ai_turn("england", 13).unwrap();

        let france = campaign.faction("france").unwrap();
        assert!(france.treasury <= treasury_before);
        assert!(
            campaign
                .recruitment_queue
                .iter()
                .any(|order| order.faction_id == "france")
        );
    }

    #[test]
    fn ai_does_not_automate_the_human_faction() {
        let mut campaign = new_campaign();
        let before = campaign.clone();

        campaign.play_ai_turn("england", 19).unwrap();

        assert_eq!(campaign, before);
    }

    #[test]
    fn ai_prefers_a_legal_attack_and_resolves_it_before_ending_turn() {
        let mut campaign = new_campaign();
        campaign.end_turn().unwrap();

        campaign.play_ai_turn("england", 42).unwrap();

        assert_eq!(campaign.active_faction, "england");
        assert!(campaign.pending_battle.is_none());
        assert_eq!(campaign.battle_reports.len(), 1);
        assert_eq!(campaign.battle_reports[0].attacker_faction, "france");
        assert_eq!(campaign.battle_reports[0].target_province, "normandy");
    }

    #[test]
    fn ai_can_reinforce_a_friendly_frontier_when_no_attack_is_adjacent() {
        let mut campaign = new_campaign();
        campaign.active_faction = "france".into();
        campaign.armies.retain(|army| army.owner != "england");
        let france = campaign
            .armies
            .iter_mut()
            .find(|army| army.id == "france-main")
            .unwrap();
        france.province = "flanders".into();
        for province in &mut campaign.provinces {
            province.owner = match province.id.as_str() {
                "wessex" | "normandy" => "england".into(),
                _ => "france".into(),
            };
        }

        campaign.play_ai_turn("england", 7).unwrap();

        let france = campaign
            .armies
            .iter()
            .find(|army| army.id == "france-main")
            .unwrap();
        assert_eq!(france.province, "paris");
        assert!(campaign.battle_reports.is_empty());
        assert_eq!(campaign.active_faction, "england");
    }

    #[test]
    fn ai_with_no_treasury_still_moves_legally_without_recruiting() {
        let mut campaign = new_campaign();
        campaign.end_turn().unwrap();
        campaign.factions[1].treasury = 0;

        campaign.play_ai_turn("england", 101).unwrap();

        assert!(
            campaign
                .recruitment_queue
                .iter()
                .all(|order| order.faction_id != "france")
        );
        assert!(campaign.pending_battle.is_none());
        assert_eq!(campaign.active_faction, "england");
    }
}
