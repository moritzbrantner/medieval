import init, {
  battle_sandbox_control,
  battle_sandbox_campaign_result,
  battle_sandbox_frame,
  battle_sandbox_ground_viewport,
  battle_sandbox_pan,
  battle_sandbox_pointer,
  battle_sandbox_quote_army,
  battle_sandbox_reset,
  battle_sandbox_set_formation,
  battle_sandbox_set_location,
  battle_sandbox_set_paused,
  battle_sandbox_set_speed,
  battle_sandbox_start_at_location,
  battle_sandbox_start_campaign,
  battle_sandbox_status,
  battle_sandbox_unit_viewport,
} from "./pkg/medieval_web_battle.js";
import { attachBattleInputBindings } from "./battle-input-bindings.js";

const canvas = document.querySelector("#battle-canvas");
const stateText = document.querySelector("#battle-state");
const selectionText = document.querySelector("#battle-selection");
const unitList = document.querySelector("#unit-list");
const errorBox = document.querySelector("#battle-error");
const withdrawButton = document.querySelector("#withdraw-battle");
const pauseButton = document.querySelector("#pause-battle");
const stopButton = document.querySelector("#stop-units");
const lineFormationButton = document.querySelector("#line-formation");
const columnFormationButton = document.querySelector("#column-formation");
const fitButton = document.querySelector("#fit-camera");
const resetButton = document.querySelector("#reset-battle");
const armySetup = document.querySelector("#army-setup");
const battleStage = document.querySelector("#battle-stage");
const armyOptions = document.querySelector("#army-options");
const armyBudget = document.querySelector("#army-budget");
const armySpent = document.querySelector("#army-spent");
const armyRemaining = document.querySelector("#army-remaining");
const armySetupReason = document.querySelector("#army-setup-reason");
const armySetupError = document.querySelector("#army-setup-error");
const startBattleButton = document.querySelector("#start-sandbox-battle");
const locationSelect = document.querySelector("#battle-location");
const battleSides = document.querySelector("#battle-sides");
const playerSideText = battleSides.querySelector('[data-field="player-side"]');
const opponentSideText = battleSides.querySelector('[data-field="opponent-side"]');
const siegeCapture = document.querySelector("#siege-capture");
const siegeCaptureText = document.querySelector("#siege-capture-text");
const siegeCaptureProgress = document.querySelector("#siege-capture-progress");
const speedInputs = [...document.querySelectorAll('input[name="simulation-speed"]')];
const selectedUnitCards = document.querySelector("#selected-unit-cards");
const selectedUnitsEmpty = document.querySelector("#selected-units-empty");
const controlGroupList = document.querySelector("#control-group-list");
const controlGroupsEmpty = document.querySelector("#control-groups-empty");
const battleLocations = new Set(["mountainPass", "forestClearing", "riverFord"]);
const requestedLocation = new URLSearchParams(window.location.search).get("location");
if (battleLocations.has(requestedLocation)) locationSelect.value = requestedLocation;

let currentStatus;
const controlsE2E = new URLSearchParams(window.location.search).has("e2e-controls");
let animationActive = false;
const campaignMode = new URLSearchParams(window.location.search).has("campaign");
const returnCampaignButton = document.querySelector("#return-campaign");
let campaignStarting = false;
let campaignResultSent = false;
let wasmReady = false;
let battleStarted = false;
let lastStatusRefresh = 0;
let renderedControlGroups = "";
const armySelection = {
  levy: 0,
  spearmen: 1,
  archers: 1,
  knights: 1,
};

function reportError(error) {
  const target = battleStarted ? errorBox : armySetupError;
  target.hidden = false;
  target.textContent = String(error);
}

function clearError() {
  errorBox.hidden = true;
  errorBox.textContent = "";
  armySetupError.hidden = true;
  armySetupError.textContent = "";
}

function renderArmyQuote(rawQuote) {
  const quote = typeof rawQuote === "string" ? JSON.parse(rawQuote) : rawQuote;
  armyBudget.textContent = `${quote.budget} gold`;
  armySpent.textContent = `${quote.spent} gold`;
  armyRemaining.textContent = `${quote.remaining} gold`;
  armySetupReason.textContent = quote.reason
    ?? `${quote.battalions} battalion${quote.battalions === 1 ? "" : "s"} ready to deploy.`;
  stateText.textContent = "Choose your army.";
  selectionText.textContent = `${quote.spent} / ${quote.budget} gold committed`;
  startBattleButton.disabled = !wasmReady || !quote.canStart || battleStarted;

  const fragment = document.createDocumentFragment();
  for (const unit of quote.units) {
    const row = document.createElement("div");
    row.className = "army-option";

    const copy = document.createElement("div");
    const name = document.createElement("strong");
    name.textContent = unit.label;
    const cost = document.createElement("small");
    cost.textContent = `${unit.cost} gold per battalion`;
    copy.append(name, cost);

    const stepper = document.createElement("div");
    stepper.className = "army-stepper";
    const remove = document.createElement("button");
    remove.type = "button";
    remove.className = "secondary";
    remove.textContent = "−";
    remove.setAttribute("aria-label", `Remove ${unit.label} battalion`);
    remove.disabled = unit.selected === 0;
    remove.addEventListener("click", () => adjustArmy(unit.unit, -1));

    const count = document.createElement("span");
    count.className = "army-count";
    count.textContent = String(unit.selected);
    count.setAttribute("aria-label", `${unit.label} battalions selected`);

    const add = document.createElement("button");
    add.type = "button";
    add.textContent = "+";
    add.setAttribute("aria-label", `Add ${unit.label} battalion`);
    add.disabled = quote.battalions >= quote.maxBattalions || unit.cost > quote.remaining;
    add.addEventListener("click", () => adjustArmy(unit.unit, 1));

    stepper.append(remove, count, add);
    row.append(copy, stepper);
    fragment.append(row);
  }
  armyOptions.replaceChildren(fragment);
}

function refreshArmyQuote({ preserveError = false } = {}) {
  if (!preserveError) clearError();
  try {
    renderArmyQuote(battle_sandbox_quote_army(JSON.stringify(armySelection)));
  } catch (error) {
    reportError(error);
  }
}

function adjustArmy(unit, delta) {
  const current = armySelection[unit];
  if (!Number.isInteger(current)) {
    reportError(`Unknown army unit ${unit}.`);
    return;
  }
  armySelection[unit] = Math.max(0, current + delta);
  refreshArmyQuote();
}

async function beginBattle() {
  if (!wasmReady || battleStarted) return;
  clearError();
  startBattleButton.disabled = true;
  if (!navigator.gpu) {
    renderArmyQuote(battle_sandbox_quote_army(JSON.stringify(armySelection)));
    reportError("This browser does not expose WebGPU. Use a current browser with WebGPU enabled.");
    return;
  }

  armySetup.hidden = true;
  battleStage.hidden = false;
  syncCanvasSize();
  try {
    const status = await battle_sandbox_start_at_location(
      canvas.id,
      JSON.stringify(armySelection),
      locationSelect.value,
    );
    battleStarted = true;
    renderStatus(status);
    canvas.focus();
    animationActive = true;
    if (controlsE2E) {
      await battleInputBindings.ready;
      document.documentElement.dataset.controlsE2eReady = "true";
    }
    requestAnimationFrame(animate);
  } catch (error) {
    battleStage.hidden = true;
    armySetup.hidden = false;
    reportError(error);
    refreshArmyQuote({ preserveError: true });
  }
}

async function beginCampaignBattle(seed) {
  if (!wasmReady || battleStarted || campaignStarting) return;
  campaignStarting = true;
  clearError();
  armySetup.hidden = true;
  battleStage.hidden = false;
  locationSelect.disabled = true;
  document.querySelector("#reset-battle").disabled = true;
  document.querySelector(".back-link").hidden = true;
  returnCampaignButton.hidden = false;
  document.querySelector("#battle-context").textContent = "Campaign tactical battle";
  document.querySelector("#battle-title").textContent = "Campaign Battle";
  syncCanvasSize();
  try {
    const status = await battle_sandbox_start_campaign(canvas.id, JSON.stringify(seed));
    battleStarted = true;
    renderStatus(status);
    canvas.focus();
    animationActive = true;
    await battleInputBindings.ready;
    document.documentElement.dataset.controlsE2eReady = "true";
    requestAnimationFrame(animate);
  } catch (error) {
    reportError(error);
    stateText.textContent = "Campaign battle could not start.";
  } finally {
    campaignStarting = false;
  }
}

window.addEventListener("message", (event) => {
  if (!campaignMode || event.origin !== window.location.origin || event.source !== window.parent
    || event.data?.kind !== "medieval-campaign-battle-start") return;
  beginCampaignBattle(event.data.seed);
});
returnCampaignButton.addEventListener("click", () => {
  window.parent.postMessage({ kind: "medieval-campaign-battle-exit" }, window.location.origin);
});

function syncCanvasSize() {
  const rect = canvas.getBoundingClientRect();
  const scale = Math.max(1, window.devicePixelRatio || 1);
  const width = Math.max(1, Math.round(rect.width * scale));
  const height = Math.max(1, Math.round(rect.height * scale));
  if (canvas.width !== width) canvas.width = width;
  if (canvas.height !== height) canvas.height = height;
}

function readableUnitName(id) {
  return id
    .replace(/^attacker-/, "")
    .replace(/^defender-/, "")
    .replaceAll("-", " ")
    .replace(/\b\w/g, (letter) => letter.toUpperCase());
}

function unitState(unit) {
  if (unit.destroyed) return "destroyed";
  if (unit.escaped) return "escaped";
  if (unit.withdrawing) return "withdrawing";
  if (unit.routed) return "routed";
  return "formed";
}

function readableFormation(unit) {
  const name = unit.formation === "column" ? "column" : "line";
  return `${name} · ${unit.formationFiles} files`;
}

function readableRange(rangeMm) {
  const metres = rangeMm / 1000;
  return Number.isInteger(metres) ? `${metres} m` : `${metres.toFixed(1)} m`;
}

const UNIT_KIND_LABELS = { levy: "Levy", spearmen: "Spearmen", archers: "Archers", knights: "Knights" };
const SIDE_LABELS = { attacker: "Attacker", defender: "Defender" };

function readableSpeed(multiplier) {
  return `${multiplier}×`;
}

function readableOrder(order) {
  switch (order?.kind) {
    case "move":
      return "Move";
    case "attackMove":
      return "Attack-move";
    case "engage":
      return `Engaging ${readableUnitName(order.targetUnitId)}`;
    case "attackGate":
      return "Attacking the gate";
    case "withdraw":
      return "Withdrawing";
    case "rout":
      return "Routed";
    case "destroyed":
      return "Destroyed";
    case "escaped":
      return "Escaped";
    default:
      return "Holding";
  }
}

function renderBattleStatus(status) {
  battleSides.hidden = false;
  playerSideText.textContent = SIDE_LABELS[status.playerSide] ?? status.playerSide;
  opponentSideText.textContent = SIDE_LABELS[status.opponentSide] ?? status.opponentSide;
  for (const input of speedInputs) {
    input.checked = Number(input.value) === status.speedMultiplier;
  }

  const capture = status.siege?.capture;
  siegeCapture.hidden = !capture;
  if (!capture) return;
  siegeCaptureProgress.max = status.siegeCaptureMaxProgress;
  siegeCaptureProgress.value = capture.progress;
  const holder = capture.capturedBy ?? capture.capturingSide;
  const action = capture.capturedBy ? "captured" : "capturing";
  siegeCaptureText.textContent = holder
    ? `Siege capture · ${SIDE_LABELS[holder] ?? holder} ${action} · ${capture.progress} / ${status.siegeCaptureMaxProgress}`
    : `Siege capture · ${capture.progress} / ${status.siegeCaptureMaxProgress}`;
}

function cardField(label, field, value) {
  const term = document.createElement("dt");
  term.textContent = label;
  const detail = document.createElement("dd");
  detail.dataset.field = field;
  detail.textContent = value;
  return [term, detail];
}

function renderSelectedUnitCards(status) {
  const fragment = document.createDocumentFragment();
  for (const unitId of status.selectedUnits) {
    const unit = status.units.find((candidate) => candidate.id === unitId);
    if (!unit) continue;
    const card = document.createElement("article");
    card.className = "unit-card";
    card.dataset.unitId = unit.id;
    card.dataset.state = unitState(unit);

    const heading = document.createElement("header");
    const name = document.createElement("strong");
    name.textContent = unit.sourceArmyId ?? readableUnitName(unit.id);
    const kind = document.createElement("span");
    kind.dataset.field = "kind";
    kind.textContent = UNIT_KIND_LABELS[unit.unitKind] ?? "Legacy unit";
    heading.append(name, kind);

    const fields = document.createElement("dl");
    fields.append(
      ...cardField("Soldiers", "soldiers", String(unit.soldiers)),
      ...cardField("Morale", "morale", String(unit.morale)),
      ...cardField("Fatigue", "fatigue", String(unit.fatigue)),
      ...cardField("Ammunition", "ammunition", Number.isInteger(unit.ammunition) ? `${unit.ammunition} volleys` : "—"),
      ...cardField("Formation", "formation", readableFormation(unit)),
      ...cardField("Order", "order", readableOrder(unit.order)),
    );
    card.append(heading, fields);
    fragment.append(card);
  }
  selectedUnitsEmpty.hidden = fragment.childNodes.length > 0;
  selectedUnitCards.replaceChildren(fragment);
}

function renderControlGroups(status) {
  const groups = status.controlGroups ?? [];
  // Rebuild only when Rust group state changes so a focused group button keeps focus.
  const signature = JSON.stringify(groups);
  if (signature === renderedControlGroups) return;
  renderedControlGroups = signature;
  const focusedGroup = document.activeElement?.closest?.("[data-control-group]")?.dataset.controlGroup;

  const fragment = document.createDocumentFragment();
  for (const { group, unitIds } of groups) {
    const button = document.createElement("button");
    button.type = "button";
    button.className = "secondary control-group";
    button.dataset.controlGroup = String(group);
    const count = `${unitIds.length} unit${unitIds.length === 1 ? "" : "s"}`;
    button.setAttribute("aria-label", `Control group ${group}, ${count}: ${unitIds.map(readableUnitName).join(", ")}`);
    const number = document.createElement("strong");
    number.textContent = String(group);
    const detail = document.createElement("span");
    detail.textContent = count;
    button.append(number, detail);
    button.addEventListener("click", () => runControl({ kind: "recallControlGroup", group }));
    fragment.append(button);
  }
  controlGroupsEmpty.hidden = groups.length > 0;
  controlGroupList.replaceChildren(fragment);
  if (focusedGroup !== undefined) {
    controlGroupList.querySelector(`[data-control-group="${focusedGroup}"]`)?.focus();
  }
}

function renderStatus(rawStatus) {
  currentStatus = typeof rawStatus === "string" ? JSON.parse(rawStatus) : rawStatus;
  if (campaignMode && currentStatus.outcome && !campaignResultSent) {
    const document = battle_sandbox_campaign_result();
    campaignResultSent = true;
    window.parent.postMessage({ kind: "medieval-campaign-battle-finished", document }, window.location.origin);
  }
  document.querySelector("#attack-move").setAttribute("aria-pressed", String(currentStatus.attackMoveArmed));
  const reason = currentStatus.battleState?.reason;
  const withdrawalText = reason === "withdrawal"
    ? { playerVictory: "Victory — the opposing force withdrew.", playerDefeat: "Your force withdrew from the battlefield." }[currentStatus.outcome]
    : reason === "mutualWithdrawal" ? "Both forces withdrew from the battlefield." : null;
  const captureText = reason === "siegeCapture"
    ? { playerVictory: "Victory — your force captured the stronghold.", playerDefeat: "Defeat — the enemy captured the stronghold." }[currentStatus.outcome]
    : null;
  const outcomeText = withdrawalText ?? captureText ?? {
    playerVictory: "Victory — the opposing force can no longer fight.",
    playerDefeat: "Defeat — your force can no longer fight.",
    draw: "Battle ended with neither side able to continue.",
  }[currentStatus.outcome];
  stateText.textContent = outcomeText
    ?? `${currentStatus.paused ? "Paused" : "Running"} · ${readableSpeed(currentStatus.speedMultiplier)} · simulation tick ${currentStatus.tick}`;
  renderBattleStatus(currentStatus);

  selectionText.textContent = currentStatus.selectedUnits.length
    ? `Selected: ${currentStatus.selectedUnits.map(readableUnitName).join(", ")}`
    : "No units selected.";
  locationSelect.value = currentStatus.battlefieldLocation;
  pauseButton.textContent = currentStatus.paused && !currentStatus.outcome ? "Resume" : "Pause";
  pauseButton.disabled = Boolean(currentStatus.outcome);
  withdrawButton.disabled = !currentStatus.canWithdraw;
  const selectionDisabled = currentStatus.selectedUnits.length === 0 || Boolean(currentStatus.outcome);
  stopButton.disabled = selectionDisabled;
  document.querySelector("#attack-move").disabled = selectionDisabled;
  lineFormationButton.disabled = selectionDisabled;
  columnFormationButton.disabled = selectionDisabled;
  renderSelectedUnitCards(currentStatus);
  renderControlGroups(currentStatus);

  const fragment = document.createDocumentFragment();
  for (const unit of currentStatus.units) {
    const row = document.createElement("div");
    row.className = "unit-row";
    row.dataset.selected = String(unit.selected);
    row.dataset.state = unitState(unit);
    row.dataset.unitKind = unit.unitKind ?? "legacy";

    const name = document.createElement("strong");
    name.textContent = unit.sourceArmyId ?? readableUnitName(unit.id);

    const side = document.createElement("span");
    side.className = "unit-side";
    side.textContent = unit.side;

    const detail = document.createElement("small");
    const order = unit.engagementTarget
      ? ` · engaging ${readableUnitName(unit.engagementTarget)}`
      : "";
    const kind = UNIT_KIND_LABELS[unit.unitKind] ?? "Legacy unit";
    const ammunition = unit.ammunition === 0 ? " · ammunition exhausted" : Number.isInteger(unit.ammunition) ? ` · ammunition ${unit.ammunition} volleys` : "";
    detail.textContent = `${kind} · ${unitState(unit)} · ${unit.soldiers} soldiers · ${readableFormation(unit)} · range ${readableRange(unit.attackRangeMm)} · morale ${unit.morale} · fatigue ${unit.fatigue}${ammunition}${order}`;

    row.append(name, side, detail);
    fragment.append(row);
  }
  unitList.replaceChildren(fragment);
}

function projectForControlsE2E(projector, ...args) {
  const rect = canvas.getBoundingClientRect();
  return JSON.parse(projector(...args, rect.width, rect.height));
}

if (controlsE2E) {
  window.__medievalControlsE2E = Object.freeze({
    status: () => structuredClone(currentStatus),
    unitViewport: (unitId) => projectForControlsE2E(battle_sandbox_unit_viewport, unitId),
    groundViewport: (xMm, yMm) => projectForControlsE2E(
      battle_sandbox_ground_viewport,
      xMm,
      yMm,
    ),
  });
}

function runControl(request) {
  clearError();
  try {
    renderStatus(battle_sandbox_control(JSON.stringify(request)));
  } catch (error) {
    reportError(error);
  }
}

function setFormation(kind) {
  if (!currentStatus?.selectedUnits.length || currentStatus.outcome) return;
  clearError();
  try {
    renderStatus(battle_sandbox_set_formation(kind));
  } catch (error) {
    reportError(error);
  }
}

function setSpeed(multiplier) {
  if (!currentStatus) return;
  clearError();
  try {
    renderStatus(battle_sandbox_set_speed(multiplier));
  } catch (error) {
    reportError(error);
  }
}

function togglePause() {
  if (!currentStatus || currentStatus.outcome) return;
  clearError();
  try {
    renderStatus(battle_sandbox_set_paused(!currentStatus.paused));
  } catch (error) {
    reportError(error);
  }
}

function animate(timestamp) {
  if (!animationActive) return;
  try {
    syncCanvasSize();
    battle_sandbox_frame(timestamp);
    if (timestamp - lastStatusRefresh >= 200) {
      renderStatus(battle_sandbox_status());
      lastStatusRefresh = timestamp;
    }
  } catch (error) {
    animationActive = false;
    reportError(error);
    stateText.textContent = "Battle sandbox stopped.";
    return;
  }
  requestAnimationFrame(animate);
}

canvas.addEventListener("pointerdown", (event) => {
  if (event.button !== 0 && event.button !== 2) return;
  event.preventDefault();
  // Focusing must not scroll the page under the pointer: the click position is
  // measured against the canvas where the player clicked.
  canvas.focus({ preventScroll: true });
  const rect = canvas.getBoundingClientRect();
  clearError();
  try {
    renderStatus(
      battle_sandbox_pointer(
        event.button,
        event.clientX - rect.left,
        event.clientY - rect.top,
        rect.width,
        rect.height,
        event.shiftKey,
        event.ctrlKey,
      ),
    );
  } catch (error) {
    reportError(error);
  }
});

canvas.addEventListener("contextmenu", (event) => event.preventDefault());
canvas.addEventListener(
  "wheel",
  (event) => {
    event.preventDefault();
    runControl({ kind: "zoomCamera", factor: event.deltaY < 0 ? 1.15 : 0.87 });
  },
  { passive: false },
);

const battleInputBindings = attachBattleInputBindings({
  canvas,
  pan(direction) {
    try {
      renderStatus(battle_sandbox_pan(direction));
    } catch (error) {
      reportError(error);
    }
  },
  zoom(factor) {
    runControl({ kind: "zoomCamera", factor });
  },
  stopSelected() {
    runControl({ kind: "stopSelected" });
  },
  setFormation,
  armAttackMove() { runControl({ kind: "armAttackMove" }); },
  turnSelected(quarterTurns) {
    runControl({ kind: "turnSelected", quarterTurns });
  },
  fitCamera() {
    runControl({ kind: "fitCamera" });
  },
  togglePause,
  assignControlGroup(group) {
    runControl({ kind: "assignControlGroup", group });
  },
  recallControlGroup(group, additive) {
    runControl({ kind: "recallControlGroup", group, additive });
  },
});
window.addEventListener("pagehide", () => battleInputBindings.destroy(), { once: true });

withdrawButton.addEventListener("click", () => runControl({ kind: "withdraw" }));

pauseButton.addEventListener("click", togglePause);
for (const input of speedInputs) {
  input.addEventListener("change", () => {
    if (input.checked) setSpeed(Number(input.value));
  });
}
stopButton.addEventListener("click", () => runControl({ kind: "stopSelected" }));
document.querySelector("#attack-move").addEventListener("click", () => runControl({ kind: "armAttackMove" }));
document.querySelector("#set-frontage").addEventListener("click", () => {
  const input = document.querySelector("#frontage-metres");
  if (!input.reportValidity()) return;
  runControl({ kind: "frontageSelected", widthMm: Number(input.value) * 1000 });
});
lineFormationButton.addEventListener("click", () => setFormation("line"));
columnFormationButton.addEventListener("click", () => setFormation("column"));
fitButton.addEventListener("click", () => runControl({ kind: "fitCamera" }));
resetButton.addEventListener("click", () => {
  clearError();
  try {
    renderStatus(battle_sandbox_reset());
    canvas.focus();
  } catch (error) {
    reportError(error);
  }
});
startBattleButton.addEventListener("click", beginBattle);
locationSelect.addEventListener("change", () => {
  if (!currentStatus || !battleLocations.has(locationSelect.value)) return;
  clearError();
  try {
    renderStatus(battle_sandbox_set_location(locationSelect.value));
    const url = new URL(window.location.href);
    url.searchParams.set("location", locationSelect.value);
    history.replaceState(null, "", url);
    canvas.focus();
  } catch (error) {
    reportError(error);
  }
});
window.addEventListener("resize", syncCanvasSize);

async function start() {
  try {
    await init();
    wasmReady = true;
    if (campaignMode) {
      armySetup.hidden = true;
      returnCampaignButton.hidden = false;
      stateText.textContent = "Preparing campaign army…";
      window.parent.postMessage({ kind: "medieval-campaign-battle-ready" }, window.location.origin);
    } else {
      refreshArmyQuote();
    }
  } catch (error) {
    animationActive = false;
    stateText.textContent = "Army muster could not load.";
    reportError(error);
  }
}

start();
