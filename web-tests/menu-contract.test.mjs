import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

const index = await readFile(new URL("../web/index.html", import.meta.url), "utf8");
const script = await readFile(new URL("../web/main.js", import.meta.url), "utf8");
const nativeBattleScript = await readFile(new URL("../web/native-battle.js", import.meta.url), "utf8");

function occurrenceCount(haystack, needle) {
  return haystack.split(needle).length - 1;
}

test("the application shell exposes campaign and online battle as sibling modes", () => {
  assert.match(index, /id="menu-view"/);
  assert.match(index, /id="campaign-view"/);
  assert.match(index, /id="online-view"/);
  assert.match(index, />Campaign</);
  assert.match(index, />Online Battle</);
  assert.match(script, /showView\("menu"\);\s*$/);
});

test("campaign state is loaded lazily after the campaign mode is selected", () => {
  assert.equal(occurrenceCount(script, 'invoke("campaign_state")'), 1);
  assert.match(script, /async function ensureCampaignLoaded\(\)[\s\S]*invoke\("campaign_state"\)/);
  assert.match(script, /campaignButton\.addEventListener\("click"[\s\S]*ensureCampaignLoaded\(\)/);
});

test("online battle preview does not expose fake host or join controls", () => {
  assert.match(index, /<button type="button" disabled>Host battle<\/button>/);
  assert.match(index, /<button type="button" class="secondary" disabled>Join invite<\/button>/);
  assert.match(index, /The game will not pretend to be connected until that integration is implemented\./);
});

test("native renderer preview delegates to Tauri without duplicating battle rules", () => {
  assert.match(index, /id="open-native-battle"/);
  assert.match(nativeBattleScript, /invoke\("open_native_battle_renderer"\)/);
  assert.doesNotMatch(nativeBattleScript, /TacticalBattle|advance_ticks|casualt|morale|frontage|formation/);
});

test("the future battle gate explicitly waits for verified content on both peers", () => {
  assert.match(index, /Verify the trusted Medieval release manifest\./);
  assert.match(index, /Enable Start battle only when both players report the same release and required assets ready\./);
  assert.match(index, /The match remains locked until required content is verified on both computers\./);
});
