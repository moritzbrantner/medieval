import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

const [
  html,
  index,
  script,
  launcher,
  wasmRust,
  nativeControls,
  browserControls,
  pages,
  tacticalCore,
  terrainCore,
  rendererScene,
] = await Promise.all([
  readFile(new URL("../web/battle.html", import.meta.url), "utf8"),
  readFile(new URL("../web/index.html", import.meta.url), "utf8"),
  readFile(new URL("../web/battle-sandbox.js", import.meta.url), "utf8"),
  readFile(new URL("../web/native-battle.js", import.meta.url), "utf8"),
  readFile(new URL("../web-battle-wasm/src/lib.rs", import.meta.url), "utf8"),
  readFile(new URL("../src-tauri/src/native_battle/controls.rs", import.meta.url), "utf8"),
  readFile(new URL("../web-battle-wasm/src/controls.rs", import.meta.url), "utf8"),
  readFile(new URL("../.github/workflows/pages.yml", import.meta.url), "utf8"),
  readFile(new URL("../crates/medieval-core/src/tactical.rs", import.meta.url), "utf8"),
  readFile(new URL("../crates/medieval-core/src/terrain.rs", import.meta.url), "utf8"),
  readFile(new URL("../crates/medieval-renderer/src/scene.rs", import.meta.url), "utf8"),
]);

test("Pages exposes a focused single-player tactical sandbox", () => {
  assert.match(index, /id="open-battle-sandbox"/);
  assert.match(index, />Battle Sandbox</);
  assert.match(launcher, /battleSandboxButton\?\.addEventListener\("click", openBattlePreview\)/);
  assert.match(launcher, /new URL\("battle\.html", window\.location\.href\)/);
  assert.match(html, /<canvas[\s\S]*id="battle-canvas"/);
  assert.match(html, /id="pause-battle"/);
  assert.match(html, /id="stop-units"/);
  assert.match(html, /id="fit-camera"/);
  assert.match(html, /id="reset-battle"/);
  assert.match(html, /id="battle-location"/);
  assert.match(index, /id="sandbox-battle-location"/);
  assert.match(index, /id="online-battle-location"/);
  assert.match(script, /battle_sandbox_start_at_location/);
  assert.match(script, /battle_sandbox_set_location/);
  assert.match(launcher, /searchParams\.set\("location", location\)/);
  assert.match(html, /type="module" src="battle-sandbox\.js"/);
});

test("browser input is adaptation only while tactical state remains Rust-owned", () => {
  assert.match(script, /medieval_web_battle\.js/);
  assert.match(script, /battle_sandbox_start/);
  assert.match(script, /battle_sandbox_frame/);
  assert.match(script, /battle_sandbox_pointer/);
  assert.match(script, /battle_sandbox_control/);
  assert.doesNotMatch(script, /advance_ticks|TacticalBattle|issue_move_order|issue_engagement_order/);
  assert.doesNotMatch(script, /casualt(?:y|ies).*[-+*/]|morale.*[-+*/]|fatigue.*[-+*/]/i);
  assert.doesNotMatch(script, /movement_waypoint|cell_is_passable|river_crossing_cells/);
});

test("browser sandbox renders through wgpu and advances the real tactical core", () => {
  assert.match(wasmRust, /SurfaceTarget::Canvas/);
  assert.match(wasmRust, /GpuBattleRenderer/);
  assert.match(wasmRust, /TACTICAL_TICKS_PER_SECOND/);
  assert.match(wasmRust, /battle\.advance_ticks\(pending_ticks\)/);
  assert.match(wasmRust, /TacticalControls/);
  assert.match(wasmRust, /TacticalBattle::deploy/);
  assert.match(wasmRust, /deployment_zones/);
  assert.match(wasmRust, /terrain\.forest_cells\(\)/);
  assert.match(wasmRust, /terrain\.river_cells\(\)/);
  assert.match(wasmRust, /terrain\.river_crossing_cells\(\)/);
  assert.match(wasmRust, /terrain\.height_mm\(battlefield, position\)/);
  assert.match(wasmRust, /terrain\.ground_cover_at\(battlefield, position\)/);
  assert.match(wasmRust, /\.ranged_target_damage_factor_milli\(battlefield, position\)/);
  assert.match(wasmRust, /terrain\.elevation_damage_factor_milli/);
  assert.match(wasmRust, /issue_engagement_order/);
  assert.match(wasmRust, /battle_sandbox_start/);
  assert.match(wasmRust, /battle_sandbox_pointer/);
});

test("battlefield choice stays core-owned while browser and renderer only adapt it", () => {
  assert.match(terrainCore, /enum BattlefieldLocation/);
  assert.match(terrainCore, /MountainPass/);
  assert.match(terrainCore, /ForestClearing/);
  assert.match(terrainCore, /RiverFord/);
  assert.match(tacticalCore, /deploy_at_location/);
  assert.match(rendererScene, /pub terrain: TacticalTerrain/);
  assert.doesNotMatch(script, /FOREST_CLEARING_CELLS|RIVER_FORD_FOREST_CELLS|mountain_height_mm/);
});

test("river legality and chokepoint routing stay core-owned while the renderer only projects cells", () => {
  assert.match(terrainCore, /RiverCrossingsV3/);
  assert.match(terrainCore, /fn cell_is_passable/);
  assert.match(terrainCore, /fn movement_waypoint/);
  assert.match(terrainCore, /river_crossing_cells/);
  assert.match(tacticalCore, /DestinationImpassable/);
  assert.match(tacticalCore, /\.movement_waypoint\(self\.battlefield/);
  assert.match(rendererScene, /river_cells: Vec<TacticalTerrainCell>/);
  assert.match(rendererScene, /river_crossing_cells: Vec<TacticalTerrainCell>/);
  assert.doesNotMatch(rendererScene, /fn movement_waypoint|fn cell_is_passable/);
});

test("terrain combat modifiers stay Rust-owned while browser status only projects them", () => {
  assert.match(terrainCore, /CombatTerrainV4/);
  assert.match(terrainCore, /fn elevation_damage_factor_milli/);
  assert.match(terrainCore, /fn ranged_target_damage_factor_milli/);
  assert.match(tacticalCore, /terrain_adjusted_frontage/);
  assert.doesNotMatch(script, /FOREST_RANGED_DAMAGE_FACTOR|COMBAT_FACTOR_BASE|elevation_damage_factor/);
});

test("native and browser runtimes consume one tactical control implementation", () => {
  assert.match(nativeControls, /include!\("\.\.\/\.\.\/\.\.\/shared\/tactical_controls\.rs"\)/);
  assert.match(browserControls, /include!\("\.\.\/\.\.\/shared\/tactical_controls\.rs"\)/);
});

test("browser snapshots reconcile autonomous routing before exposing selection", () => {
  assert.match(browserControls, /fn reconcile_with_battle\([\s\S]*self\.sync_with_battle\(battle\)/);
  const renderView = browserControls.slice(browserControls.indexOf("pub fn render_view"));
  const reconcileIndex = renderView.indexOf("controls.reconcile_with_battle(battle);");
  const renderIndex = renderView.indexOf("controls.render_view(battle)");
  assert.ok(reconcileIndex >= 0, "browser render snapshots must reconcile the shared control state");
  assert.ok(renderIndex > reconcileIndex, "selection must be reconciled before projection/status reads it");
});

test("Pages creates browser bindings from the tactical WASM artifact", () => {
  assert.match(pages, /web-battle-wasm\/Cargo\.toml/);
  assert.match(pages, /wasm-bindgen-cli --version 0\.2\.128 --locked/);
  assert.match(pages, /medieval_web_battle\.wasm/);
  assert.match(pages, /--target web/);
  assert.match(pages, /--out-name medieval_web_battle/);
});
