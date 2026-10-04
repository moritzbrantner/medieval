use serde::{Deserialize, Serialize};

use crate::{
    Army, BattlePoint, BattleSide, CampaignError, CampaignState, FlatBattlefield, Formation,
    TacticalBattle, TacticalError, TacticalUnit, UNIT_KINDS, UnitKind,
};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TacticalBattleSeed {
    pub turn: u32,
    pub attacker_army_id: String,
    pub from_province: String,
    pub target_province: String,
    pub attacker: TacticalForceSeed,
    pub defender: TacticalForceSeed,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TacticalForceSeed {
    pub faction_id: String,
    pub source_army_ids: Vec<String>,
    pub units: Vec<TacticalUnitSeed>,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TacticalUnitSeed {
    pub kind: UnitKind,
    pub soldiers: u64,
}

impl TacticalBattle {
    /// Builds core-owned tactical composition and retains its campaign provenance.
    ///
    /// Counts are canonicalized by kind and split into `u16`-sized units without
    /// truncation. Core deployment packs complete formation footprints into legal
    /// zones and rejects overflow. Editable deployment remains a separate concern.
    pub fn from_campaign_seed(
        battlefield: FlatBattlefield,
        seed: TacticalBattleSeed,
    ) -> Result<Self, TacticalError> {
        Self::from_campaign_seed_with_profile(
            battlefield,
            seed,
            crate::TacticalBattlefieldProfile::Field {
                location: crate::BattlefieldLocation::MountainPass,
            },
        )
    }

    pub fn from_campaign_seed_with_profile(
        battlefield: FlatBattlefield,
        mut seed: TacticalBattleSeed,
        profile: crate::TacticalBattlefieldProfile,
    ) -> Result<Self, TacticalError> {
        // Validate dimensions before allocating the force composition.
        Self::new(battlefield, Vec::new())?;
        let mut units = Vec::new();
        for (side, force) in [
            (BattleSide::Attacker, &mut seed.attacker),
            (BattleSide::Defender, &mut seed.defender),
        ] {
            canonicalize_force(force, side)?;
            let count: u64 = force
                .units
                .iter()
                .map(|unit| unit.soldiers.div_ceil(u64::from(u16::MAX)))
                .sum();
            let capacity =
                usize::try_from(count).map_err(|_| TacticalError::CampaignForceTooLarge(side))?;
            units
                .try_reserve(capacity)
                .map_err(|_| TacticalError::CampaignForceTooLarge(side))?;
            for entry in &force.units {
                let kind_id = match entry.kind {
                    UnitKind::Levy => "levy",
                    UnitKind::Spearmen => "spearmen",
                    UnitKind::Archers => "archers",
                    UnitKind::Knights => "knights",
                };
                let side_id = match side {
                    BattleSide::Attacker => "attacker",
                    BattleSide::Defender => "defender",
                };
                let mut remaining = entry.soldiers;
                let mut chunk = 0_u64;
                while remaining > 0 {
                    let soldiers = remaining.min(u64::from(u16::MAX)) as u16;
                    units.push(
                        TacticalUnit::new(
                            format!("campaign-{side_id}-{kind_id}-{chunk:020}"),
                            side,
                            soldiers,
                            BattlePoint::new(0, 0),
                            Formation::Line {
                                files: soldiers.min(10),
                            },
                            100,
                        )
                        .with_unit_kind(entry.kind),
                    );
                    remaining -= u64::from(soldiers);
                    chunk += 1;
                }
            }
        }
        crate::campaign_deployment::place_campaign_units(battlefield, profile, &mut units)?;
        let mut battle = match profile {
            crate::TacticalBattlefieldProfile::Field { location } => {
                Self::deploy_at_location(battlefield, units, location)
            }
            crate::TacticalBattlefieldProfile::Siege { location } => {
                Self::deploy_siege_at_location(battlefield, units, location)
            }
        }?;
        battle.campaign_seed = Some(seed);
        Ok(battle.start())
    }
}

fn canonicalize_force(
    force: &mut TacticalForceSeed,
    side: BattleSide,
) -> Result<(), TacticalError> {
    let mut units = Vec::new();
    for kind in UNIT_KINDS {
        let soldiers = force
            .units
            .iter()
            .filter(|unit| unit.kind == kind)
            .try_fold(0_u64, |count, unit| count.checked_add(unit.soldiers))
            .ok_or(TacticalError::CampaignForceTooLarge(side))?;
        if soldiers > 0 {
            units.push(TacticalUnitSeed { kind, soldiers });
        }
    }
    force.units = units;
    force.source_army_ids.sort();
    Ok(())
}

impl CampaignState {
    /// Projects the currently pending strategic battle into a deterministic,
    /// serialization-friendly tactical composition seed.
    ///
    /// This is deliberately not a deployed `TacticalBattle` yet. Deployment
    /// geometry, formation layout, and tactical outcomes remain separate
    /// follow-up boundaries so campaign truth is not silently coupled to one
    /// renderer or battlefield presentation.
    pub fn pending_tactical_battle_seed(&self) -> Result<TacticalBattleSeed, CampaignError> {
        let pending = self
            .pending_battle
            .as_ref()
            .ok_or(CampaignError::NoPendingBattle)?;
        let attacker_army = self
            .armies
            .iter()
            .find(|army| army.id == pending.attacker_army_id)
            .ok_or_else(|| CampaignError::ArmyNotFound(pending.attacker_army_id.clone()))?;

        let attacker = force_seed(&pending.attacker_faction, std::iter::once(attacker_army));
        let defender = force_seed(
            &pending.defender_faction,
            self.armies.iter().filter(|army| {
                army.owner == pending.defender_faction && army.province == pending.target_province
            }),
        );

        Ok(TacticalBattleSeed {
            turn: self.turn,
            attacker_army_id: pending.attacker_army_id.clone(),
            from_province: pending.from_province.clone(),
            target_province: pending.target_province.clone(),
            attacker,
            defender,
        })
    }
}

fn force_seed<'a>(faction_id: &str, armies: impl Iterator<Item = &'a Army>) -> TacticalForceSeed {
    let mut source_army_ids = Vec::new();
    let mut counts = [0_u64; 4];

    for army in armies {
        source_army_ids.push(army.id.clone());
        counts[0] = counts[0].saturating_add(u64::from(army.levy));
        counts[1] = counts[1].saturating_add(u64::from(army.spearmen));
        counts[2] = counts[2].saturating_add(u64::from(army.archers));
        counts[3] = counts[3].saturating_add(u64::from(army.knights));
    }

    source_army_ids.sort();
    let units = [
        (UnitKind::Levy, counts[0]),
        (UnitKind::Spearmen, counts[1]),
        (UnitKind::Archers, counts[2]),
        (UnitKind::Knights, counts[3]),
    ]
    .into_iter()
    .filter_map(|(kind, soldiers)| (soldiers > 0).then_some(TacticalUnitSeed { kind, soldiers }))
    .collect();

    TacticalForceSeed {
        faction_id: faction_id.to_owned(),
        source_army_ids,
        units,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Army, new_campaign};

    fn pending_paris_battle() -> CampaignState {
        let mut campaign = new_campaign();
        campaign
            .move_army("england-main", "paris")
            .expect("fixture movement must create a pending battle");
        campaign
    }

    #[test]
    fn pending_battle_seed_preserves_campaign_composition_and_provenance() {
        let campaign = pending_paris_battle();

        let seed = campaign
            .pending_tactical_battle_seed()
            .expect("pending battle must produce a tactical seed");

        assert_eq!(seed.turn, 1);
        assert_eq!(seed.attacker_army_id, "england-main");
        assert_eq!(seed.from_province, "normandy");
        assert_eq!(seed.target_province, "paris");
        assert_eq!(seed.attacker.faction_id, "england");
        assert_eq!(seed.attacker.source_army_ids, ["england-main"]);
        assert_eq!(
            seed.attacker.units,
            [
                TacticalUnitSeed {
                    kind: UnitKind::Levy,
                    soldiers: 120,
                },
                TacticalUnitSeed {
                    kind: UnitKind::Spearmen,
                    soldiers: 80,
                },
                TacticalUnitSeed {
                    kind: UnitKind::Archers,
                    soldiers: 40,
                },
                TacticalUnitSeed {
                    kind: UnitKind::Knights,
                    soldiers: 20,
                },
            ]
        );
        assert_eq!(seed.defender.faction_id, "france");
        assert_eq!(seed.defender.source_army_ids, ["france-main"]);
        assert_eq!(seed.defender.units, seed.attacker.units);
    }

    #[test]
    fn defender_aggregation_is_storage_order_independent_and_canonical() {
        let mut campaign = new_campaign();
        campaign.armies.push(Army {
            id: "france-reserve".into(),
            owner: "france".into(),
            province: "paris".into(),
            levy: 5,
            spearmen: 4,
            archers: 3,
            knights: 2,
            moved_this_turn: false,
        });
        campaign.armies.reverse();
        campaign
            .move_army("england-main", "paris")
            .expect("fixture movement must create a pending battle");

        let seed = campaign
            .pending_tactical_battle_seed()
            .expect("pending battle must produce a tactical seed");

        assert_eq!(
            seed.defender.source_army_ids,
            ["france-main", "france-reserve"]
        );
        assert_eq!(
            seed.defender.units,
            [
                TacticalUnitSeed {
                    kind: UnitKind::Levy,
                    soldiers: 125,
                },
                TacticalUnitSeed {
                    kind: UnitKind::Spearmen,
                    soldiers: 84,
                },
                TacticalUnitSeed {
                    kind: UnitKind::Archers,
                    soldiers: 43,
                },
                TacticalUnitSeed {
                    kind: UnitKind::Knights,
                    soldiers: 22,
                },
            ]
        );
    }

    #[test]
    fn zero_strength_unit_kinds_are_omitted_without_reordering_remaining_kinds() {
        let mut campaign = pending_paris_battle();
        let defender = campaign
            .armies
            .iter_mut()
            .find(|army| army.id == "france-main")
            .expect("fixture defender must exist");
        defender.levy = 0;
        defender.archers = 0;

        let seed = campaign
            .pending_tactical_battle_seed()
            .expect("pending battle must produce a tactical seed");

        assert_eq!(
            seed.defender.units,
            [
                TacticalUnitSeed {
                    kind: UnitKind::Spearmen,
                    soldiers: 80,
                },
                TacticalUnitSeed {
                    kind: UnitKind::Knights,
                    soldiers: 20,
                },
            ]
        );
    }

    #[test]
    fn tactical_seed_round_trips_without_campaign_or_renderer_state() {
        let seed = pending_paris_battle()
            .pending_tactical_battle_seed()
            .expect("pending battle must produce a tactical seed");

        let encoded = serde_json::to_string(&seed).expect("seed must serialize");
        let decoded: TacticalBattleSeed =
            serde_json::from_str(&encoded).expect("seed must deserialize");

        assert_eq!(decoded, seed);
    }

    #[test]
    fn tactical_seed_requires_a_pending_campaign_battle() {
        assert_eq!(
            new_campaign().pending_tactical_battle_seed(),
            Err(CampaignError::NoPendingBattle)
        );
    }
}
