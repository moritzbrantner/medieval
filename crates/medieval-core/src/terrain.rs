use serde::{Deserialize, Serialize};

use crate::{BattlePoint, FlatBattlefield};

pub const TACTICAL_TERRAIN_GRID_SIZE: u32 = 8;
const TERRAIN_MAX_HEIGHT_DIVISOR: u32 = 25;

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TacticalTerrainProfile {
    #[default]
    HeightFoundationV1,
}

/// Deterministic tactical terrain authority.
///
/// The current profile owns elevation data/query semantics in `medieval-core`
/// while movement, combat, morale, and legality deliberately remain terrain-neutral.
/// Renderers may project this contract but must not reproduce its generation rules.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TacticalTerrain {
    profile: TacticalTerrainProfile,
}

impl TacticalTerrain {
    #[must_use]
    pub const fn height_foundation() -> Self {
        Self {
            profile: TacticalTerrainProfile::HeightFoundationV1,
        }
    }

    #[must_use]
    pub const fn profile(self) -> TacticalTerrainProfile {
        self.profile
    }

    #[must_use]
    pub fn height_mm(self, battlefield: FlatBattlefield, point: BattlePoint) -> u32 {
        if battlefield.width_mm == 0 || battlefield.depth_mm == 0 {
            return 0;
        }
        self.cell_height_mm(
            battlefield,
            terrain_cell_index(point.x_mm, battlefield.width_mm),
            terrain_cell_index(point.y_mm, battlefield.depth_mm),
        )
    }

    #[must_use]
    pub fn cell_height_mm(self, battlefield: FlatBattlefield, cell_x: u32, cell_z: u32) -> u32 {
        let _profile = self.profile;
        let cell_x = cell_x.min(TACTICAL_TERRAIN_GRID_SIZE - 1);
        let cell_z = cell_z.min(TACTICAL_TERRAIN_GRID_SIZE - 1);
        let sample_x = cell_x * 2 + 1;
        let sample_z = cell_z * 2 + 1;
        let center = TACTICAL_TERRAIN_GRID_SIZE;
        let distance = sample_x.abs_diff(center).max(sample_z.abs_diff(center));
        let max_distance = TACTICAL_TERRAIN_GRID_SIZE - 1;
        let elevation_scale = max_distance.saturating_sub(distance);
        let peak_scale = TACTICAL_TERRAIN_GRID_SIZE.saturating_sub(2).max(1);
        let max_height =
            battlefield.width_mm.min(battlefield.depth_mm) / TERRAIN_MAX_HEIGHT_DIVISOR;
        max_height.saturating_mul(elevation_scale) / peak_scale
    }

    #[must_use]
    pub fn cell_bounds_mm(
        self,
        battlefield: FlatBattlefield,
        cell_x: u32,
        cell_z: u32,
    ) -> Option<(u32, u32, u32, u32)> {
        let _profile = self.profile;
        if battlefield.width_mm == 0
            || battlefield.depth_mm == 0
            || cell_x >= TACTICAL_TERRAIN_GRID_SIZE
            || cell_z >= TACTICAL_TERRAIN_GRID_SIZE
        {
            return None;
        }
        let x0 = scaled_boundary(battlefield.width_mm, cell_x);
        let x1 = scaled_boundary(battlefield.width_mm, cell_x + 1);
        let z0 = scaled_boundary(battlefield.depth_mm, cell_z);
        let z1 = scaled_boundary(battlefield.depth_mm, cell_z + 1);
        Some((x0, x1, z0, z1))
    }
}

fn terrain_cell_index(coordinate_mm: u32, span_mm: u32) -> u32 {
    if span_mm == 0 {
        return 0;
    }
    let coordinate_mm = coordinate_mm.min(span_mm);
    (1..TACTICAL_TERRAIN_GRID_SIZE)
        .rev()
        .find(|&cell| coordinate_mm >= scaled_boundary(span_mm, cell))
        .unwrap_or(0)
}

fn scaled_boundary(span_mm: u32, index: u32) -> u32 {
    ((u64::from(span_mm) * u64::from(index)) / u64::from(TACTICAL_TERRAIN_GRID_SIZE)) as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn height_foundation_is_deterministic_and_serializable() {
        let terrain = TacticalTerrain::height_foundation();
        let battlefield = FlatBattlefield::new(100_000, 80_000);
        let point = BattlePoint::new(50_000, 40_000);
        let first = terrain.height_mm(battlefield, point);
        let second = terrain.height_mm(battlefield, point);
        assert_eq!(first, second);
        assert!(first > 0);

        let encoded = serde_json::to_string(&terrain).unwrap();
        let decoded: TacticalTerrain = serde_json::from_str(&encoded).unwrap();
        assert_eq!(decoded, terrain);
        assert_eq!(decoded.height_mm(battlefield, point), first);
    }

    #[test]
    fn height_lookup_uses_the_same_floored_boundaries_as_cell_geometry() {
        let terrain = TacticalTerrain::height_foundation();
        let battlefield = FlatBattlefield::new(100_003, 80_005);
        let boundary_x = scaled_boundary(battlefield.width_mm, 1);
        let boundary_z = scaled_boundary(battlefield.depth_mm, 4);
        assert_eq!(boundary_x, 12_500);
        assert_eq!(terrain_cell_index(boundary_x - 1, battlefield.width_mm), 0);
        assert_eq!(terrain_cell_index(boundary_x, battlefield.width_mm), 1);
        assert_eq!(
            terrain.height_mm(battlefield, BattlePoint::new(boundary_x, boundary_z)),
            terrain.cell_height_mm(battlefield, 1, 4)
        );
        assert_eq!(
            terrain_cell_index(battlefield.width_mm, battlefield.width_mm),
            TACTICAL_TERRAIN_GRID_SIZE - 1
        );
    }

    #[test]
    fn cell_bounds_cover_the_exact_battlefield_extent() {
        let terrain = TacticalTerrain::height_foundation();
        let battlefield = FlatBattlefield::new(100_003, 80_005);
        assert_eq!(
            terrain.cell_bounds_mm(battlefield, 0, 0),
            Some((0, 12_500, 0, 10_000))
        );
        assert_eq!(
            terrain.cell_bounds_mm(battlefield, 7, 7),
            Some((87_502, 100_003, 70_004, 80_005))
        );
    }

    #[test]
    fn terrain_is_flat_at_the_outer_edge_and_elevated_in_the_center() {
        let terrain = TacticalTerrain::height_foundation();
        let battlefield = FlatBattlefield::new(100_000, 80_000);
        assert_eq!(terrain.height_mm(battlefield, BattlePoint::new(0, 0)), 0);
        assert!(terrain.height_mm(battlefield, BattlePoint::new(50_000, 40_000)) > 0);
    }
}
