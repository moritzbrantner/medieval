from pathlib import Path


def replace_once(path: str, old: str, new: str) -> None:
    file = Path(path)
    text = file.read_text()
    if old not in text:
        raise SystemExit(f"missing expected block in {path}: {old[:120]!r}")
    file.write_text(text.replace(old, new, 1))


Path("crates/medieval-core/src/terrain.rs").write_text(r'''use serde::{Deserialize, Serialize};

use crate::{BattlePoint, FlatBattlefield};

pub const TACTICAL_TERRAIN_GRID_SIZE: u32 = 8;
pub const TACTICAL_FOREST_CELL_COUNT: usize = 6;
const TERRAIN_MAX_HEIGHT_DIVISOR: u32 = 25;
const FOREST_MOVEMENT_SPEED_DIVISOR: u32 = 2;

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TacticalTerrainProfile {
    #[default]
    HeightFoundationV1,
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

/// Deterministic tactical terrain authority.
///
/// The current profile owns elevation, cell boundaries, and ground-cover query
/// semantics in `medieval-core`. Forest cover is the first gameplay modifier:
/// movement starting a tick in forest is reduced deterministically. Renderers
/// may project this contract but must not reproduce its generation or effects.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TacticalTerrain {
    profile: TacticalTerrainProfile,
}

impl TacticalTerrain {
    #[must_use]
    pub const fn battlefield_foundation() -> Self {
        Self {
            profile: TacticalTerrainProfile::HeightFoundationV1,
        }
    }

    #[must_use]
    pub const fn height_foundation() -> Self {
        Self::battlefield_foundation()
    }

    #[must_use]
    pub const fn profile(self) -> TacticalTerrainProfile {
        self.profile
    }

    #[must_use]
    pub const fn forest_cells(self) -> [TacticalTerrainCell; TACTICAL_FOREST_CELL_COUNT] {
        let _profile = self.profile;
        TACTICAL_FOREST_CELLS
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
        let _profile = self.profile;
        if TACTICAL_FOREST_CELLS.contains(&TacticalTerrainCell { cell_x, cell_z }) {
            TacticalGroundCover::Forest
        } else {
            TacticalGroundCover::Open
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
    fn height_foundation_is_deterministic_and_serializable() {
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
            terrain.movement_speed_mm_per_tick(
                battlefield,
                BattlePoint::new(5_000, 15_000),
                1_201,
            ),
            1_201
        );
    }
}
''')

replace_once(
    "crates/medieval-core/src/lib.rs",
    "pub use terrain::{TACTICAL_TERRAIN_GRID_SIZE, TacticalTerrain, TacticalTerrainProfile};",
    "pub use terrain::{\n    TACTICAL_FOREST_CELL_COUNT, TACTICAL_TERRAIN_GRID_SIZE, TacticalGroundCover, TacticalTerrain,\n    TacticalTerrainCell, TacticalTerrainProfile,\n};",
)

replace_once(
    "crates/medieval-core/src/tactical.rs",
    "use crate::deployment::{DeploymentZone, standard_deployment_zone, standard_deployment_zones};",
    "use crate::deployment::{DeploymentZone, standard_deployment_zone, standard_deployment_zones};\nuse crate::terrain::TacticalTerrain;",
)

replace_once(
    "crates/medieval-core/src/tactical.rs",
    "    pub const fn deployment_zones(&self) -> [DeploymentZone; 2] {\n        standard_deployment_zones(self.battlefield)\n    }\n\n    #[must_use]\n    pub const fn tick(&self) -> u64 {",
    "    pub const fn deployment_zones(&self) -> [DeploymentZone; 2] {\n        standard_deployment_zones(self.battlefield)\n    }\n\n    #[must_use]\n    pub const fn terrain(&self) -> TacticalTerrain {\n        TacticalTerrain::battlefield_foundation()\n    }\n\n    #[must_use]\n    pub const fn tick(&self) -> u64 {",
)

replace_once(
    "crates/medieval-core/src/tactical.rs",
    "        let movement_speed =\n            if target.is_some_and(|target| target.state == TacticalUnitState::Routed) {\n                unit.speed_mm_per_tick.saturating_mul(2)\n            } else {\n                unit.speed_mm_per_tick\n            };\n        let next = move_point_toward(unit.position, destination, movement_speed);",
    "        let base_movement_speed =\n            if target.is_some_and(|target| target.state == TacticalUnitState::Routed) {\n                unit.speed_mm_per_tick.saturating_mul(2)\n            } else {\n                unit.speed_mm_per_tick\n            };\n        let movement_speed = self.terrain().movement_speed_mm_per_tick(\n            self.battlefield,\n            unit.position,\n            base_movement_speed,\n        );\n        let next = move_point_toward(unit.position, destination, movement_speed);",
)

replace_once(
    "crates/medieval-core/src/tactical.rs",
    "        let route_speed = unit.speed_mm_per_tick.saturating_mul(2);\n        let next = move_point_away(\n            unit.position,\n            enemy.position,\n            route_speed,",
    "        let route_speed = self.terrain().movement_speed_mm_per_tick(\n            self.battlefield,\n            unit.position,\n            unit.speed_mm_per_tick.saturating_mul(2),\n        );\n        let next = move_point_away(\n            unit.position,\n            enemy.position,\n            route_speed,",
)

replace_once(
    "crates/medieval-core/src/tactical.rs",
    "    #[test]\n    fn rejected_orders_do_not_mutate_authoritative_state() {",
    "    #[test]\n    fn forest_ground_cover_reduces_tick_movement_without_changing_orders() {\n        let battlefield = FlatBattlefield::new(80_000, 80_000);\n        let destination = BattlePoint::new(35_000, 15_000);\n        let mut forest = TacticalBattle::new(\n            battlefield,\n            vec![sample_unit(\n                \"forest\",\n                BattleSide::Attacker,\n                BattlePoint::new(25_000, 15_000),\n                Formation::Line { files: 10 },\n            )],\n        )\n        .unwrap();\n        forest\n            .issue_move_order(MovementOrder {\n                unit_id: \"forest\".into(),\n                destination,\n            })\n            .unwrap();\n        forest.advance_ticks(1);\n        assert_eq!(unit(&forest, \"forest\").position(), BattlePoint::new(25_600, 15_000));\n        assert_eq!(unit(&forest, \"forest\").destination(), Some(destination));\n\n        let mut open = TacticalBattle::new(\n            battlefield,\n            vec![sample_unit(\n                \"open\",\n                BattleSide::Attacker,\n                BattlePoint::new(5_000, 15_000),\n                Formation::Line { files: 10 },\n            )],\n        )\n        .unwrap();\n        open.issue_move_order(MovementOrder {\n            unit_id: \"open\".into(),\n            destination: BattlePoint::new(15_000, 15_000),\n        })\n        .unwrap();\n        open.advance_ticks(1);\n        assert_eq!(unit(&open, \"open\").position(), BattlePoint::new(6_200, 15_000));\n    }\n\n    #[test]\n    fn rejected_orders_do_not_mutate_authoritative_state() {",
)

replace_once(
    "crates/medieval-renderer/src/terrain.rs",
    "    TacticalTerrain::height_foundation()",
    "    TacticalTerrain::battlefield_foundation()",
)

replace_once(
    "crates/medieval-renderer/src/scene.rs",
    "    BattlePoint, BattleSide, DeploymentZone, FlatBattlefield, Formation, TacticalBattle,\n};",
    "    BattlePoint, BattleSide, DeploymentZone, FlatBattlefield, Formation, TacticalBattle,\n    TacticalTerrainCell,\n};",
)
replace_once(
    "crates/medieval-renderer/src/scene.rs",
    "    pub deployment_zones: [DeploymentZone; 2],\n    pub camera: Camera3d,",
    "    pub deployment_zones: [DeploymentZone; 2],\n    pub forest_cells: Vec<TacticalTerrainCell>,\n    pub camera: Camera3d,",
)
replace_once(
    "crates/medieval-renderer/src/scene.rs",
    "            deployment_zones: battle.deployment_zones(),\n            camera: view.camera,",
    "            deployment_zones: battle.deployment_zones(),\n            forest_cells: battle.terrain().forest_cells().to_vec(),\n            camera: view.camera,",
)
replace_once(
    "crates/medieval-renderer/src/scene.rs",
    "        assert_eq!(first.deployment_zones, battle.deployment_zones());\n        assert_eq!(first.units[0].soldier_centers_mm.len(), 80);",
    "        assert_eq!(first.deployment_zones, battle.deployment_zones());\n        assert_eq!(first.forest_cells, battle.terrain().forest_cells().to_vec());\n        assert_eq!(first.units[0].soldier_centers_mm.len(), 80);",
)

replace_once(
    "crates/medieval-renderer/src/gpu.rs",
    "use medieval_core::{BattlePoint, BattleSide, DeploymentZone, FlatBattlefield};",
    "use medieval_core::{\n    BattlePoint, BattleSide, DeploymentZone, FlatBattlefield, TacticalTerrainCell,\n};",
)
replace_once(
    "crates/medieval-renderer/src/gpu.rs",
    "const DEPLOYMENT_BOUNDARY_HEIGHT_MM: f32 = 40.0;\nconst CUBE_VERTEX_COUNT: u32 = 36;",
    "const DEPLOYMENT_BOUNDARY_HEIGHT_MM: f32 = 40.0;\nconst FOREST_TREES_PER_CELL: u32 = 4;\nconst CUBE_VERTEX_COUNT: u32 = 36;",
)
replace_once(
    "crates/medieval-renderer/src/gpu.rs",
    "    fn soldier(\n        center: [f32; 3],",
    "    fn forest_tree(\n        cell: TacticalTerrainCell,\n        battlefield: FlatBattlefield,\n        tree_index: u32,\n    ) -> Option<Self> {\n        if tree_index >= FOREST_TREES_PER_CELL {\n            return None;\n        }\n        let (x0, x1, z0, z1) =\n            terrain_cell_bounds_mm(battlefield, cell.cell_x, cell.cell_z)?;\n        if x1 <= x0 || z1 <= z0 {\n            return None;\n        }\n        let slot_x = tree_index % 2;\n        let slot_z = tree_index / 2;\n        let tree_x = x0\n            + u32::try_from(\n                u64::from(x1 - x0) * u64::from(slot_x * 2 + 1) / 4,\n            )\n            .expect(\"forest tree X offset fits in u32\");\n        let tree_z = z0\n            + u32::try_from(\n                u64::from(z1 - z0) * u64::from(slot_z * 2 + 1) / 4,\n            )\n            .expect(\"forest tree Z offset fits in u32\");\n        let terrain_y = terrain_height_mm(battlefield, BattlePoint::new(tree_x, tree_z)) as f32;\n        let minimum_span = (x1 - x0).min(z1 - z0) as f32;\n        let half_width = (minimum_span * 0.06).clamp(120.0, 700.0);\n        let half_height = (half_width * 3.0).clamp(700.0, 2_600.0);\n        Some(Self {\n            center_material: [tree_x as f32, terrain_y + half_height, tree_z as f32, 5.0],\n            half_extent_routed: [half_width, half_height, half_width, 0.0],\n            visual: [0.0; 4],\n        })\n    }\n\n    fn soldier(\n        center: [f32; 3],",
)
replace_once(
    "crates/medieval-renderer/src/gpu.rs",
    "    let deployment_capacity = (2 * TERRAIN_GRID_SIZE) as usize;\n    let mut instances = Vec::with_capacity(terrain_capacity + deployment_capacity + soldier_count);",
    "    let deployment_capacity = (2 * TERRAIN_GRID_SIZE) as usize;\n    let forest_capacity = snapshot.forest_cells.len() * FOREST_TREES_PER_CELL as usize;\n    let mut instances = Vec::with_capacity(\n        terrain_capacity + deployment_capacity + forest_capacity + soldier_count,\n    );",
)
replace_once(
    "crates/medieval-renderer/src/gpu.rs",
    "    for zone in snapshot.deployment_zones {",
    "    for cell in &snapshot.forest_cells {\n        for tree_index in 0..FOREST_TREES_PER_CELL {\n            if let Some(tree) = GpuWorldInstance::forest_tree(*cell, snapshot.battlefield, tree_index)\n            {\n                instances.push(tree);\n            }\n        }\n    }\n    for zone in snapshot.deployment_zones {",
)
replace_once(
    "crates/medieval-renderer/src/gpu.rs",
    "            (TERRAIN_GRID_SIZE * TERRAIN_GRID_SIZE + 2 * TERRAIN_GRID_SIZE) as usize + 80",
    "            (TERRAIN_GRID_SIZE * TERRAIN_GRID_SIZE + 2 * TERRAIN_GRID_SIZE) as usize\n                + snapshot.forest_cells.len() * FOREST_TREES_PER_CELL as usize\n                + 80",
)
replace_once(
    "crates/medieval-renderer/src/gpu.rs",
    "    #[test]\n    fn terrain_instances_raise_center_cells_above_edge_cells() {",
    "    #[test]\n    fn forest_instances_are_projected_from_core_cells() {\n        let battlefield = FlatBattlefield::new(100_000, 100_000);\n        let battle = TacticalBattle::new(\n            battlefield,\n            vec![TacticalUnit::new(\n                \"attacker\",\n                BattleSide::Attacker,\n                1,\n                BattlePoint::new(10_000, 10_000),\n                Formation::Line { files: 1 },\n                1_000,\n            )],\n        )\n        .unwrap();\n        let snapshot =\n            BattleRenderSnapshot::capture(&battle, &RenderViewState::fit(battlefield));\n        let cell = snapshot.forest_cells[0];\n        let tree = GpuWorldInstance::forest_tree(cell, battlefield, 0).unwrap();\n        let (x0, x1, z0, z1) =\n            terrain_cell_bounds_mm(battlefield, cell.cell_x, cell.cell_z).unwrap();\n        assert_eq!(tree.center_material[3], 5.0);\n        assert!(tree.center_material[0] > x0 as f32 && tree.center_material[0] < x1 as f32);\n        assert!(tree.center_material[2] > z0 as f32 && tree.center_material[2] < z1 as f32);\n        assert!(tree.half_extent_routed[1] > tree.half_extent_routed[0]);\n    }\n\n    #[test]\n    fn terrain_instances_raise_center_cells_above_edge_cells() {",
)

replace_once(
    "crates/medieval-renderer/src/battlefield.wgsl",
    "    if input.material > 3.5 {\n        color = vec3<f32>(0.18, 0.45, 0.92);",
    "    if input.material > 4.5 {\n        color = vec3<f32>(0.10, 0.24, 0.08);\n    } else if input.material > 3.5 {\n        color = vec3<f32>(0.18, 0.45, 0.92);",
)

replace_once(
    "web-battle-wasm/src/lib.rs",
    "    TACTICAL_TICKS_PER_SECOND, TacticalBattle, TacticalUnit,\n};",
    "    TACTICAL_TICKS_PER_SECOND, TacticalBattle, TacticalTerrainCell, TacticalUnit,\n};",
)
replace_once(
    "web-battle-wasm/src/lib.rs",
    "    deployment_zones: [DeploymentZone; 2],\n    camera: CameraStatus,",
    "    deployment_zones: [DeploymentZone; 2],\n    forest_cells: Vec<TacticalTerrainCell>,\n    camera: CameraStatus,",
)
replace_once(
    "web-battle-wasm/src/lib.rs",
    "            deployment_zones: self.battle.deployment_zones(),\n            camera: CameraStatus {",
    "            deployment_zones: self.battle.deployment_zones(),\n            forest_cells: self.battle.terrain().forest_cells().to_vec(),\n            camera: CameraStatus {",
)

replace_once(
    "web-tests/browser-battle-contract.test.mjs",
    "  assert.match(wasmRust, /deployment_zones/);",
    "  assert.match(wasmRust, /deployment_zones/);\n  assert.match(wasmRust, /forest_cells/);\n  assert.match(wasmRust, /battle\\.terrain\\(\\)\\.forest_cells\\(\\)/);",
)

replace_once(
    "e2e/tactical-controls.spec.mjs",
    "  expect(current.deploymentZones).toEqual([\n    { side: \"attacker\", minXMm: 0, maxXMm: 33_333, minYMm: 0, maxYMm: 100_000 },\n    { side: \"defender\", minXMm: 66_667, maxXMm: 100_000, minYMm: 0, maxYMm: 100_000 },\n  ]);",
    "  expect(current.deploymentZones).toEqual([\n    { side: \"attacker\", minXMm: 0, maxXMm: 33_333, minYMm: 0, maxYMm: 100_000 },\n    { side: \"defender\", minXMm: 66_667, maxXMm: 100_000, minYMm: 0, maxYMm: 100_000 },\n  ]);\n  expect(current.forestCells).toEqual([\n    { cellX: 2, cellZ: 1 },\n    { cellX: 3, cellZ: 1 },\n    { cellX: 3, cellZ: 2 },\n    { cellX: 4, cellZ: 5 },\n    { cellX: 4, cellZ: 6 },\n    { cellX: 5, cellZ: 6 },\n  ]);",
)

replace_once(
    "docs/ROADMAP.md",
    "6. **Terrain and battlefield legality — deployment zones current:** deterministic height/cell geometry lives in `medieval-core`; core-owned attacker/defender deployment zones now validate production battle setup and are projected by renderers. Next add forests and rivers, then derive chokepoints from explicit movement/pathing rules.",
    "6. **Terrain and battlefield legality — forests current:** deterministic height/cell geometry and forest cover live in `medieval-core`; deployment zones validate production setup, and units starting a tick in forest move at half speed while renderers only project the core-owned cover. Next add rivers, then derive chokepoints from explicit movement/pathing rules.",
)

replace_once(
    "docs/BATTLE-ARCHITECTURE.md",
    "Owns battle truth: units, positions on the battlefield, formations, movement, combat, morale, fatigue, routing, deterministic ticks, and the deterministic `TacticalTerrain` elevation contract. Terrain effects on gameplay remain separate rules that have not landed yet.",
    "Owns battle truth: units, positions on the battlefield, formations, movement, combat, morale, fatigue, routing, deterministic ticks, and the deterministic `TacticalTerrain` elevation/ground-cover contract. Forest movement is the first terrain-owned gameplay modifier; additional terrain effects remain explicit core rules.",
)
replace_once(
    "docs/BATTLE-ARCHITECTURE.md",
    "The current terrain-height profile is core-owned and gameplay-neutral. `medieval-core::TacticalTerrain` owns deterministic height and cell-boundary queries; `medieval-renderer` is only a projection adapter for geometry and picking. Movement, combat, morale, routing, and legality still do not depend on elevation.",
    "The current terrain profile is core-owned. `medieval-core::TacticalTerrain` owns deterministic height, cell-boundary, and ground-cover queries; `medieval-renderer` is only a projection adapter for geometry and picking. Elevation is still gameplay-neutral, while forest cover now applies one explicit movement-speed rule in core. Combat, morale, and line-of-sight remain independent of terrain.",
)
replace_once(
    "docs/BATTLE-ARCHITECTURE.md",
    "| Flat renderer ground had no elevation source | Replaced by the core-owned deterministic `TacticalTerrain::HeightFoundationV1` profile. wgpu geometry, unit elevation, camera targeting, and viewport picking consume the same contract while gameplay remains terrain-neutral. |",
    "| Flat renderer ground had no elevation source | Replaced by the core-owned deterministic `TacticalTerrain::HeightFoundationV1` profile. wgpu geometry, unit elevation, camera targeting, viewport picking, and forest projection consume the same contract; forest cover now also feeds an explicit core movement modifier. |",
)
replace_once(
    "docs/BATTLE-ARCHITECTURE.md",
    "This makes terrain elevation authoritative data, but **not yet a tactical modifier**. Movement, combat, morale, routing, and legality remain independent of elevation until explicit core rules consume the terrain contract.",
    "Forest cover is the first tactical terrain modifier. The current profile owns six deterministic forest cells. A unit that starts a simulation tick in a forest cell receives half of its normal movement budget for that tick, rounded up; the same rule is applied to normal, pursuit, and routed movement. The renderer consumes those exact cells and draws primitive tree proxies, but those proxies do not own collision or movement rules. Forests currently do not modify combat, morale, line-of-sight, or deployment legality, and elevation itself remains gameplay-neutral.",
)
replace_once(
    "docs/BATTLE-ARCHITECTURE.md",
    "1. Add forests and rivers as core-owned terrain features with renderer projection kept separate from effects.\n2. Derive chokepoints from explicit movement/pathing legality rather than renderer geometry.",
    "1. Add rivers as the next core-owned terrain feature, with crossing legality/effects defined before renderer decoration.\n2. Derive chokepoints from explicit movement/pathing legality rather than renderer geometry.",
)
replace_once(
    "docs/BATTLE-ARCHITECTURE.md",
    "Terrain height is now authoritative through the explicit deterministic `TacticalTerrain` interface. Hills must remain gameplay-neutral until movement, combat, or line-of-sight effects are introduced as separate core rules.",
    "Terrain height and forest cover are authoritative through the explicit deterministic `TacticalTerrain` interface. Hills remain gameplay-neutral; any future hill, river, combat, or line-of-sight effects must land as separate core rules rather than renderer behavior.",
)
