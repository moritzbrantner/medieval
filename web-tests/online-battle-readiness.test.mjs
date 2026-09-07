import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { readFile } from "node:fs/promises";
import test from "node:test";
import {
  MEDIEVAL_BATTLE_PROTOCOL,
  MEDIEVAL_RELEASE,
  ONLINE_BATTLE_STATES,
  buildInviteUrl,
  canStartBattle,
  createReadinessMessage,
  inspectReadinessMessage,
  lobbyCodeFromUrl,
} from "../web/online-battle-model.mjs";

const controller = await readFile(new URL("../web/online-battle.js", import.meta.url), "utf8");
const cargo = await readFile(new URL("../Cargo.toml", import.meta.url), "utf8");
const lobbySession = await readFile(
  new URL("../web/vendor/multiplayer-setup-service/lobby-session.js", import.meta.url),
  "utf8",
);
const resilientLobbySession = await readFile(
  new URL("../web/vendor/multiplayer-setup-service/resilient-lobby-session.js", import.meta.url),
  "utf8",
);
const tauriConfig = JSON.parse(await readFile(new URL("../src-tauri/tauri.conf.json", import.meta.url), "utf8"));

function gitBlobSha(content) {
  const bytes = Buffer.from(content, "utf8");
  return createHash("sha1")
    .update(`blob ${bytes.byteLength}\0`)
    .update(bytes)
    .digest("hex");
}

test("Medieval owns a release and battle-protocol readiness contract", () => {
  assert.match(cargo, new RegExp(`version = "${MEDIEVAL_RELEASE.replaceAll(".", "\\.")}"`));
  assert.equal(MEDIEVAL_BATTLE_PROTOCOL, 1);
  assert.deepEqual(ONLINE_BATTLE_STATES, [
    "idle",
    "creating",
    "joining",
    "connecting",
    "peer-connected",
    "release-check",
    "ready",
    "recovery",
    "failure",
  ]);
});

test("compatible readiness enables the battle gate only after the direct peer is connected", () => {
  const peerReadiness = inspectReadinessMessage(createReadinessMessage({ ready: true }));
  assert.equal(peerReadiness.recognized, true);
  assert.equal(peerReadiness.compatible, true);
  assert.equal(peerReadiness.ready, true);
  assert.equal(canStartBattle({ peerConnected: true, localReady: true, peerReadiness }), true);
  assert.equal(canStartBattle({ peerConnected: false, localReady: true, peerReadiness }), false);
  assert.equal(canStartBattle({ peerConnected: true, localReady: false, peerReadiness }), false);
});

test("release or battle-protocol mismatches fail closed", () => {
  const releaseMismatch = inspectReadinessMessage({
    ...createReadinessMessage(),
    release: "9.9.9",
  });
  const protocolMismatch = inspectReadinessMessage({
    ...createReadinessMessage(),
    battleProtocol: MEDIEVAL_BATTLE_PROTOCOL + 1,
  });

  assert.equal(releaseMismatch.compatible, false);
  assert.match(releaseMismatch.reason, /Release mismatch/);
  assert.equal(protocolMismatch.compatible, false);
  assert.match(protocolMismatch.reason, /Battle protocol mismatch/);
  assert.equal(canStartBattle({ peerConnected: true, localReady: true, peerReadiness: releaseMismatch }), false);
  assert.equal(canStartBattle({ peerConnected: true, localReady: true, peerReadiness: protocolMismatch }), false);
});

test("unrecognized data-channel traffic never becomes readiness", () => {
  const result = inspectReadinessMessage({ type: "battle-command", v: 1, ready: true });
  assert.equal(result.recognized, false);
  assert.equal(canStartBattle({ peerConnected: true, localReady: true, peerReadiness: result }), false);
});

test("invite links contain only the public lobby identity", () => {
  const invite = buildInviteUrl(
    "https://example.test/medieval/?setupApi=https%3A%2F%2Fsetup.example&participantToken=secret&view=online#private",
    "0123-ABCD-EFGH",
  );
  const url = new URL(invite);

  assert.deepEqual([...url.searchParams.entries()], [["lobby", "0123-ABCD-EFGH"]]);
  assert.equal(url.hash, "");
  assert.equal(invite.includes("secret"), false);
  assert.equal(invite.includes("setupApi"), false);
  assert.equal(lobbyCodeFromUrl(invite), "0123-ABCD-EFGH");
});

test("the controller delegates private identity and recovery to LobbySession", () => {
  assert.match(controller, /new LobbySession\(/);
  assert.match(controller, /await current\.host\(2\)/);
  assert.match(controller, /await current\.join\(code\)/);
  assert.match(controller, /contentSharing:\s*false/);
  assert.match(controller, /readinessSendPending/);
  assert.match(controller, /channel-close/);
  assert.match(controller, /peer-recovery/);
  assert.match(controller, /signaling-closed/);
  assert.doesNotMatch(controller, /participantToken/);
  assert.doesNotMatch(controller, /broadcastRealtime|sendRealtime/);
  assert.equal(controller.split("sendReliable(").length - 1, 1);
});

test("the desktop CSP allows rendezvous connections while scripts stay self-hosted", () => {
  const csp = tauriConfig.app.security.csp;
  assert.match(csp, /script-src 'self'/);
  assert.match(csp, /connect-src[^;]*https:/);
  assert.match(csp, /connect-src[^;]*wss:/);
  assert.match(csp, /http:\/\/127\.0\.0\.1:8787/);
});

test("the vendored LobbySession is an exact pinned multiplayer-setup-service source copy", () => {
  assert.equal(gitBlobSha(lobbySession), "4cc08e8ee9e10e68d33a4198033d74675236760b");
  assert.equal(gitBlobSha(resilientLobbySession), "6f9e7e425f88d896915719a8c1d38828d8559e26");
});
