export const MEDIEVAL_RELEASE = "0.1.0";
export const MEDIEVAL_BATTLE_PROTOCOL = 1;
export const MEDIEVAL_READINESS_PROTOCOL = 1;
export const READINESS_MESSAGE_TYPE = "medieval-ready";

export const ONLINE_BATTLE_STATES = Object.freeze([
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

export function releaseFingerprint() {
  return {
    release: MEDIEVAL_RELEASE,
    battleProtocol: MEDIEVAL_BATTLE_PROTOCOL,
  };
}

export function createReadinessMessage({ ready = true } = {}) {
  return {
    type: READINESS_MESSAGE_TYPE,
    v: MEDIEVAL_READINESS_PROTOCOL,
    ...releaseFingerprint(),
    ready: ready === true,
  };
}

export function inspectReadinessMessage(message) {
  if (
    !message ||
    typeof message !== "object" ||
    message.type !== READINESS_MESSAGE_TYPE ||
    message.v !== MEDIEVAL_READINESS_PROTOCOL ||
    typeof message.release !== "string" ||
    !Number.isSafeInteger(message.battleProtocol) ||
    typeof message.ready !== "boolean"
  ) {
    return {
      recognized: false,
      compatible: false,
      ready: false,
      reason: "Unexpected pre-battle readiness message",
    };
  }

  if (message.release !== MEDIEVAL_RELEASE) {
    return {
      recognized: true,
      compatible: false,
      ready: message.ready,
      fingerprint: { release: message.release, battleProtocol: message.battleProtocol },
      reason: `Release mismatch: friend has ${message.release}, this client has ${MEDIEVAL_RELEASE}`,
    };
  }

  if (message.battleProtocol !== MEDIEVAL_BATTLE_PROTOCOL) {
    return {
      recognized: true,
      compatible: false,
      ready: message.ready,
      fingerprint: { release: message.release, battleProtocol: message.battleProtocol },
      reason: `Battle protocol mismatch: friend has ${message.battleProtocol}, this client has ${MEDIEVAL_BATTLE_PROTOCOL}`,
    };
  }

  return {
    recognized: true,
    compatible: true,
    ready: message.ready,
    fingerprint: { release: message.release, battleProtocol: message.battleProtocol },
    reason: null,
  };
}

export function canStartBattle({ peerConnected = false, localReady = false, peerReadiness = null } = {}) {
  return Boolean(
    peerConnected &&
      localReady &&
      peerReadiness?.recognized &&
      peerReadiness.compatible &&
      peerReadiness.ready,
  );
}

export function normalizeLobbyCode(value) {
  const code = String(value ?? "").trim();
  if (!code) throw new Error("Enter a lobby code");
  return code;
}

export function buildInviteUrl(baseHref, lobbyCode) {
  const code = normalizeLobbyCode(lobbyCode);
  const url = new URL(baseHref);
  url.search = "";
  url.hash = "";
  url.searchParams.set("lobby", code);
  return url.toString();
}

export function lobbyCodeFromUrl(href) {
  const value = new URL(href).searchParams.get("lobby");
  return value ? normalizeLobbyCode(value) : null;
}
