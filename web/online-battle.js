import { LobbySession } from "./vendor/multiplayer-setup-service/lobby-session.js";
import {
  MEDIEVAL_BATTLE_PROTOCOL,
  MEDIEVAL_RELEASE,
  ONLINE_BATTLE_STATES,
  buildInviteUrl,
  canStartBattle,
  createReadinessMessage,
  inspectReadinessMessage,
  lobbyCodeFromUrl,
  normalizeLobbyCode,
} from "./online-battle-model.mjs";

const DEFAULT_SETUP_API = "http://127.0.0.1:8787";
const stateSet = new Set(ONLINE_BATTLE_STATES);

const hostButton = document.querySelector("#host-battle");
const joinButton = document.querySelector("#join-battle");
const lobbyCodeInput = document.querySelector("#lobby-code");
const inviteDetails = document.querySelector("#lobby-invite");
const displayCode = document.querySelector("#lobby-display-code");
const inviteLink = document.querySelector("#invite-link");
const copyInviteButton = document.querySelector("#copy-invite");
const copyNotice = document.querySelector("#copy-notice");
const leaveButton = document.querySelector("#leave-lobby");
const startButton = document.querySelector("#start-battle");
const statusBox = document.querySelector("#online-status");
const statusLabel = document.querySelector("#online-status-label");
const statusTitle = document.querySelector("#online-status-title");
const statusDetail = document.querySelector("#online-status-detail");
const localRelease = document.querySelector("#local-release");
const peerRelease = document.querySelector("#peer-release");
const battleGateNote = document.querySelector("#battle-gate-note");

const stateCopy = {
  idle: ["Idle", "Host a private battle or join a friend's invite."],
  creating: ["Creating lobby", "Requesting a private two-player lobby."],
  joining: ["Joining lobby", "Claiming your own private participant capability."],
  connecting: ["Connecting", "Establishing the direct WebRTC peer link."],
  "peer-connected": ["Peer connected", "The reliable direct channel is open."],
  "release-check": ["Release check", "Comparing Medieval release and battle-protocol compatibility."],
  ready: ["Ready", "Both players report a compatible release and battle protocol."],
  recovery: ["Recovery", "Reusing the current participant identity while signaling or ICE recovers."],
  failure: ["Failure", "The private battle setup could not continue."],
};

let session = null;
let currentPeerId = null;
let localReady = false;
let readinessSendPending = false;
let peerReadiness = null;
let currentState = "idle";
let intentionalClose = false;

function configuredMultiplayer() {
  const configured = window.__MEDIEVAL_MULTIPLAYER__;
  return configured && typeof configured === "object" ? configured : {};
}

function setupApiBase() {
  const queryOverride = new URLSearchParams(window.location.search).get("setupApi");
  return queryOverride || configuredMultiplayer().apiBase || DEFAULT_SETUP_API;
}

function configuredIceServers(name) {
  const value = configuredMultiplayer()[name];
  return Array.isArray(value) ? value : [];
}

function setState(state, detail) {
  if (!stateSet.has(state)) throw new Error(`Unknown Online Battle state: ${state}`);
  currentState = state;
  const [label, title] = stateCopy[state];
  statusBox.dataset.state = state;
  statusLabel.textContent = label;
  statusTitle.textContent = title;
  statusDetail.textContent = detail ?? "";
}

function setSetupControlsDisabled(disabled) {
  hostButton.disabled = disabled;
  joinButton.disabled = disabled;
  lobbyCodeInput.disabled = disabled;
}

function resetReadiness() {
  currentPeerId = null;
  localReady = false;
  readinessSendPending = false;
  peerReadiness = null;
  startButton.disabled = true;
  peerRelease.textContent = "Waiting for friend…";
  battleGateNote.textContent = "This slice stops at compatibility/readiness. No battle simulation traffic is sent.";
}

function resetLobbyPresentation() {
  inviteDetails.hidden = true;
  displayCode.textContent = "";
  inviteLink.textContent = "";
  inviteLink.removeAttribute("href");
  copyNotice.textContent = "";
  leaveButton.disabled = true;
}

function presentLobby(code) {
  const url = buildInviteUrl(window.location.href, code);
  displayCode.textContent = code;
  inviteLink.href = url;
  inviteLink.textContent = url;
  inviteDetails.hidden = false;
  leaveButton.disabled = false;
}

function renderPeerReadiness(readiness) {
  if (!readiness?.recognized || !readiness.fingerprint) {
    peerRelease.textContent = "Waiting for friend…";
    return;
  }
  const { release, battleProtocol } = readiness.fingerprint;
  const readinessLabel = readiness.ready ? "ready" : "not ready";
  peerRelease.textContent = `Medieval ${release} · battle protocol ${battleProtocol} · ${readinessLabel}`;
}

function updateStartGate() {
  const peerConnected = Boolean(
    session && currentPeerId && session.readyPeerIds().includes(currentPeerId),
  );
  const startable = canStartBattle({ peerConnected, localReady, peerReadiness });
  startButton.disabled = !startable;
  if (startable) {
    setState("ready", "The pre-battle release gate passed on both peers.");
    battleGateNote.textContent = "Readiness gate passed. Starting tactical simulation is deliberately outside this slice.";
  }
}

function closeSession({ keepInviteCode = false } = {}) {
  intentionalClose = true;
  session?.close();
  session = null;
  intentionalClose = false;
  resetReadiness();
  resetLobbyPresentation();
  setSetupControlsDisabled(false);
  if (!keepInviteCode) lobbyCodeInput.value = lobbyCodeFromUrl(window.location.href) ?? "";
  setState("idle", lobbyCodeInput.value ? "Invite loaded. Join when you are ready." : "");
}

function fail(error) {
  resetReadiness();
  setSetupControlsDisabled(false);
  setState("failure", error instanceof Error ? error.message : String(error));
}

function beginReleaseCheck(current, peerId) {
  if (current !== session || !current.readyPeerIds().includes(peerId)) return;
  if (currentPeerId === peerId && (localReady || readinessSendPending)) {
    updateStartGate();
    return;
  }

  currentPeerId = peerId;
  localReady = false;
  readinessSendPending = true;
  peerReadiness = null;
  startButton.disabled = true;
  renderPeerReadiness(null);
  setState("peer-connected", "Direct channels are ready; Medieval owns the compatibility decision next.");

  window.requestAnimationFrame(() => {
    if (current !== session || currentPeerId !== peerId || !current.readyPeerIds().includes(peerId)) {
      readinessSendPending = false;
      return;
    }
    setState(
      "release-check",
      `Checking Medieval ${MEDIEVAL_RELEASE} against your friend before battle readiness is accepted.`,
    );
    try {
      current.sendReliable(peerId, createReadinessMessage({ ready: true }));
      localReady = true;
      readinessSendPending = false;
      updateStartGate();
    } catch (error) {
      readinessSendPending = false;
      fail(error);
    }
  });
}

function enterRecovery(detail) {
  if (!session || intentionalClose) return;
  resetReadiness();
  setState("recovery", detail);
}

function wireSession(current) {
  current.addEventListener("lobby", () => {
    lobbyCodeInput.value = current.displayCode;
    presentLobby(current.displayCode);
    setState("connecting", `Private lobby ${current.displayCode} is active. Waiting for the direct peer link.`);
  });

  current.addEventListener("roster", (event) => {
    const participants = event.detail.participants ?? [];
    if (participants.length < 2 && currentState !== "recovery") {
      setState("connecting", "Private lobby is ready. Waiting for your friend to join.");
    }
  });

  current.addEventListener("participant-connected", () => {
    setState("connecting", "Your friend joined. Establishing the direct WebRTC peer link.");
  });

  current.addEventListener("participant-disconnected", () => {
    enterRecovery("Your friend disconnected. Waiting for the same lobby participant to reconnect.");
  });

  current.addEventListener("peer-ready", (event) => beginReleaseCheck(current, event.detail.peerId));

  current.addEventListener("peer-statechange", (event) => {
    if (current !== session || intentionalClose) return;
    if (event.detail.state === "failed" || event.detail.state === "disconnected") {
      enterRecovery(`Direct peer transport is ${event.detail.state}; the existing session will recover directly or restart ICE if needed.`);
      return;
    }
    if (event.detail.state === "connected" && current.readyPeerIds().includes(event.detail.peerId)) {
      beginReleaseCheck(current, event.detail.peerId);
    }
  });

  current.addEventListener("channel-close", (event) => {
    if (current !== session || intentionalClose) return;
    if (event.detail.kind !== "reliable" && event.detail.kind !== "realtime") return;
    resetReadiness();
    setState(
      "failure",
      `Direct ${event.detail.kind} channel closed. The battle gate is locked until a healthy peer session is established again.`,
    );
  });

  current.addEventListener("peer-recovery", (event) => {
    const { action, attempt } = event.detail;
    enterRecovery(`ICE recovery ${action} attempt ${attempt} is in progress with the existing participant identity.`);
  });

  current.addEventListener("peer-recovery-exhausted", () => {
    fail(new Error("Direct peer recovery was exhausted. Create or join a fresh lobby to try again."));
  });

  current.addEventListener("signaling-closed", () => {
    enterRecovery("Lobby signaling was interrupted. LobbySession is reconnecting with the same capability.");
  });

  current.addEventListener("statechange", (event) => {
    if (current !== session || intentionalClose) return;
    const state = event.detail.state;
    if (state === "reconnect-wait" || state === "reconnect-socket-open") {
      enterRecovery("Lobby signaling is recovering without minting a new participant identity.");
      return;
    }
    if (state === "reconnect-exhausted") {
      fail(new Error("Lobby signaling reconnect attempts were exhausted."));
      return;
    }
    if (state === "signaling-connected") {
      const peerId = current.readyPeerIds()[0];
      if (peerId) beginReleaseCheck(current, peerId);
      else setState("connecting", "Lobby signaling is connected. Waiting for the direct peer link.");
    }
  });

  current.addEventListener("reliable", (event) => {
    if (current !== session) return;
    const readiness = inspectReadinessMessage(event.detail.data);
    if (!readiness.recognized) return;
    if (currentPeerId && event.detail.peerId !== currentPeerId) return;

    currentPeerId = event.detail.peerId;
    peerReadiness = readiness;
    renderPeerReadiness(readiness);

    if (!readiness.compatible) {
      startButton.disabled = true;
      setState("failure", readiness.reason);
      return;
    }

    setState("release-check", readiness.ready ? "Friend reports compatible readiness; checking the local gate." : "Friend is compatible but not ready.");
    updateStartGate();
  });

  current.addEventListener("error", (event) => {
    if (current !== session || intentionalClose) return;
    if (currentState === "recovery") {
      statusDetail.textContent = `Recovery is still in progress: ${event.detail.error.message}`;
      return;
    }
    fail(event.detail.error);
  });
}

function createSession() {
  intentionalClose = true;
  session?.close();
  intentionalClose = false;
  resetReadiness();
  const current = new LobbySession({
    apiBase: setupApiBase(),
    topology: "mesh",
    contentSharing: false,
    iceServers: configuredIceServers("iceServers"),
    turnIceServers: configuredIceServers("turnIceServers"),
  });
  session = current;
  wireSession(current);
  return current;
}

async function hostBattle() {
  setSetupControlsDisabled(true);
  setState("creating", "Creating a private lobby for exactly two participants.");
  try {
    const current = createSession();
    await current.host(2);
  } catch (error) {
    fail(error);
  }
}

async function joinBattle() {
  setSetupControlsDisabled(true);
  setState("joining", "Joining the private lobby with a new participant capability owned only by this client.");
  try {
    const code = normalizeLobbyCode(lobbyCodeInput.value);
    const current = createSession();
    await current.join(code);
  } catch (error) {
    fail(error);
  }
}

hostButton.addEventListener("click", hostBattle);
joinButton.addEventListener("click", joinBattle);
lobbyCodeInput.addEventListener("keydown", (event) => {
  if (event.key === "Enter" && !joinButton.disabled) joinBattle();
});

copyInviteButton.addEventListener("click", async () => {
  if (!inviteLink.href) return;
  try {
    await navigator.clipboard.writeText(inviteLink.href);
    copyNotice.textContent = "Invite copied.";
  } catch {
    copyNotice.textContent = "Copy the invite URL shown above.";
  }
});

leaveButton.addEventListener("click", () => closeSession({ keepInviteCode: true }));

startButton.addEventListener("click", () => {
  if (startButton.disabled || !session || !currentPeerId) return;
  const allowed = canStartBattle({
    peerConnected: session.readyPeerIds().includes(currentPeerId),
    localReady,
    peerReadiness,
  });
  if (!allowed) return;
  window.dispatchEvent(
    new CustomEvent("medieval:battle-start-requested", {
      detail: {
        lobbyId: session.lobbyId,
        peerId: currentPeerId,
        release: MEDIEVAL_RELEASE,
        battleProtocol: MEDIEVAL_BATTLE_PROTOCOL,
      },
    }),
  );
  battleGateNote.textContent = "Start requested locally. Tactical battle transport is not implemented in this slice.";
});

for (const button of document.querySelectorAll("#online-view [data-back-to-menu]")) {
  button.addEventListener("click", () => closeSession({ keepInviteCode: true }));
}

window.addEventListener("beforeunload", () => {
  intentionalClose = true;
  session?.close();
});

localRelease.textContent = `Medieval ${MEDIEVAL_RELEASE} · battle protocol ${MEDIEVAL_BATTLE_PROTOCOL}`;
lobbyCodeInput.value = lobbyCodeFromUrl(window.location.href) ?? "";
setState("idle", lobbyCodeInput.value ? "Invite loaded. Join when you are ready." : "");
