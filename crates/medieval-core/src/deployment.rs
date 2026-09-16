use serde::{Deserialize, Serialize};

use crate::tactical::{BattlePoint, BattleSide, FlatBattlefield};

const DEPLOYMENT_ZONE_DIVISOR: u32 = 3;

#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeploymentZone {
    pub side: BattleSide,
    pub min_x_mm: u32,
    pub max_x_mm: u32,
    pub min_y_mm: u32,
    pub max_y_mm: u32,
}

impl DeploymentZone {
    #[must_use]
    pub const fn contains(self, point: BattlePoint) -> bool {
        point.x_mm >= self.min_x_mm
            && point.x_mm <= self.max_x_mm
            && point.y_mm >= self.min_y_mm
            && point.y_mm <= self.max_y_mm
    }
}

#[must_use]
pub const fn standard_deployment_zone(
    battlefield: FlatBattlefield,
    side: BattleSide,
) -> DeploymentZone {
    let zone_width = battlefield.width_mm / DEPLOYMENT_ZONE_DIVISOR;
    let (min_x_mm, max_x_mm) = match side {
        BattleSide::Attacker => (0, zone_width),
        BattleSide::Defender => (battlefield.width_mm - zone_width, battlefield.width_mm),
    };
    DeploymentZone {
        side,
        min_x_mm,
        max_x_mm,
        min_y_mm: 0,
        max_y_mm: battlefield.depth_mm,
    }
}

#[must_use]
pub const fn standard_deployment_zones(battlefield: FlatBattlefield) -> [DeploymentZone; 2] {
    [
        standard_deployment_zone(battlefield, BattleSide::Attacker),
        standard_deployment_zone(battlefield, BattleSide::Defender),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standard_zones_are_deterministic_disjoint_and_cover_each_back_third() {
        let battlefield = FlatBattlefield::new(100_003, 80_005);
        let [attacker, defender] = standard_deployment_zones(battlefield);
        assert_eq!(attacker.min_x_mm, 0);
        assert_eq!(attacker.max_x_mm, 33_334);
        assert_eq!(defender.min_x_mm, 66_669);
        assert_eq!(defender.max_x_mm, 100_003);
        assert_eq!(attacker.max_y_mm, battlefield.depth_mm);
        assert_eq!(defender.max_y_mm, battlefield.depth_mm);
        assert!(attacker.max_x_mm < defender.min_x_mm);
    }

    #[test]
    fn zone_boundaries_are_inclusive_but_the_neutral_center_is_not_deployable() {
        let battlefield = FlatBattlefield::new(90_000, 60_000);
        let attacker = standard_deployment_zone(battlefield, BattleSide::Attacker);
        let defender = standard_deployment_zone(battlefield, BattleSide::Defender);
        assert!(attacker.contains(BattlePoint::new(30_000, 60_000)));
        assert!(defender.contains(BattlePoint::new(60_000, 0)));
        assert!(!attacker.contains(BattlePoint::new(45_000, 30_000)));
        assert!(!defender.contains(BattlePoint::new(45_000, 30_000)));
    }
}
