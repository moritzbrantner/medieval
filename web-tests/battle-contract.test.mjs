import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

const [html, script] = await Promise.all([
  readFile(new URL("../web/index.html", import.meta.url), "utf8"),
  readFile(new URL("../web/main.js", import.meta.url), "utf8"),
]);

test("battle resolution is an explicit seeded Rust command", () => {
  assert.match(html, /id="battle-seed"/);
  assert.match(html, /id="resolve-battle"/);
  assert.match(script, /const seed = Number\(battleSeedInput\.value\)/);
  assert.match(script, /Number\.isSafeInteger\(seed\)/);
  assert.match(script, /invoke\("resolve_pending_battle", \{ seed \}\)/);
  assert.doesNotMatch(script, /Number\.parseInt/);
  assert.doesNotMatch(script, /Math\.random/);
});

test("the browser projects persisted Rust battle reports", () => {
  assert.match(script, /campaign\.battleReports\?\.at\(-1\)/);
  assert.match(script, /report\.attackerScore/);
  assert.match(script, /report\.defenderScore/);
  assert.match(script, /report\.attackerCasualtyPercent/);
  assert.match(script, /report\.defenderCasualtyPercent/);
  assert.match(script, /report\.captured/);
  assert.match(script, /report\.defenderRetreatProvince/);
});

test("combat weights and casualty arithmetic are absent from JavaScript", () => {
  assert.doesNotMatch(script, /knights\s*\*\s*5/);
  assert.doesNotMatch(script, /spearmen\s*\*\s*2/);
  assert.doesNotMatch(script, /archers\s*\*\s*2/);
  assert.doesNotMatch(script, /casualt(?:y|ies).*\//i);
  assert.doesNotMatch(script, /defenderModifierPercent\s*=/);
});
