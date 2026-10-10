// Source contracts for issue #94 (tactical deployment phase). The browser
// journeys in e2e/tactical-deployment.spec.mjs prove behaviour; these checks
// keep deployment-zone legality and the phase gate in Rust, keep the browser
// and desktop on the shared Rust controls, and keep the contract documented.
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

const read = (path) => readFile(new URL(`../${path}`, import.meta.url), "utf8");
const [html, script, bindings, sharedControls, architecture] = await Promise.all([
  read("web/battle.html"),
  read("web/battle-sandbox.js"),
  read("web/battle-input-bindings.js"),
  read("shared/tactical_controls.rs"),
  read("docs/BATTLE-ARCHITECTURE.md"),
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

test("deployment placement is a shared Rust tactical control, so browser and desktop share it", () => {
  // shared/tactical_controls.rs is compiled into both web-battle-wasm and
  // src-tauri; a deployment intent there is the same semantics on both surfaces.
  assert.match(
    sharedControls,
    /"[a-zA-Z]*[dD]eploy[a-zA-Z]*"\s*(?:\|[^=]*)?=>/,
    "shared tactical controls accept a deployment request kind",
  );
});
