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

test("save schema version and validation remain Rust-owned", () => {
  assert.match(saveCore, /pub const CAMPAIGN_SAVE_SCHEMA_VERSION: u32 = 1/);
  assert.match(saveCore, /pub fn from_json\(json: &str\) -> Result<Self, SaveError>/);
  assert.match(saveCore, /pub fn validate\(&self\) -> Result<\(\), SaveError>/);
  assert.match(saveCore, /UnsupportedVersion/);
  assert.doesNotMatch(script, /schemaVersion|JSON\.stringify\(campaign|JSON\.parse\([^)]*campaign/);
});

test("mobile campaign persistence keeps single-column controls and touch targets", () => {
  assert.match(persistenceCss, /@media \(max-width: 560px\)/);
  assert.match(persistenceCss, /\.save-actions\s*\{\s*grid-template-columns: 1fr;/s);
  assert.match(persistenceCss, /\.save-actions button[\s\S]*min-height: 44px/);
});
