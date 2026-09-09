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

test("recruitment completion timing is projected from campaign state", () => {
  assert.match(script, /campaign\.recruitmentQueue/);
  assert.match(script, /order\.readyOnTurn/);
  assert.doesNotMatch(script, /readyOnTurn\s*=/);
});
