// Source contracts for issue #96 (tactical battle HUD). The browser journey in
// e2e/tactical-hud.spec.mjs proves behaviour; these checks keep scheduling and
// battle rules out of JavaScript and keep the speed contract documented.
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

const [html, script, architecture] = await Promise.all([
  readFile(new URL("../web/battle.html", import.meta.url), "utf8"),
  readFile(new URL("../web/battle-sandbox.js", import.meta.url), "utf8"),
  readFile(new URL("../docs/BATTLE-ARCHITECTURE.md", import.meta.url), "utf8"),
]);

const MULTIPLIERS = ["0.5", "1", "2", "4"];

test("simulation speed multipliers are documented as tick scheduling only", () => {
  assert.match(architecture, /simulation speed/i);
  for (const multiplier of MULTIPLIERS) {
    assert.match(
      architecture,
      new RegExp(`(?<![\\d.])${multiplier.replace(".", "\\.")}\\s?[×x]`),
      `docs/BATTLE-ARCHITECTURE.md documents the ${multiplier}× multiplier`,
    );
  }
  assert.match(
    architecture,
    /(?:speed|pause)[\s\S]{0,600}tick scheduling|tick scheduling[\s\S]{0,600}(?:speed|pause)/i,
    "docs state that pause/speed change tick scheduling, not simulation truth",
  );
});

test("battle page exposes the documented speed multipliers", () => {
  assert.match(html, /Simulation speed/);
  for (const multiplier of MULTIPLIERS) {
    assert.match(
      html,
      new RegExp(`(?<![\\d.])${multiplier.replace(".", "\\.")}\\s?[×x]`),
      `battle.html offers ${multiplier}×`,
    );
  }
});

test("JavaScript forwards raw frame time; Rust owns tick scheduling under pause and speed", () => {
  // The rAF timestamp reaches Rust unscaled; speed is Rust sandbox state, like pause.
  assert.match(script, /battle_sandbox_frame\(timestamp\)/);
  assert.doesNotMatch(script, /battle_sandbox_frame\([^)]*[*/]/);
  assert.doesNotMatch(script, /TICKS_PER_SECOND|ticksPerSecond|tick_?[aA]ccumulator|pendingTicks/);
  assert.doesNotMatch(script, /advance_ticks|advanceTicks/);
});

test("terminal HUD text distinguishes a siege capture from a defeated force", () => {
  // TacticalFinishReason::SiegeCapture must not be reported as
  // "the opposing force can no longer fight".
  assert.match(script, /siegeCapture/);
});

test("battle interface carries no developer diagnostic panel", () => {
  assert.doesNotMatch(html, /class="runtime-note"/);
  assert.doesNotMatch(html, /JavaScript only adapts/);
});
