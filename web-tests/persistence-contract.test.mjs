import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

const [html, script, tauri, saveCore, persistenceCss] = await Promise.all([
  readFile(new URL("../web/index.html", import.meta.url), "utf8"),
  readFile(new URL("../web/main.js", import.meta.url), "utf8"),
  readFile(new URL("../src-tauri/src/lib.rs", import.meta.url), "utf8"),
  readFile(new URL("../crates/medieval-core/src/save.rs", import.meta.url), "utf8"),
  readFile(new URL("../web/persistence.css", import.meta.url), "utf8"),
]);

test("manual persistence is exposed through semantic controls with live status", () => {
  assert.match(html, /<button id="save-campaign"[^>]*type="button"/);
  assert.match(html, /<button id="load-campaign"[^>]*type="button"/);
  assert.match(html, /id="save-status"[^>]*aria-live="polite"/);
  assert.match(script, /invoke\("save_campaign"\)/);
  assert.match(script, /invoke\("load_campaign"\)/);
});

test("browser never chooses or receives a filesystem path", () => {
  assert.doesNotMatch(script, /save_campaign[^\n]*(path|fileName|filename|directory)/i);
  assert.doesNotMatch(script, /load_campaign[^\n]*(path|fileName|filename|directory)/i);
  assert.match(tauri, /const SAVE_FILE_NAME: &str = "campaign-save\.json"/);
  assert.match(tauri, /app\s*\.path\(\)\s*\.app_data_dir\(\)/s);
  assert.doesNotMatch(tauri, /fn save_campaign\([^)]*path:/s);
  assert.doesNotMatch(tauri, /fn load_campaign\([^)]*path:/s);
});

test("turn state is committed only after the authoritative autosave succeeds", () => {
  const turnCommand = tauri.slice(tauri.indexOf("fn end_player_turn("));
  const saveIndex = turnCommand.indexOf("save_session_to_path(&next, &path)?");
  const commitIndex = turnCommand.indexOf("*session = next;");

  assert.ok(saveIndex >= 0, "end_player_turn must persist its candidate state");
  assert.ok(commitIndex > saveIndex, "live state must change only after persistence succeeds");
});

test("failed load validates before replacing the live session", () => {
  const helper = tauri.slice(
    tauri.indexOf("fn load_into_session("),
    tauri.indexOf("fn write_save_document("),
  );
  const loadIndex = helper.indexOf("let loaded = load_session_from_path(path)?;");
  const commitIndex = helper.indexOf("*session = loaded;");

  assert.ok(loadIndex >= 0);
  assert.ok(commitIndex > loadIndex);
});

test("interrupted staged saves validate and recover their backup before load succeeds", () => {
  const loader = tauri.slice(
    tauri.indexOf("fn load_session_from_path("),
    tauri.indexOf("fn load_into_session("),
  );
  const backupReadIndex = loader.indexOf('path.with_extension("json.bak")');
  const validationIndex = loader.indexOf("decode_session_document(&backup_document)?");
  const recoveryIndex = loader.indexOf("fs::rename(&backup, path)");

  assert.ok(backupReadIndex >= 0, "missing primary saves must inspect the staged backup");
  assert.ok(validationIndex > backupReadIndex, "backup bytes must be Rust-validated");
  assert.ok(recoveryIndex > validationIndex, "only a validated backup may be recovered");
});

test("campaign transitions share one busy gate so stale orders cannot cross turn boundaries", () => {
  assert.match(script, /let campaignBusy = false;/);
  assert.match(script, /function setCampaignBusy\(busy\)/);
  assert.match(script, /button\.disabled = campaignBusy \|\| !option\.available;/);
  assert.match(script, /selectArmyButton\.disabled = campaignBusy \|\|/);
  assert.match(script, /button\.disabled = campaignBusy;/);

  const turnCommand = script.slice(
    script.indexOf("async function endPlayerTurn()"),
    script.indexOf("async function startNewCampaign()"),
  );
  const busyIndex = turnCommand.indexOf("setCampaignBusy(true);");
  const invokeIndex = turnCommand.indexOf('invoke("end_player_turn"');
  const releaseIndex = turnCommand.lastIndexOf("setCampaignBusy(false);");

  assert.ok(busyIndex >= 0 && busyIndex < invokeIndex, "end turn must close the gate before invoking Rust");
  assert.ok(releaseIndex > invokeIndex, "end turn must keep the gate closed through UI refresh");

  for (const action of ["moveSelectedArmy", "queueRecruitment", "resolvePendingBattle"]) {
    const start = script.indexOf(`async function ${action}`);
    const next = script.indexOf("\nasync function ", start + 1);
    const body = script.slice(start, next >= 0 ? next : undefined);
    assert.match(body, /campaignBusy\) return;/, `${action} must reject stale input while busy`);
  }
});

test("save schema version and validation remain Rust-owned", () => {
  assert.match(saveCore, /pub const CAMPAIGN_SAVE_SCHEMA_VERSION: u32 = 1/);
  assert.match(saveCore, /pub fn from_json\(json: &str\) -> Result<Self, SaveError>/);
  assert.match(saveCore, /pub fn validate\(&self\) -> Result<\(\), SaveError>/);
  assert.match(saveCore, /UnsupportedVersion/);
  assert.doesNotMatch(script, /schemaVersion|JSON\.stringify\(campaign|JSON\.parse\([^)]*campaign/);
});

test("keyboard focus remains visible for native campaign controls", () => {
  assert.match(persistenceCss, /button:focus-visible/);
  assert.match(persistenceCss, /input:focus-visible/);
  assert.match(persistenceCss, /select:focus-visible/);
  assert.match(persistenceCss, /outline:\s*2px solid/);
});
