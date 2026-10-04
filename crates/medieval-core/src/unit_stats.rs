use serde::{Deserialize, Serialize};

use crate::UnitKind;

/// A serialized rules revision. Missing profiles retain historical tactical rules.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum UnitStatsVersion {
    UnitStatsV1,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UnitCombatProfile {
    pub version: UnitStatsVersion,
    pub kind: UnitKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MissileStats {
    pub damage_milli: u16,
    pub range_mm: u32,
    pub ammunition: u16,
}

/// Combat factors use 1,000 as their integer base; distances are millimeters.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UnitStats {
    pub melee_attack_milli: u16,
    pub defense_milli: u16,
    pub armor_milli: u16,
    pub initial_morale: u16,
    pub movement_mm_per_tick: u32,
    pub charge_impact_milli: u16,
    pub missile: Option<MissileStats>,
    pub formation_resistance_milli: u16,
}

impl UnitCombatProfile {
    #[must_use]
    pub const fn v1(kind: UnitKind) -> Self {
        Self {
            version: UnitStatsVersion::UnitStatsV1,
            kind,
        }
    }

    #[must_use]
    pub const fn stats(self) -> UnitStats {
        match self.version {
            UnitStatsVersion::UnitStatsV1 => match self.kind {
                UnitKind::Levy => UnitStats {
                    melee_attack_milli: 1_000,
                    defense_milli: 750,
                    armor_milli: 100,
                    initial_morale: 800,
                    movement_mm_per_tick: 100,
                    charge_impact_milli: 0,
                    missile: None,
                    formation_resistance_milli: 800,
                },
                UnitKind::Spearmen => UnitStats {
                    melee_attack_milli: 1_250,
                    defense_milli: 1_100,
                    armor_milli: 250,
                    initial_morale: 1_000,
                    movement_mm_per_tick: 90,
                    charge_impact_milli: 0,
                    missile: None,
                    formation_resistance_milli: 1_400,
                },
                UnitKind::Archers => UnitStats {
                    melee_attack_milli: 600,
                    defense_milli: 700,
                    armor_milli: 100,
                    initial_morale: 900,
                    movement_mm_per_tick: 100,
                    charge_impact_milli: 0,
                    missile: Some(MissileStats {
                        damage_milli: 1_000,
                        range_mm: 25_000,
                        ammunition: 30,
                    }),
                    formation_resistance_milli: 600,
                },
                UnitKind::Knights => UnitStats {
                    melee_attack_milli: 1_500,
                    defense_milli: 1_200,
                    armor_milli: 600,
                    initial_morale: 1_000,
                    movement_mm_per_tick: 175,
                    charge_impact_milli: 400,
                    missile: None,
                    formation_resistance_milli: 1_000,
                },
            },
        }
    }
}
