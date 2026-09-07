import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

const index = await readFile(new URL("../web/index.html", import.meta.url), "utf8");
const script = await readFile(new URL("../web/main.js", import.meta.url), "utf8");

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

test("online battle exposes real lobby controls and a separate module", () => {
  assert.match(index, /id="host-battle"[^>]*>Host battle<\/button>/);
  assert.match(index, /id="join-battle"[^>]*>Join invite<\/button>/);
  assert.match(index, /id="lobby-code"/);
  assert.match(index, /id="start-battle"[^>]*disabled>Start battle<\/button>/);
  assert.match(index, /<script type="module" src="online-battle\.js"><\/script>/);
  assert.doesNotMatch(index, /Lobby controls are the next multiplayer implementation horizon/);
});

test("the future asset gate remains explicitly separate from lobby readiness", () => {
  assert.match(index, /Next networking slice/);
  assert.match(index, /Verify the trusted Medieval release manifest\./);
  assert.match(index, /Require the same verified content fingerprint on both computers\./);
  assert.match(index, /The match will remain locked until required content is verified on both computers\./);
});
