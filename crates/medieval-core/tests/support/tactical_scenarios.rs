use medieval_core::{
    BattlePoint, BattleSide, BattlefieldLocation, FlatBattlefield, Formation, TacticalBattle,
    TacticalUnit, UnitCombatProfile, UnitKind,
};

#[derive(Clone, Copy, Debug)]
pub enum Scenario {
    Small,
    Medium,
    Large,
    DenseMelee,
    RangedHeavy,
    TerrainHeavy,
    Siege,
    Idle,
}

pub const SCENARIOS: [Scenario; 8] = [
    Scenario::Small,
    Scenario::Medium,
    Scenario::Large,
    Scenario::DenseMelee,
    Scenario::RangedHeavy,
    Scenario::TerrainHeavy,
    Scenario::Siege,
    Scenario::Idle,
];

impl Scenario {
    pub fn name(self) -> &'static str {
        match self {
            Self::Small => "small",
            Self::Medium => "medium",
            Self::Large => "large",
            Self::DenseMelee => "dense-melee",
            Self::RangedHeavy => "ranged-heavy",
            Self::TerrainHeavy => "terrain-heavy",
            Self::Siege => "siege",
            Self::Idle => "idle",
        }
    }

    pub fn ticks(self) -> u32 {
        match self {
            Self::DenseMelee | Self::RangedHeavy => 120,
            Self::Idle => 40,
            _ => 600,
        }
    }

    pub fn build(self) -> TacticalBattle {
        let count = match self {
            Self::Small => 1,
            Self::Medium => 6,
            Self::Large => 32,
            Self::DenseMelee => 16,
            Self::RangedHeavy => 12,
            Self::TerrainHeavy => 8,
            Self::Siege => 4,
            Self::Idle => 16,
        };
        let field = FlatBattlefield::new(100_000, 400_000);
        let location = match self {
            Self::TerrainHeavy => BattlefieldLocation::RiverFord,
            Self::Siege => BattlefieldLocation::MountainPass,
            _ => BattlefieldLocation::ForestClearing,
        };
        let mut units = Vec::new();
        for index in 0..count {
            for side in [BattleSide::Attacker, BattleSide::Defender] {
                let attacking = side == BattleSide::Attacker;
                let kind = if matches!(self, Self::RangedHeavy) {
                    UnitKind::Archers
                } else if matches!(self, Self::TerrainHeavy) && index % 3 == 0 {
                    UnitKind::Knights
                } else {
                    UnitKind::Levy
                };
                let (left, right) = match self {
                    Self::Small | Self::DenseMelee => (49_500, 50_500),
                    Self::RangedHeavy => (38_000, 60_000),
                    Self::Siege => (20_000, 90_000),
                    _ => (20_000, 80_000),
                };
                let soldiers = if matches!(self, Self::Small) {
                    if attacking { 240 } else { 80 }
                } else {
                    120
                };
                let y = if matches!(self, Self::TerrainHeavy) {
                    25_000 + index * 50_000
                } else if matches!(self, Self::Siege) {
                    170_000 + index * 20_000
                } else {
                    5_000 + index * 12_000
                };
                units.push(
                    TacticalUnit::new(
                        format!("{}-{index:02}", if attacking { "a" } else { "d" }),
                        side,
                        soldiers,
                        BattlePoint::new(if attacking { left } else { right }, y),
                        Formation::Line { files: 24 },
                        100,
                    )
                    .with_combat_stats(UnitCombatProfile::v1(kind)),
                );
            }
        }
        let mut battle = if matches!(self, Self::Siege) {
            TacticalBattle::deploy_siege_at_location(field, units, location).unwrap()
        } else {
            TacticalBattle::new_at_location(field, units, location).unwrap()
        }
        .start();
        if matches!(self, Self::Siege) {
            battle.open_siege_gate().unwrap();
        }
        if !matches!(self, Self::Idle) {
            for index in 0..count {
                let attacker = format!("a-{index:02}");
                let defender = format!("d-{index:02}");
                battle.issue_engagement_order(&attacker, &defender).unwrap();
                battle.issue_engagement_order(&defender, &attacker).unwrap();
            }
        }
        battle
    }
}
