// Source contracts for issue #127 (staged siege objectives). Core
// (crates/medieval-core/tests/siege_staged_objectives.rs) proves the rules and
// the renderer tests prove projection; these checks keep objective state
// flowing from core into the browser HUD and the victory rule documented.
//
// Seams: the WASM sandbox status gains `siegeObjectives`, built from core
// `TacticalBattle::siege_objectives()` (one entry per objective, core order,
// with `progress`, `capturingSide`, `capturedBy`, `unlocked`), and the battle
// page gains a `#siege-objectives` list rendered from it.
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

const [html, script, wasm, architecture] = await Promise.all([
  readFile(new URL("../web/battle.html", import.meta.url), "utf8"),
  readFile(new URL("../web/battle-sandbox.js", import.meta.url), "utf8"),
  readFile(new URL("../web-battle-wasm/src/lib.rs", import.meta.url), "utf8"),
  readFile(new URL("../docs/BATTLE-ARCHITECTURE.md", import.meta.url), "utf8"),
]);

test("the sandbox status projects core objective state", () => {
  assert.match(wasm, /siege_objectives\(\)/, "status is built from TacticalBattle::siege_objectives");
  assert.match(wasm, /siege_objectives\s*:/, "SandboxStatus carries siegeObjectives");
});

test("the battle HUD lists every objective with its state", () => {
  assert.match(html, /id="siege-objectives"/, "battle.html has an objective list");
  assert.match(script, /#siege-objectives/);
  assert.match(script, /status\.siegeObjectives/, "the HUD renders status.siegeObjectives");
  for (const field of ["unlocked", "capturedBy", "capturingSide", "progress"]) {
    assert.match(script, new RegExp(`\\.${field}\\b`), `the HUD shows objective ${field}`);
  }
});

test("the HUD never decides capture", () => {
  assert.doesNotMatch(script, /SIEGE_CAPTURE_PROGRESS_PER_PULSE|progressPerPulse/i);
  assert.doesNotMatch(script, /progress\s*\+=|progress\s*=\s*[^;]*\+/);
});

test("the staged objective victory rule is documented", () => {
  const section = architecture.match(/^## Staged siege objectives[\s\S]*?(?=^## |(?![\s\S]))/m);
  assert.ok(section, "docs/BATTLE-ARCHITECTURE.md has a 'Staged siege objectives' section");
  const text = section[0];
  assert.match(text, /siege_objectives/);
  assert.match(text, /StagedKeepV1/);
  assert.match(text, /final objective/i, "victory names the final objective");
  assert.match(text, /unlock/i, "the ordered unlock rule is documented");
  assert.match(text, /contested/i, "contested capture is documented per objective");
  assert.match(text, /single-point|one objective/i, "single-point compatibility is documented");
});
