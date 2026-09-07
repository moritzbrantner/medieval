import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

const [html, script] = await Promise.all([
  readFile(new URL("../web/index.html", import.meta.url), "utf8"),
  readFile(new URL("../web/main.js", import.meta.url), "utf8"),
]);

test("movement destinations come from the Rust command surface", () => {
  assert.match(script, /invoke\("legal_army_destinations", \{ armyId \}\)/);
  assert.match(script, /legalDestinationIds = await invoke/);
  assert.doesNotMatch(script, /neighbors\.includes\(/);
  assert.doesNotMatch(script, /neighbors\.some\(/);
});

test("movement sends an intent instead of mutating campaign state in the browser", () => {
  assert.match(script, /invoke\("move_army", \{ armyId, destination \}\)/);
  assert.doesNotMatch(script, /\.province\s*=\s*destination/);
  assert.doesNotMatch(script, /pendingBattle\s*=/);
});

test("pending battle state is projected explicitly", () => {
  assert.match(html, /id="pending-battle"/);
  assert.match(html, /id="pending-battle-detail"/);
  assert.match(script, /const battle = campaign\.pendingBattle/);
  assert.match(script, /Combat is intentionally unresolved/);
});
