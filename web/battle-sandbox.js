import init, {
  battle_sandbox_control,
  battle_sandbox_frame,
  battle_sandbox_ground_viewport,
  battle_sandbox_pan,
  battle_sandbox_pointer,
  battle_sandbox_reset,
  battle_sandbox_set_paused,
  battle_sandbox_start,
  battle_sandbox_status,
  battle_sandbox_unit_viewport,
} from "./pkg/medieval_web_battle.js";

const canvas = document.querySelector("#battle-canvas");
const stateText = document.querySelector("#battle-state");
const selectionText = document.querySelector("#battle-selection");
const unitList = document.querySelector("#unit-list");
const errorBox = document.querySelector("#battle-error");
const pauseButton = document.querySelector("#pause-battle");
const stopButton = document.querySelector("#stop-units");
const fitButton = document.querySelector("#fit-camera");
const resetButton = document.querySelector("#reset-battle");

let currentStatus;
const controlsE2E = new URLSearchParams(window.location.search).has("e2e-controls");
let animationActive = true;
let lastStatusRefresh = 0;

function reportError(error) {
  errorBox.hidden = false;
  errorBox.textContent = String(error);
}

function clearError() {
  errorBox.hidden = true;
  errorBox.textContent = "";
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
  stopButton.disabled = currentStatus.selectedUnits.length === 0 || Boolean(currentStatus.outcome);

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
    detail.textContent = `${unitState(unit)} · ${unit.soldiers} soldiers · morale ${unit.morale} · fatigue ${unit.fatigue}${order}`;

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

canvas.addEventListener("keydown", (event) => {
  const panDirection = {
    ArrowLeft: "left",
    KeyA: "left",
    ArrowRight: "right",
    KeyD: "right",
    ArrowUp: "up",
    KeyW: "up",
    ArrowDown: "down",
    KeyS: "down",
  }[event.code];
  if (panDirection) {
    event.preventDefault();
    try {
      renderStatus(battle_sandbox_pan(panDirection));
    } catch (error) {
      reportError(error);
    }
    return;
  }

  if (event.code === "Equal" || event.code === "NumpadAdd") {
    event.preventDefault();
    runControl({ kind: "zoomCamera", factor: 1.15 });
  } else if (event.code === "Minus" || event.code === "NumpadSubtract") {
    event.preventDefault();
    runControl({ kind: "zoomCamera", factor: 0.87 });
  } else if (event.code === "Space") {
    event.preventDefault();
    runControl({ kind: "stopSelected" });
  } else if (event.code === "Digit0") {
    event.preventDefault();
    runControl({ kind: "fitCamera" });
  } else if (event.code === "KeyP") {
    event.preventDefault();
    togglePause();
  }
});

pauseButton.addEventListener("click", togglePause);
stopButton.addEventListener("click", () => runControl({ kind: "stopSelected" }));
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
window.addEventListener("resize", syncCanvasSize);

async function start() {
  try {
    if (!navigator.gpu) {
      throw new Error("This browser does not expose WebGPU. Use a current browser with WebGPU enabled.");
    }
    syncCanvasSize();
    await init();
    renderStatus(await battle_sandbox_start(canvas.id));
    canvas.focus();
    if (controlsE2E) document.documentElement.dataset.controlsE2eReady = "true";
    requestAnimationFrame(animate);
  } catch (error) {
    animationActive = false;
    stateText.textContent = "Battle sandbox could not start.";
    reportError(error);
  }
}

start();
