import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

const script = await readFile(new URL("../web/main.js", import.meta.url), "utf8");

test("recruitment options and rejection reasons come from Rust", () => {
  assert.match(
    script,
    /invoke\("recruitment_options", \{\s*provinceId: requestedProvinceId,?\s*\}\)/,
  );
  assert.match(script, /const requestedProvinceId = selectedProvinceId/);
  assert.match(script, /const requestId = \+\+recruitmentRequestId/);
  assert.match(
    script,
    /requestId !== recruitmentRequestId \|\|\s*requestedProvinceId !== selectedProvinceId/,
  );
  assert.match(script, /button\.disabled = campaignBusy \|\| !option\.available/);
  assert.match(script, /reason\.textContent = option\.reason/);
  assert.doesNotMatch(script, /unitCosts|recruitmentCosts|costByUnit/);
});

test("queueing recruitment sends an intent rather than mutating treasury or armies", () => {
  assert.match(
    script,
    /invoke\("queue_recruitment", \{ provinceId, unit \}\)/,
  );
  assert.doesNotMatch(script, /\.treasury\s*[-+]?=/);
  assert.doesNotMatch(script, /\.levy\s*[-+]?=/);
  assert.doesNotMatch(script, /\.spearmen\s*[-+]?=/);
  assert.doesNotMatch(script, /\.archers\s*[-+]?=/);
  assert.doesNotMatch(script, /\.knights\s*[-+]?=/);
});

test("unit unlocks and recruitment pools are projected from Rust", () => {
  assert.match(script, /option\.unlocked/);
  assert.match(script, /option\.pool\}\/\$\{option\.poolCapacity\}/);
  assert.match(script, /option\.unlockLabel/);
  assert.doesNotMatch(script, /recruitmentPool\s*[-+]?=(?!=)|poolCapacity\s*[-+]?=(?!=)/);
  assert.doesNotMatch(script, /unitUnlocks|unlockByUnit|requiresBarracks/);
});

test("recruitment completion timing is projected from campaign state", () => {
  assert.match(script, /campaign\.recruitmentQueue/);
  assert.match(script, /order\.readyOnTurn/);
  assert.doesNotMatch(script, /readyOnTurn\s*=/);
});

test("settlement levels, effects, and upgrade rules come from Rust", () => {
  assert.match(
    script,
    /invoke\("settlement_upgrade_option", \{ provinceId: requestedProvinceId \}\)/,
  );
  assert.match(script, /invoke\("queue_settlement_upgrade", \{ provinceId \}\)/);
  assert.match(script, /button\.disabled = campaignBusy \|\| !settlementOption\.available/);
  assert.match(script, /reason\.textContent = settlementOption\.reason/);
  assert.match(script, /current\.incomeBonus/);
  assert.match(script, /target\.upgrade\.cost/);
  assert.match(script, /target\.upgrade\.rounds/);
  assert.match(script, /settlementOption\.readyOnTurn/);
  assert.doesNotMatch(script, /settlementLevel\s*=(?!=)|incomeBonus\s*[-+]?=(?!=)|buildingSlots\s*[-+]?=(?!=)/);
  assert.doesNotMatch(script, /upgradeCosts|levelCosts|costByLevel/);
});

test("buildings, prerequisites, and construction rules come from Rust", () => {
  assert.match(
    script,
    /invoke\("construction_options", \{ provinceId: requestedProvinceId \}\)/,
  );
  assert.match(script, /invoke\("queue_construction", \{ provinceId, building \}\)/);
  assert.match(script, /button\.disabled = campaignBusy \|\| !option\.available/);
  assert.match(script, /construction\.buildingSlots/);
  assert.match(script, /option\.target\.cost/);
  assert.match(script, /option\.readyOnTurn/);
  assert.doesNotMatch(script, /\.buildings\s*(=(?!=)|\.push)|usedSlots\s*[-+]?=(?!=)|currentLevel\s*[-+]?=(?!=)/);
  assert.doesNotMatch(script, /buildingCosts|costByBuilding|prerequisites/);
});

test("siege presentation follows the Rust battle seed, not province fields", () => {
  assert.match(script, /invoke\("pending_tactical_battle_seed"\)/);
  assert.match(script, /profile\?\.kind === "siege" \? profile\.fortification : null/);
  assert.doesNotMatch(script, /battlefield\?\.fortified/);
  assert.match(script, /invoke\("load_campaign"\);\n\s*resetPendingSiegeProfile\(\);/);
  assert.match(script, /invoke\("start_new_campaign"[^\n]*\n\s*resetPendingSiegeProfile\(\);/);
  assert.doesNotMatch(script, /Walls[^\n]*siege|palisadeV1|stoneWallsV1/);
});
