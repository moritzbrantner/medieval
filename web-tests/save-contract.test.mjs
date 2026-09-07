import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

const [html, saveScript, tauri, saveCore, platformCss] = await Promise.all([
  readFile(new URL("../web/index.html", import.meta.url), "utf8"),
  readFile(new URL("../web/save.js", import.meta.url), "utf8"),
  readFile(new URL("../src-tauri/src/lib.rs", import.meta.url), "utf8"),
  readFile(new URL("../crates/medieval-save/src/lib.rs", import.meta.url), "utf8"),
  readFile(new URL("../web/platform.css", import.meta.url), "utf8"),
]);

test("campaign saves use an explicit Rust-owned schema version", () => {
  assert.match(saveCore, /pub const SAVE_SCHEMA_VERSION: u32 = 1/);
  assert.match(saveCore, /pub struct CampaignSave/);
  assert.match(saveCore, /pub schema_version: u32/);
  assert.match(saveCore, /UnsupportedVersion/);
  assert.match(saveCore, /validate_campaign_save\(&save\)\?/);
});

test("Tauri owns fixed app-data save paths instead of exposing arbitrary filesystem access", () => {
  assert.match(tauri, /app_data_dir\(\)/);
  assert.match(tauri, /const MANUAL_SAVE_FILE: &str = "campaign-manual\.json"/);
  assert.match(tauri, /const AUTOSAVE_FILE: &str = "campaign-autosave\.json"/);
  assert.match(saveScript, /invoke\("save_campaign"\)/);
  assert.match(saveScript, /loadCampaignSave\("load_manual_campaign", "manual"\)/);
  assert.match(saveScript, /loadCampaignSave\("load_autosave_campaign", "autosave"\)/);
  assert.doesNotMatch(saveScript, /invoke\("save_campaign",\s*\{/);
  assert.doesNotMatch(saveScript, /filePath|savePath|directoryPath/);
});

test("completed player turns autosave transactionally before replacing authoritative session state", () => {
  assert.match(tauri, /let mut next_session = session\.clone\(\);/);
  assert.match(tauri, /persist_session\(&app, &next_session, AUTOSAVE_FILE\)\?;/);
  assert.match(tauri, /\*session = next_session;/);
});

test("manual save and both load slots are projected as semantic controls", () => {
  assert.match(html, /id="save-campaign"/);
  assert.match(html, /id="load-campaign"/);
  assert.match(html, /id="load-autosave"/);
  assert.match(html, /id="save-status"[^>]*aria-live="polite"/);
});

test("desktop focus and coarse-pointer touch acceptance remain explicit", () => {
  assert.match(platformCss, /button:focus-visible/);
  assert.match(platformCss, /select:focus-visible/);
  assert.match(platformCss, /@media \(pointer: coarse\)/);
  assert.match(platformCss, /min-height: 3rem/);
});
