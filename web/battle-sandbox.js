import init, {
  battle_sandbox_control,
  battle_sandbox_frame,
  battle_sandbox_ground_viewport,
  battle_sandbox_pan,
  battle_sandbox_pointer,
  battle_sandbox_quote_army,
  battle_sandbox_reset,
  battle_sandbox_set_formation,
  battle_sandbox_set_paused,
  battle_sandbox_start,
  battle_sandbox_status,
  battle_sandbox_unit_viewport,
} from "./pkg/medieval_web_battle.js";
import { attachBattleInputBindings } from "./battle-input-bindings.js";

const canvas = document.querySelector("#battle-canvas");
const stateText = document.querySelector("#battle-state");
const selectionText = document.querySelector("#battle-selection");
const unitList = document.querySelector("#unit-list");
const errorBox = document.querySelector("#battle-error");
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

let currentStatus;
const controlsE2E = new URLSearchParams(window.location.search).has("e2e-controls");
let animationActive = false;
let wasmReady = false;
let battleStarted = false;
let lastStatusRefresh = 0;
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

function refreshArmyQuote() {
  clearError();
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
    const status = await battle_sandbox_start(canvas.id, JSON.stringify(armySelection));
    battleStarted = true;
    renderStatus(status);
    canvas.focus();
    animationActive = true;
    if (controlsE2E) document.documentElement.dataset.controlsE2eReady = "true";
    requestAnimationFrame(animate);
  } catch (error) {
    battleStage.hidden = true;
    armySetup.hidden = false;
    refreshArmyQuote();
    reportError(error);
  }
}

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

function renderStatus(rawStatus) {
  currentStatus = typeof rawStatus === "string" ? JSON.parse(rawStatus) : rawStatus;
  const outcomeText = {
    playerVictory: "Victory — the opposing force can no longer fight.",
    playerDefeat: "Defeat — your force can no longer fight.",
    draw: "Battle ended with neither side able to continue.",
  }[currentStatus.outcome];
  stateText.textContent = outcomeText
    ?? `${currentStatus.paused ? "Paused" : "Running"} · simulation tick ${currentStatus.tick}`;

  selectionText.textContent = currentStatus.selectedUnits.length
    ? `Selected: ${currentStatus.selectedUnits.map(readableUnitName).join(", ")}`
    : "No units selected.";
  pauseButton.textContent = currentStatus.paused && !currentStatus.outcome ? "Resume" : "Pause";
  pauseButton.disabled = Boolean(currentStatus.outcome);
  const selectionDisabled = currentStatus.selectedUnits.length === 0 || Boolean(currentStatus.outcome);
  stopButton.disabled = selectionDisabled;
  lineFormationButton.disabled = selectionDisabled;
  columnFormationButton.disabled = selectionDisabled;

  const fragment = document.createDocumentFragment();
  for (const unit of currentStatus.units) {
    const row = document.createElement("div");
    row.className = "unit-row";
    row.dataset.selected = String(unit.selected);
    row.dataset.state = unitState(unit);

    const name = document.createElement("strong");
    name.textContent = readableUnitName(unit.id);

    const side = document.createElement("span");
    side.className = "unit-side";
    side.textContent = unit.side;

    const detail = document.createElement("small");
    const order = unit.engagementTarget
      ? ` · engaging ${readableUnitName(unit.engagementTarget)}`
      : "";
    detail.textContent = `${unitState(unit)} · ${unit.soldiers} soldiers · ${readableFormation(unit)} · range ${readableRange(unit.attackRangeMm)} · morale ${unit.morale} · fatigue ${unit.fatigue}${order}`;

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
  canvas.focus();
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
  fitCamera() {
    runControl({ kind: "fitCamera" });
  },
  togglePause,
});
window.addEventListener("pagehide", () => battleInputBindings.destroy(), { once: true });

pauseButton.addEventListener("click", togglePause);
stopButton.addEventListener("click", () => runControl({ kind: "stopSelected" }));
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
window.addEventListener("resize", syncCanvasSize);

async function start() {
  try {
    await init();
    wasmReady = true;
    refreshArmyQuote();
  } catch (error) {
    animationActive = false;
    stateText.textContent = "Army muster could not load.";
    reportError(error);
  }
}

start();
