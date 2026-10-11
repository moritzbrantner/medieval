// Source contracts for issue #94 (tactical deployment phase). The browser
// journeys in e2e/tactical-deployment.spec.mjs and the Rust tests in
// crates/medieval-core/tests/tactical_deployment.rs and
// crates/medieval-renderer/tests/shared_deployment_controls.rs prove
// behaviour; these checks keep the footprint rule (owner decision 2026-10-11,
// option A) and the phase gate in Rust, keep the browser and desktop on the
// shared Rust controls, and keep the contract documented.
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

const read = (path) => readFile(new URL(`../${path}`, import.meta.url), "utf8");
const [html, script, bindings, sharedControls, architecture, nativeBattle, nativeInput] = await Promise.all([
  read("web/battle.html"),
  read("web/battle-sandbox.js"),
  read("web/battle-input-bindings.js"),
  read("shared/tactical_controls.rs"),
  read("docs/BATTLE-ARCHITECTURE.md"),
  read("src-tauri/src/native_battle.rs"),
  read("src-tauri/src/native_battle/input.rs"),
]);

test("the deployment phase and Start battle are documented as Rust-owned", () => {
  assert.match(architecture, /deployment phase/i);
  assert.match(architecture, /Start battle/);
  assert.match(
    architecture,
    /deployment phase[\s\S]{0,1200}(?:no tick|ticks? (?:do|does) not|before the start|tick 0)/i,
    "docs state that no tick runs before the start",
  );
  assert.match(
    architecture,
    /deployment phase[\s\S]{0,1500}(?:exactly once|only once|second start|no-op)/i,
    "docs state that the start happens exactly once",
  );
});

test("the footprint placement rule and its scoped defaults are documented", () => {
  // Owner decision 2026-10-11: whole footprint inside the own zone, no
  // friendly overlap, for moves, rotations and frontage changes, in Rust.
  assert.match(architecture, /deployment phase[\s\S]{0,2500}footprint/i, "docs name the footprint rule");
  assert.match(architecture, /deployment phase[\s\S]{0,2500}overlap/i, "docs name the no-overlap rule");
  assert.match(
    architecture,
    /deployment phase[\s\S]{0,2500}(?:rotat|facing)[\s\S]{0,600}frontage|deployment phase[\s\S]{0,2500}frontage[\s\S]{0,600}(?:rotat|facing)/i,
    "docs apply the rule to facing and frontage edits",
  );
  assert.match(architecture, /deployment phase[\s\S]{0,3000}withdraw/i, "docs state that Withdraw is allowed during deployment");
});

test("battle page offers a Deployment region with a Start battle button", () => {
  assert.match(html, /aria-label="Deployment"|aria-labelledby="[^"]*deployment[^"]*"/i);
  assert.match(html, />\s*Start battle\s*</);
});

test("JavaScript does no deployment-zone geometry or legality checks", () => {
  // Zones may be drawn by the Rust renderer; the page never compares
  // coordinates against zone bounds or decides whether a placement is legal.
  for (const [name, source] of [["battle-sandbox.js", script], ["battle-input-bindings.js", bindings]]) {
    assert.doesNotMatch(source, /\b(?:min|max)[XY]Mm\b/, `${name} reads zone bounds`);
    assert.doesNotMatch(source, /deploymentZones/, `${name} reads deployment zones`);
    assert.doesNotMatch(source, /FormationFootprint|footprint/i, `${name} computes footprints`);
  }
});

test("JavaScript does not gate the simulation clock itself during deployment", () => {
  // Rust decides whether a frame runs ticks; the page keeps forwarding raw
  // frame time and must not hold frames back or fake a pause for deployment.
  assert.match(script, /battle_sandbox_frame\(timestamp\)/);
  assert.doesNotMatch(
    script,
    /if\s*\([^)]*deploy[^)]*\)[^;{]*battle_sandbox_frame|deploy[\w.]*\s*&&\s*battle_sandbox_frame/i,
    "the frame call is not conditional on a JavaScript deployment flag",
  );
  assert.doesNotMatch(script, /battle_sandbox_set_paused\(\s*true\s*\)/, "deployment is not a JavaScript pause");
});

test("deployment placement and Start battle are shared Rust tactical controls", () => {
  // shared/tactical_controls.rs is compiled into both web-battle-wasm and
  // src-tauri; deployment intents there are the same semantics on both surfaces.
  assert.match(sharedControls, /"deploySelected"\s*(?:\|[^=]*)?=>/, "shared controls accept deploySelected");
  assert.match(sharedControls, /"startBattle"\s*(?:\|[^=]*)?=>/, "shared controls accept startBattle");
  assert.match(sharedControls, /\.deploy_formation\(/, "deploySelected uses the core placement API");
  assert.match(sharedControls, /\.begin_battle\(/, "startBattle uses the core start API");
});

test("desktop battles open in deployment and the native window has a minimal Start control", () => {
  // Scoped default: desktop shares the Rust semantics and gets a minimal
  // Start control; a full desktop deployment editor is a follow-up.
  const desktop = `${nativeBattle}\n${nativeInput}`;
  const entries = nativeBattle.match(/\.into_deployment\(\)/g) ?? [];
  assert.ok(entries.length >= 2, "both the desktop sandbox and the desktop campaign battle enter deployment");
  assert.doesNotMatch(nativeBattle, /\.start\(\);/, "the desktop sandbox no longer starts at construction");
  assert.match(desktop, /"startBattle"|begin_battle\(/, "the native battle window can start the battle");
});
