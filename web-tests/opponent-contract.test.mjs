import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

const [html, script, tauri, battle] = await Promise.all([
  readFile(new URL("../web/index.html", import.meta.url), "utf8"),
  readFile(new URL("../web/main.js", import.meta.url), "utf8"),
  readFile(new URL("../src-tauri/src/lib.rs", import.meta.url), "utf8"),
  readFile(new URL("../crates/medieval-core/src/battle.rs", import.meta.url), "utf8"),
]);

test("new campaign flow offers both factions and delegates selection to Rust", () => {
  assert.match(html, /id="player-faction"/);
  assert.match(html, /value="england"/);
  assert.match(html, /value="france"/);
  assert.match(
    script,
    /invoke\("start_new_campaign", \{ playerFaction: requestedFaction \}\)/,
  );
  assert.match(tauri, /select_player_faction\(&player_faction\)/);
});

test("campaign victory is queried from Rust and only projected by the browser", () => {
  assert.match(html, /id="campaign-outcome"/);
  assert.match(script, /invoke\("campaign_winner"\)/);
  assert.match(tauri, /session\.campaign\.winner\(\)/);
  assert.match(battle, /pub fn winner\(&self\) -> Option<String>/);
  assert.doesNotMatch(script, /provinces\.every\(/);
  assert.doesNotMatch(script, /armies\.some\([^)]*winner/i);
});

test("ending a player turn supplies an explicit deterministic seed to Rust AI", () => {
  assert.match(script, /const seed = campaign\.turn/);
  assert.match(script, /invoke\("end_player_turn", \{ seed \}\)/);
  assert.match(tauri, /play_ai_turn\(&player_faction, seed\)/);
  assert.doesNotMatch(script, /Math\.random/);
  assert.doesNotMatch(script, /invoke\("end_turn"/);
});

test("Rust opponent reuses the same legal command surfaces as the human", () => {
  assert.match(battle, /self\s*\.recruitment_options\(&province_id\)\?/);
  assert.match(battle, /self\s*\.queue_recruitment\(&province_id, options\[option_index\]\)\?/);
  assert.match(battle, /self\s*\.legal_destinations\(&army_id\)\?/);
  assert.match(battle, /self\s*\.move_army\(army_id, destination\)\?/);
  assert.match(battle, /self\s*\.resolve_pending_battle\(/);
  assert.match(battle, /self\s*\.end_turn\(\)\?/);
});

test("AI strategic scoring and victory rules are absent from JavaScript", () => {
  assert.doesNotMatch(script, /AI_ATTACK_SCORE|AI_REINFORCE_SCORE|AI_FRIENDLY_MOVE_SCORE/);
  assert.doesNotMatch(script, /strategicScore|strategic_score/);
  assert.doesNotMatch(script, /hash_text|tie_break/);
});
