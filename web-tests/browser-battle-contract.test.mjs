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
  assert.match(wasmRust, /forest_cells/);
  assert.match(wasmRust, /battle\.terrain\(\)\.forest_cells\(\)/);
  assert.match(wasmRust, /issue_engagement_order/);
  assert.match(wasmRust, /battle_sandbox_start/);
  assert.match(wasmRust, /battle_sandbox_pointer/);
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
  assert.match(pages, /wasm-bindgen-cli --version 0\.2\.127 --locked/);
  assert.match(pages, /medieval_web_battle\.wasm/);
  assert.match(pages, /--target web/);
  assert.match(pages, /--out-name medieval_web_battle/);
});
