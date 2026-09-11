from pathlib import Path


def replace_once(path: str, old: str, new: str) -> None:
    file = Path(path)
    text = file.read_text()
    if old not in text:
        raise SystemExit(f"missing expected block in {path}: {old[:120]!r}")
    file.write_text(text.replace(old, new, 1))


Path("crates/medieval-core/src/deployment.rs").write_text(r'''use serde::{Deserialize, Serialize};

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
pub const fn standard_deployment_zones(
    battlefield: FlatBattlefield,
) -> [DeploymentZone; 2] {
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
''')

replace_once(
    "crates/medieval-core/src/lib.rs",
    "mod battle;\nmod save;\nmod tactical;\nmod terrain;\n",
    "mod battle;\nmod deployment;\nmod save;\nmod tactical;\nmod terrain;\n",
)
replace_once(
    "crates/medieval-core/src/lib.rs",
    "pub use battle::{ArmyRoster, BattleOutcome, BattleReport};\npub use save::{CAMPAIGN_SAVE_SCHEMA_VERSION, CampaignSave, SaveError};\n",
    "pub use battle::{ArmyRoster, BattleOutcome, BattleReport};\npub use deployment::{DeploymentZone, standard_deployment_zone, standard_deployment_zones};\npub use save::{CAMPAIGN_SAVE_SCHEMA_VERSION, CampaignSave, SaveError};\n",
)

replace_once(
    "crates/medieval-core/src/tactical.rs",
    "use serde::{Deserialize, Serialize};\n",
    "use serde::{Deserialize, Serialize};\n\nuse crate::deployment::{DeploymentZone, standard_deployment_zone, standard_deployment_zones};\n",
)
replace_once(
    "crates/medieval-core/src/tactical.rs",
    "        Ok(Self {\n            tick: 0,\n            battlefield,\n            units,\n        })\n    }\n\n    #[must_use]\n    pub const fn tick(&self) -> u64 {",
    "        Ok(Self {\n            tick: 0,\n            battlefield,\n            units,\n        })\n    }\n\n    pub fn deploy(\n        battlefield: FlatBattlefield,\n        units: Vec<TacticalUnit>,\n    ) -> Result<Self, TacticalError> {\n        let battle = Self::new(battlefield, units)?;\n        for unit in &battle.units {\n            let zone = standard_deployment_zone(battlefield, unit.side);\n            if !zone.contains(unit.position) {\n                return Err(TacticalError::UnitOutsideDeploymentZone {\n                    unit_id: unit.id.clone(),\n                    side: unit.side,\n                    position: unit.position,\n                });\n            }\n        }\n        Ok(battle)\n    }\n\n    #[must_use]\n    pub const fn deployment_zones(&self) -> [DeploymentZone; 2] {\n        standard_deployment_zones(self.battlefield)\n    }\n\n    #[must_use]\n    pub const fn tick(&self) -> u64 {",
)
replace_once(
    "crates/medieval-core/src/tactical.rs",
    "    UnitOutOfBounds {\n        unit_id: String,\n        position: BattlePoint,\n    },\n    UnitNotFound(String),",
    "    UnitOutOfBounds {\n        unit_id: String,\n        position: BattlePoint,\n    },\n    UnitOutsideDeploymentZone {\n        unit_id: String,\n        side: BattleSide,\n        position: BattlePoint,\n    },\n    UnitNotFound(String),",
)
replace_once(
    "crates/medieval-core/src/tactical.rs",
    "            Self::UnitOutOfBounds { unit_id, position } => write!(\n                formatter,\n                \"tactical unit {unit_id} starts outside the battlefield at ({}, {}) mm\",\n                position.x_mm, position.y_mm\n            ),\n            Self::UnitNotFound(unit_id) => {",
    "            Self::UnitOutOfBounds { unit_id, position } => write!(\n                formatter,\n                \"tactical unit {unit_id} starts outside the battlefield at ({}, {}) mm\",\n                position.x_mm, position.y_mm\n            ),\n            Self::UnitOutsideDeploymentZone {\n                unit_id,\n                side,\n                position,\n            } => write!(\n                formatter,\n                \"tactical unit {unit_id} starts outside the {:?} deployment zone at ({}, {}) mm\",\n                side, position.x_mm, position.y_mm\n            ),\n            Self::UnitNotFound(unit_id) => {",
)
replace_once(
    "crates/medieval-core/src/tactical.rs",
    "    fn sample_battle() -> TacticalBattle {\n        TacticalBattle::new(\n            FlatBattlefield::new(300_000, 200_000),",
    "    fn sample_battle() -> TacticalBattle {\n        TacticalBattle::deploy(\n            FlatBattlefield::new(300_000, 200_000),",
)
replace_once(
    "crates/medieval-core/src/tactical.rs",
    "    #[test]\n    fn rejected_orders_do_not_mutate_authoritative_state() {",
    "    #[test]\n    fn deployment_validation_accepts_own_back_thirds_and_rejects_neutral_setup() {\n        let battlefield = FlatBattlefield::new(90_000, 60_000);\n        let deployed = TacticalBattle::deploy(\n            battlefield,\n            vec![\n                sample_unit(\n                    \"attacker\",\n                    BattleSide::Attacker,\n                    BattlePoint::new(30_000, 20_000),\n                    Formation::Line { files: 10 },\n                ),\n                sample_unit(\n                    \"defender\",\n                    BattleSide::Defender,\n                    BattlePoint::new(60_000, 40_000),\n                    Formation::Line { files: 10 },\n                ),\n            ],\n        )\n        .unwrap();\n        assert_eq!(deployed.deployment_zones(), standard_deployment_zones(battlefield));\n\n        let invalid = TacticalBattle::deploy(\n            battlefield,\n            vec![sample_unit(\n                \"attacker\",\n                BattleSide::Attacker,\n                BattlePoint::new(45_000, 30_000),\n                Formation::Line { files: 10 },\n            )],\n        );\n        assert!(matches!(\n            invalid,\n            Err(TacticalError::UnitOutsideDeploymentZone {\n                side: BattleSide::Attacker,\n                ..\n            })\n        ));\n    }\n\n    #[test]\n    fn rejected_orders_do_not_mutate_authoritative_state() {",
)

replace_once(
    "crates/medieval-renderer/src/scene.rs",
    "use medieval_core::{BattlePoint, BattleSide, FlatBattlefield, Formation, TacticalBattle};",
    "use medieval_core::{\n    BattlePoint, BattleSide, DeploymentZone, FlatBattlefield, Formation, TacticalBattle,\n};",
)
replace_once(
    "crates/medieval-renderer/src/scene.rs",
    "pub struct BattleRenderSnapshot {\n    pub tick: u64,\n    pub battlefield: FlatBattlefield,\n    pub camera: Camera3d,\n    pub units: Vec<RenderUnitInstance>,\n}",
    "pub struct BattleRenderSnapshot {\n    pub tick: u64,\n    pub battlefield: FlatBattlefield,\n    pub deployment_zones: [DeploymentZone; 2],\n    pub camera: Camera3d,\n    pub units: Vec<RenderUnitInstance>,\n}",
)
replace_once(
    "crates/medieval-renderer/src/scene.rs",
    "        Self {\n            tick: battle.tick(),\n            battlefield,\n            camera: view.camera,\n            units,\n        }",
    "        Self {\n            tick: battle.tick(),\n            battlefield,\n            deployment_zones: battle.deployment_zones(),\n            camera: view.camera,\n            units,\n        }",
)
replace_once(
    "crates/medieval-renderer/src/scene.rs",
    "        assert_eq!(first, BattleRenderSnapshot::project(&battle, &view));\n        assert_eq!(first.units[0].soldier_centers_mm.len(), 80);",
    "        assert_eq!(first, BattleRenderSnapshot::project(&battle, &view));\n        assert_eq!(first.deployment_zones, battle.deployment_zones());\n        assert_eq!(first.units[0].soldier_centers_mm.len(), 80);",
)

replace_once(
    "crates/medieval-renderer/src/gpu.rs",
    "use medieval_core::{BattleSide, FlatBattlefield};",
    "use medieval_core::{BattlePoint, BattleSide, DeploymentZone, FlatBattlefield};",
)
replace_once(
    "crates/medieval-renderer/src/gpu.rs",
    "    terrain::{TERRAIN_GRID_SIZE, terrain_cell_bounds_mm, terrain_cell_height_mm},",
    "    terrain::{\n        TERRAIN_GRID_SIZE, terrain_cell_bounds_mm, terrain_cell_height_mm, terrain_height_mm,\n    },",
)
replace_once(
    "crates/medieval-renderer/src/gpu.rs",
    "const GROUND_BASE_DEPTH_MM: f32 = 200.0;\nconst CUBE_VERTEX_COUNT: u32 = 36;",
    "const GROUND_BASE_DEPTH_MM: f32 = 200.0;\nconst DEPLOYMENT_BOUNDARY_HALF_WIDTH_MM: f32 = 180.0;\nconst DEPLOYMENT_BOUNDARY_HEIGHT_MM: f32 = 360.0;\nconst CUBE_VERTEX_COUNT: u32 = 36;",
)
replace_once(
    "crates/medieval-renderer/src/gpu.rs",
    "    fn soldier(\n        center: [f32; 3],",
    "    fn deployment_boundary_segment(\n        zone: DeploymentZone,\n        battlefield: FlatBattlefield,\n        cell_z: u32,\n    ) -> Option<Self> {\n        let (_, _, z0, z1) = terrain_cell_bounds_mm(battlefield, 0, cell_z)?;\n        if z1 <= z0 {\n            return None;\n        }\n        let boundary_x = match zone.side {\n            BattleSide::Attacker => zone.max_x_mm,\n            BattleSide::Defender => zone.min_x_mm,\n        };\n        let center_z = z0 + (z1 - z0) / 2;\n        let terrain_y =\n            terrain_height_mm(battlefield, BattlePoint::new(boundary_x, center_z)) as f32;\n        let half_height = DEPLOYMENT_BOUNDARY_HEIGHT_MM / 2.0;\n        Some(Self {\n            center_material: [\n                boundary_x as f32,\n                terrain_y + half_height,\n                (z0 as f32 + z1 as f32) / 2.0,\n                match zone.side {\n                    BattleSide::Attacker => 3.0,\n                    BattleSide::Defender => 4.0,\n                },\n            ],\n            half_extent_routed: [\n                DEPLOYMENT_BOUNDARY_HALF_WIDTH_MM,\n                half_height,\n                (z1 - z0) as f32 / 2.0,\n                0.0,\n            ],\n            visual: [0.0; 4],\n        })\n    }\n\n    fn soldier(\n        center: [f32; 3],",
)
replace_once(
    "crates/medieval-renderer/src/gpu.rs",
    "    let terrain_capacity = (TERRAIN_GRID_SIZE * TERRAIN_GRID_SIZE) as usize;\n    let mut instances = Vec::with_capacity(terrain_capacity + soldier_count);",
    "    let terrain_capacity = (TERRAIN_GRID_SIZE * TERRAIN_GRID_SIZE) as usize;\n    let deployment_capacity = (2 * TERRAIN_GRID_SIZE) as usize;\n    let mut instances =\n        Vec::with_capacity(terrain_capacity + deployment_capacity + soldier_count);",
)
replace_once(
    "crates/medieval-renderer/src/gpu.rs",
    "    for unit in &snapshot.units {",
    "    for zone in snapshot.deployment_zones {\n        for cell_z in 0..TERRAIN_GRID_SIZE {\n            if let Some(marker) =\n                GpuWorldInstance::deployment_boundary_segment(zone, snapshot.battlefield, cell_z)\n            {\n                instances.push(marker);\n            }\n        }\n    }\n    for unit in &snapshot.units {",
)
replace_once(
    "crates/medieval-renderer/src/gpu.rs",
    "            (TERRAIN_GRID_SIZE * TERRAIN_GRID_SIZE) as usize + 80",
    "            (TERRAIN_GRID_SIZE * TERRAIN_GRID_SIZE + 2 * TERRAIN_GRID_SIZE) as usize + 80",
)
replace_once(
    "crates/medieval-renderer/src/gpu.rs",
    "    #[test]\n    fn terrain_instances_raise_center_cells_above_edge_cells() {",
    "    #[test]\n    fn deployment_boundaries_are_projected_from_core_zones() {\n        let battlefield = FlatBattlefield::new(100_000, 100_000);\n        let zones = medieval_core::standard_deployment_zones(battlefield);\n        let attacker = GpuWorldInstance::deployment_boundary_segment(zones[0], battlefield, 0).unwrap();\n        let defender = GpuWorldInstance::deployment_boundary_segment(zones[1], battlefield, 0).unwrap();\n        assert_eq!(attacker.center_material[0], zones[0].max_x_mm as f32);\n        assert_eq!(defender.center_material[0], zones[1].min_x_mm as f32);\n        assert_eq!(attacker.center_material[3], 3.0);\n        assert_eq!(defender.center_material[3], 4.0);\n    }\n\n    #[test]\n    fn terrain_instances_raise_center_cells_above_edge_cells() {",
)

replace_once(
    "crates/medieval-renderer/src/battlefield.wgsl",
    "    var color = vec3<f32>(0.69, 0.18, 0.12);\n    if input.material > 1.5 {\n        color = vec3<f32>(0.22, 0.29, 0.15);\n    } else if input.material > 0.5 {\n        color = vec3<f32>(0.12, 0.31, 0.67);\n    }",
    "    var color = vec3<f32>(0.69, 0.18, 0.12);\n    var alpha = 1.0;\n    if input.material > 3.5 {\n        color = vec3<f32>(0.18, 0.45, 0.92);\n        alpha = 0.72;\n    } else if input.material > 2.5 {\n        color = vec3<f32>(0.88, 0.24, 0.16);\n        alpha = 0.72;\n    } else if input.material > 1.5 {\n        color = vec3<f32>(0.22, 0.29, 0.15);\n    } else if input.material > 0.5 {\n        color = vec3<f32>(0.12, 0.31, 0.67);\n    }",
)
replace_once(
    "crates/medieval-renderer/src/battlefield.wgsl",
    "    return vec4<f32>(color * (0.36 + diffuse * 0.64), 1.0);",
    "    return vec4<f32>(color * (0.36 + diffuse * 0.64), alpha);",
)

replace_once(
    "web-battle-wasm/src/lib.rs",
    "    BattlePoint, BattleSide, FlatBattlefield, Formation, TACTICAL_TICKS_PER_SECOND, TacticalBattle,\n    TacticalUnit,",
    "    BattlePoint, BattleSide, DeploymentZone, FlatBattlefield, Formation,\n    TACTICAL_TICKS_PER_SECOND, TacticalBattle, TacticalUnit,",
)
replace_once(
    "web-battle-wasm/src/lib.rs",
    "    selected_units: Vec<String>,\n    camera: CameraStatus,",
    "    selected_units: Vec<String>,\n    deployment_zones: [DeploymentZone; 2],\n    camera: CameraStatus,",
)
replace_once(
    "web-battle-wasm/src/lib.rs",
    "            selected_units: selected.into_iter().map(str::to_owned).collect(),\n            camera: CameraStatus {",
    "            selected_units: selected.into_iter().map(str::to_owned).collect(),\n            deployment_zones: self.battle.deployment_zones(),\n            camera: CameraStatus {",
)
replace_once(
    "web-battle-wasm/src/lib.rs",
    "fn sample_battle() -> Result<TacticalBattle, String> {\n    TacticalBattle::new(",
    "fn sample_battle() -> Result<TacticalBattle, String> {\n    TacticalBattle::deploy(",
)

replace_once(
    "src-tauri/src/native_battle.rs",
    "fn sample_session() -> NativeBattleSession {\n    let battlefield = FlatBattlefield::new(100_000, 100_000);\n    let battle = TacticalBattle::new(",
    "fn sample_session() -> NativeBattleSession {\n    let battlefield = FlatBattlefield::new(100_000, 100_000);\n    let battle = TacticalBattle::deploy(",
)

replace_once(
    "web-tests/browser-battle-contract.test.mjs",
    "  assert.match(wasmRust, /TacticalControls/);\n  assert.match(wasmRust, /issue_engagement_order/);",
    "  assert.match(wasmRust, /TacticalControls/);\n  assert.match(wasmRust, /TacticalBattle::deploy/);\n  assert.match(wasmRust, /deployment_zones/);\n  assert.match(wasmRust, /issue_engagement_order/);",
)

replace_once(
    "e2e/tactical-controls.spec.mjs",
    "  await expect(page.locator(\"#battle-error\")).toBeHidden();\n\n  await clickUnit(page, \"attacker-spears\");",
    "  await expect(page.locator(\"#battle-error\")).toBeHidden();\n\n  let current = await status(page);\n  expect(current.deploymentZones).toEqual([\n    { side: \"attacker\", minXMm: 0, maxXMm: 33_333, minYMm: 0, maxYMm: 100_000 },\n    { side: \"defender\", minXMm: 66_667, maxXMm: 100_000, minYMm: 0, maxYMm: 100_000 },\n  ]);\n\n  await clickUnit(page, \"attacker-spears\");",
)
replace_once(
    "e2e/tactical-controls.spec.mjs",
    "  await clickUnit(page, \"defender-spears\", { button: \"right\" });\n  let current = await status(page);",
    "  await clickUnit(page, \"defender-spears\", { button: \"right\" });\n  current = await status(page);",
)

replace_once(
    "docs/ROADMAP.md",
    "5. **GitHub Pages demo renderer — current:** Three.js projection of the same authoritative battle state/contracts for browser dogfood and public demos; no duplicate simulation truth. Reuse the proven physical-input vocabulary where useful, but keep battle truth and control semantics in Rust.\n6. **Terrain — height authority complete:** deterministic height/cell geometry now lives in `medieval-core`; next add forests, rivers, chokepoints, and deployment zones without moving renderer concerns into core.",
    "5. **GitHub Pages demo renderer — complete acceptance foundation:** the browser projects the same authoritative tactical state, uses the shared Rust-local control semantics, and now has release-WASM Playwright acceptance for physical pointer/keyboard/wheel controls.\n6. **Terrain and battlefield legality — deployment zones current:** deterministic height/cell geometry lives in `medieval-core`; core-owned attacker/defender deployment zones now validate production battle setup and are projected by renderers. Next add forests and rivers, then derive chokepoints from explicit movement/pathing rules.",
)
replace_once(
    "docs/BATTLE-ARCHITECTURE.md",
    "1. Add forests and rivers as core-owned terrain features with renderer projection kept separate from effects.\n2. Add chokepoints and deployment zones through explicit tactical legality queries.\n3. Add explicit orbit/rotation input using the existing yaw/pitch camera state.",
    "1. Add forests and rivers as core-owned terrain features with renderer projection kept separate from effects.\n2. Derive chokepoints from explicit movement/pathing legality rather than renderer geometry.\n3. Add explicit orbit/rotation input using the existing yaw/pitch camera state.",
)

architecture = Path("docs/BATTLE-ARCHITECTURE.md")
text = architecture.read_text()
anchor = "Terrain picking intersects each rendered cell volume directly, including visible height-step faces, and uses a 1 mm renderer-space tolerance only at geometric boundaries so projected battlefield-edge points do not disappear through floating-point roundoff. That tolerance expands only horizontal X/Z cell bounds; elevation bounds remain exact so top-surface intersections do not shift tactical destinations.\n"
addition = anchor + "\nDeployment legality is also core-owned. `medieval-core` deterministically defines the attacker and defender back-third deployment zones and `TacticalBattle::deploy` rejects initial units outside their side's zone. Renderers consume those exact zones and may visualize their inner boundaries, but they do not decide legal setup positions. Arbitrary `TacticalBattle::new` construction remains available for deterministic mid-battle fixtures and replay/state reconstruction where deployment-phase validation is not applicable.\n"
if anchor not in text:
    raise SystemExit("missing terrain architecture anchor")
architecture.write_text(text.replace(anchor, addition, 1))
