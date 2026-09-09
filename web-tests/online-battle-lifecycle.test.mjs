import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

const controller = await readFile(new URL("../web/online-battle.js", import.meta.url), "utf8");

test("leaving during host or join setup discards a stale completed session", () => {
  assert.match(
    controller,
    /function discardSession\(current\)[\s\S]*commandChannels\.get\(current\)\?\.close\(\)[\s\S]*current\.close\(\)/,
  );
  assert.match(
    controller,
    /async function hostBattle\(\)[\s\S]*await current\.host\(2\);[\s\S]*if \(current !== session\) discardSession\(current\)/,
  );
  assert.match(
    controller,
    /async function joinBattle\(\)[\s\S]*await current\.join\(code\);[\s\S]*if \(current !== session\) discardSession\(current\)/,
  );
  assert.match(controller, /current\.addEventListener\("lobby", \(\) => \{\s*if \(current !== session\) return;/);
});

test("initial signaling failure closes the adopted session before surfacing failure", () => {
  const failureCleanup = /const active = current === session;\s*discardSession\(current\);\s*if \(active\) fail\(error\);/g;
  assert.equal([...controller.matchAll(failureCleanup)].length, 2);
});
