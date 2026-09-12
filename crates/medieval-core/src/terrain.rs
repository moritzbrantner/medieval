use serde::{Deserialize, Serialize};

use crate::{BattlePoint, FlatBattlefield};

pub const TACTICAL_TERRAIN_GRID_SIZE: u32 = 8;
pub const TACTICAL_FOREST_CELL_COUNT: usize = 6;
pub const TACTICAL_RIVER_CELL_COUNT: usize = TACTICAL_TERRAIN_GRID_SIZE as usize;
pub const TACTICAL_RIVER_CROSSING_CELL_COUNT: usize = 2;
const TERRAIN_MAX_HEIGHT_DIVISOR: u32 = 25;
const FOREST_MOVEMENT_SPEED_DIVISOR: u32 = 2;
const RIVER_CELL_X: u32 = 3;

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TacticalTerrainProfile {
    #[default]
    HeightFoundationV1,
    ForestMovementV2,
    RiverCrossingsV3,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TacticalGroundCover {
    Open,
    Forest,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TacticalTerrainCell {
    pub cell_x: u32,
    pub cell_z: u32,
}

const TACTICAL_FOREST_CELLS: [TacticalTerrainCell; TACTICAL_FOREST_CELL_COUNT] = [
    TacticalTerrainCell {
        cell_x: 2,
        cell_z: 1,
    },
    TacticalTerrainCell {
        cell_x: 3,
        cell_z: 1,
    },
    TacticalTerrainCell {
        cell_x: 3,
        cell_z: 2,
    },
    TacticalTerrainCell {
        cell_x: 4,
        cell_z: 5,
    },
    TacticalTerrainCell {
        cell_x: 4,
        cell_z: 6,
    },
    TacticalTerrainCell {
        cell_x: 5,
        cell_z: 6,
    },
];

const TACTICAL_RIVER_CELLS: [TacticalTerrainCell; TACTICAL_RIVER_CELL_COUNT] = [
    TacticalTerrainCell {
        cell_x: RIVER_CELL_X,
        cell_z: 0,
    },
    TacticalTerrainCell {
        cell_x: RIVER_CELL_X,
        cell_z: 1,
    },
    TacticalTerrainCell {
        cell_x: RIVER_CELL_X,
        cell_z: 2,
    },
    TacticalTerrainCell {
        cell_x: RIVER_CELL_X,
        cell_z: 3,
    },
    TacticalTerrainCell {
        cell_x: RIVER_CELL_X,
        cell_z: 4,
    },
    TacticalTerrainCell {
        cell_x: RIVER_CELL_X,
        cell_z: 5,
    },
    TacticalTerrainCell {
        cell_x: RIVER_CELL_X,
        cell_z: 6,
    },
    TacticalTerrainCell {
        cell_x: RIVER_CELL_X,
        cell_z: 7,
    },
];

const TACTICAL_RIVER_CROSSING_CELLS: [TacticalTerrainCell;
    TACTICAL_RIVER_CROSSING_CELL_COUNT] = [
    TacticalTerrainCell {
        cell_x: RIVER_CELL_X,
        cell_z: 3,
    },
    TacticalTerrainCell {
        cell_x: RIVER_CELL_X,
        cell_z: 4,
    },
];

/// Deterministic tactical terrain authority.
///
/// Terrain versioning preserves historical replay semantics. The current
/// profile owns elevation, forests, river placement, crossing legality, and the
/// first deterministic chokepoint-routing rule. Renderers may project this
/// contract but must not reproduce terrain generation, passability, or pathing.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TacticalTerrain {
    profile: TacticalTerrainProfile,
}

impl TacticalTerrain {
    #[must_use]
    pub const fn battlefield_foundation() -> Self {
        Self {
            profile: TacticalTerrainProfile::RiverCrossingsV3,
        }
    }

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
    pub const fn forest_cells(self) -> &'static [TacticalTerrainCell] {
        match self.profile {
            TacticalTerrainProfile::HeightFoundationV1 => &[],
            TacticalTerrainProfile::ForestMovementV2 | TacticalTerrainProfile::RiverCrossingsV3 => {
                &TACTICAL_FOREST_CELLS
            }
        }
    }

    #[must_use]
    pub const fn river_cells(self) -> &'static [TacticalTerrainCell] {
        match self.profile {
            TacticalTerrainProfile::RiverCrossingsV3 => &TACTICAL_RIVER_CELLS,
            TacticalTerrainProfile::HeightFoundationV1 | TacticalTerrainProfile::ForestMovementV2 => {
                &[]
            }
        }
    }

    #[must_use]
    pub const fn river_crossing_cells(self) -> &'static [TacticalTerrainCell] {
        match self.profile {
            TacticalTerrainProfile::RiverCrossingsV3 => &TACTICAL_RIVER_CROSSING_CELLS,
            TacticalTerrainProfile::HeightFoundationV1 | TacticalTerrainProfile::ForestMovementV2 => {
                &[]
            }
        }
    }

    #[must_use]
    pub fn ground_cover_at(
        self,
        battlefield: FlatBattlefield,
        point: BattlePoint,
    ) -> TacticalGroundCover {
        self.cell_ground_cover(
            terrain_cell_index(point.x_mm, battlefield.width_mm),
            terrain_cell_index(point.y_mm, battlefield.depth_mm),
        )
    }

    #[must_use]
    pub fn cell_ground_cover(self, cell_x: u32, cell_z: u32) -> TacticalGroundCover {
        if self
            .forest_cells()
            .contains(&TacticalTerrainCell { cell_x, cell_z })
        {
            TacticalGroundCover::Forest
        } else {
            TacticalGroundCover::Open
        }
    }

    #[must_use]
    pub fn is_passable_at(self, battlefield: FlatBattlefield, point: BattlePoint) -> bool {
        self.cell_is_passable(
            terrain_cell_index(point.x_mm, battlefield.width_mm),
            terrain_cell_index(point.y_mm, battlefield.depth_mm),
        )
    }

    #[must_use]
    pub fn cell_is_passable(self, cell_x: u32, cell_z: u32) -> bool {
        let cell = TacticalTerrainCell { cell_x, cell_z };
        !self.river_cells().contains(&cell) || self.river_crossing_cells().contains(&cell)
    }

    /// Returns the deterministic intermediate target for one movement leg.
    ///
    /// The current river profile uses one explicit river column with two
    /// adjacent crossing cells. Units whose final target lies across the river
    /// first align with the cheapest crossing while remaining on their own bank,
    /// then enter the crossing, and finally continue toward the original target.
    /// This keeps pathing in authoritative integer simulation state without
    /// introducing a renderer/navmesh dependency.
    #[must_use]
    pub fn movement_waypoint(
        self,
        battlefield: FlatBattlefield,
        from: BattlePoint,
        destination: BattlePoint,
    ) -> BattlePoint {
        if self.profile != TacticalTerrainProfile::RiverCrossingsV3 {
            return destination;
        }

        let from_cell = TacticalTerrainCell {
            cell_x: terrain_cell_index(from.x_mm, battlefield.width_mm),
            cell_z: terrain_cell_index(from.y_mm, battlefield.depth_mm),
        };
        let destination_cell = TacticalTerrainCell {
            cell_x: terrain_cell_index(destination.x_mm, battlefield.width_mm),
            cell_z: terrain_cell_index(destination.y_mm, battlefield.depth_mm),
        };

        if !self.cell_is_passable(destination_cell.cell_x, destination_cell.cell_z) {
            return self.bank_waypoint_for_crossing(battlefield, from, destination);
        }

        if from_cell.cell_x == RIVER_CELL_X {
            return destination;
        }

        let opposite_banks = (from_cell.cell_x < RIVER_CELL_X
            && destination_cell.cell_x > RIVER_CELL_X)
            || (from_cell.cell_x > RIVER_CELL_X
                && destination_cell.cell_x < RIVER_CELL_X);
        if !opposite_banks {
            return destination;
        }

        self.bank_waypoint_for_crossing(battlefield, from, destination)
    }

    fn bank_waypoint_for_crossing(
        self,
        battlefield: FlatBattlefield,
        from: BattlePoint,
        destination: BattlePoint,
    ) -> BattlePoint {
        let crossing = self
            .river_crossing_cells()
            .iter()
            .copied()
            .min_by_key(|cell| {
                let (_, _, z0, z1) = self
                    .cell_bounds_mm(battlefield, cell.cell_x, cell.cell_z)
                    .expect("configured crossing cell is inside the terrain grid");
                let center_z = z0 + (z1 - z0) / 2;
                (
                    u64::from(from.y_mm.abs_diff(center_z))
                        + u64::from(destination.y_mm.abs_diff(center_z)),
                    cell.cell_z,
                )
            })
            .expect("river profile always has a crossing");
        let (x0, x1, z0, z1) = self
            .cell_bounds_mm(battlefield, crossing.cell_x, crossing.cell_z)
            .expect("configured crossing cell is inside the terrain grid");
        let center_x = x0 + (x1 - x0) / 2;
        let center_z = z0 + (z1 - z0) / 2;

        if from.y_mm < z0 || from.y_mm >= z1 {
            BattlePoint::new(from.x_mm, center_z)
        } else {
            BattlePoint::new(center_x, center_z)
        }
    }

    /// Returns the movement budget for one tactical tick.
    ///
    /// Forest movement is half the base budget, rounded up so a valid unit
    /// always retains forward progress. The cover is sampled at the unit's
    /// authoritative position at the start of the tick.
    #[must_use]
    pub fn movement_speed_mm_per_tick(
        self,
        battlefield: FlatBattlefield,
        position: BattlePoint,
        base_speed_mm_per_tick: u32,
    ) -> u32 {
        match self.ground_cover_at(battlefield, position) {
            TacticalGroundCover::Open => base_speed_mm_per_tick,
            TacticalGroundCover::Forest => {
                base_speed_mm_per_tick / FOREST_MOVEMENT_SPEED_DIVISOR
                    + base_speed_mm_per_tick % FOREST_MOVEMENT_SPEED_DIVISOR
            }
        }
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
    fn battlefield_foundation_is_deterministic_and_serializable() {
        let terrain = TacticalTerrain::battlefield_foundation();
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
        assert_eq!(decoded.forest_cells(), terrain.forest_cells());
        assert_eq!(decoded.river_cells(), terrain.river_cells());
        assert_eq!(decoded.profile(), TacticalTerrainProfile::RiverCrossingsV3);
    }

    #[test]
    fn height_foundation_v1_remains_height_only_for_legacy_replays() {
        let terrain = TacticalTerrain::height_foundation();
        let battlefield = FlatBattlefield::new(80_000, 80_000);
        let former_forest_point = BattlePoint::new(25_000, 15_000);
        assert_eq!(
            terrain.profile(),
            TacticalTerrainProfile::HeightFoundationV1
        );
        assert!(terrain.forest_cells().is_empty());
        assert!(terrain.river_cells().is_empty());
        assert_eq!(
            terrain.ground_cover_at(battlefield, former_forest_point),
            TacticalGroundCover::Open
        );
        assert_eq!(
            terrain.movement_speed_mm_per_tick(battlefield, former_forest_point, 1_201),
            1_201
        );
    }

    #[test]
    fn forest_movement_v2_remains_river_free_for_legacy_replays() {
        let terrain: TacticalTerrain =
            serde_json::from_str(r#"{"profile":"forestMovementV2"}"#).unwrap();
        assert_eq!(terrain.profile(), TacticalTerrainProfile::ForestMovementV2);
        assert_eq!(terrain.forest_cells(), &TACTICAL_FOREST_CELLS);
        assert!(terrain.river_cells().is_empty());
        assert!(terrain.river_crossing_cells().is_empty());
    }

    #[test]
    fn height_lookup_uses_the_same_floored_boundaries_as_cell_geometry() {
        let terrain = TacticalTerrain::battlefield_foundation();
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
        let terrain = TacticalTerrain::battlefield_foundation();
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
        let terrain = TacticalTerrain::battlefield_foundation();
        let battlefield = FlatBattlefield::new(100_000, 80_000);
        assert_eq!(terrain.height_mm(battlefield, BattlePoint::new(0, 0)), 0);
        assert!(terrain.height_mm(battlefield, BattlePoint::new(50_000, 40_000)) > 0);
    }

    #[test]
    fn forest_ground_cover_uses_the_same_floored_cell_boundaries() {
        let terrain = TacticalTerrain::battlefield_foundation();
        let battlefield = FlatBattlefield::new(100_003, 80_005);
        let (x0, _, z0, _) = terrain.cell_bounds_mm(battlefield, 2, 1).unwrap();
        assert_eq!(
            terrain.ground_cover_at(battlefield, BattlePoint::new(x0, z0)),
            TacticalGroundCover::Forest
        );
        assert_eq!(
            terrain.ground_cover_at(battlefield, BattlePoint::new(x0 - 1, z0)),
            TacticalGroundCover::Open
        );
    }

    #[test]
    fn forest_movement_budget_is_half_speed_rounded_up() {
        let terrain = TacticalTerrain::battlefield_foundation();
        let battlefield = FlatBattlefield::new(80_000, 80_000);
        assert_eq!(
            terrain.movement_speed_mm_per_tick(
                battlefield,
                BattlePoint::new(25_000, 15_000),
                1_201,
            ),
            601
        );
        assert_eq!(
            terrain
                .movement_speed_mm_per_tick(battlefield, BattlePoint::new(5_000, 15_000), 1_201,),
            1_201
        );
    }

    #[test]
    fn river_cells_are_impassable_except_for_explicit_crossings() {
        let terrain = TacticalTerrain::battlefield_foundation();
        assert_eq!(terrain.river_cells().len(), TACTICAL_RIVER_CELL_COUNT);
        assert_eq!(
            terrain.river_crossing_cells().len(),
            TACTICAL_RIVER_CROSSING_CELL_COUNT
        );
        assert!(!terrain.cell_is_passable(RIVER_CELL_X, 1));
        assert!(terrain.cell_is_passable(RIVER_CELL_X, 3));
        assert!(terrain.cell_is_passable(RIVER_CELL_X, 4));
        assert!(terrain.cell_is_passable(RIVER_CELL_X - 1, 1));
    }

    #[test]
    fn opposite_bank_routes_align_then_enter_the_nearest_crossing() {
        let terrain = TacticalTerrain::battlefield_foundation();
        let battlefield = FlatBattlefield::new(80_000, 80_000);
        let from = BattlePoint::new(10_000, 10_000);
        let destination = BattlePoint::new(70_000, 10_000);
        let align = terrain.movement_waypoint(battlefield, from, destination);
        assert_eq!(align, BattlePoint::new(10_000, 35_000));

        let aligned = BattlePoint::new(10_000, 35_000);
        let crossing = terrain.movement_waypoint(battlefield, aligned, destination);
        assert_eq!(crossing, BattlePoint::new(35_000, 35_000));

        let inside_crossing = BattlePoint::new(35_000, 35_000);
        assert_eq!(
            terrain.movement_waypoint(battlefield, inside_crossing, destination),
            destination
        );
    }

    #[test]
    fn blocked_river_target_routes_toward_a_crossing_instead_of_water() {
        let terrain = TacticalTerrain::battlefield_foundation();
        let battlefield = FlatBattlefield::new(80_000, 80_000);
        let from = BattlePoint::new(10_000, 10_000);
        let blocked = BattlePoint::new(35_000, 15_000);
        assert!(!terrain.is_passable_at(battlefield, blocked));
        assert_eq!(
            terrain.movement_waypoint(battlefield, from, blocked),
            BattlePoint::new(10_000, 35_000)
        );
    }
}
