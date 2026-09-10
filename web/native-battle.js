const nativeBattleButton = document.querySelector("#open-native-battle");
const nativeBattleStatus = document.querySelector("#native-battle-status");

const TACTICAL_INPUT_EVENT = "medieval:tactical-input";
const tacticalKeys = new Set([
  "ArrowLeft",
  "ArrowRight",
  "ArrowUp",
  "ArrowDown",
  "Equal",
  "Minus",
  "NumpadAdd",
  "NumpadSubtract",
  "Space",
  "KeyS",
  "Escape",
  "Digit0",
  "Digit1",
  "Digit2",
  "Digit3",
  "Digit4",
  "Digit5",
  "Digit6",
  "Digit7",
  "Digit8",
  "Digit9",
]);

let browserInputActive = false;
let inputSequence = 0;
let inputQueue = Promise.resolve();

if (window.__MEDIEVAL_RUNTIME__ === "wasm" && nativeBattleButton) {
  nativeBattleButton.textContent = "Open single-player battle sandbox";
  nativeBattleStatus.textContent = "Run the Rust tactical simulation directly in this browser through WebGPU.";
}

function modifiers(event) {
  return {
    shift: event.shiftKey,
    control: event.ctrlKey || event.metaKey,
  };
}

function viewportPointer(event) {
  return {
    x: event.clientX,
    y: event.clientY,
    width: window.innerWidth,
    height: window.innerHeight,
  };
}

function queueTacticalInput(input) {
  const emit = window.__TAURI__?.event?.emit;
  if (!emit) return;

  inputSequence += 1;
  const envelope = { sequence: inputSequence, ...input };
  inputQueue = inputQueue
    .then(() => emit(TACTICAL_INPUT_EVENT, envelope))
    .catch((error) => {
      browserInputActive = false;
      nativeBattleStatus.textContent = `Tactical input stopped: ${String(error)}`;
    });
}

function consumeInput(event) {
  event.preventDefault();
  event.stopImmediatePropagation();
}

window.addEventListener(
  "keydown",
  (event) => {
    if (!browserInputActive || !tacticalKeys.has(event.code)) return;
    consumeInput(event);
    queueTacticalInput({
      kind: "keyDown",
      code: event.code,
      ...modifiers(event),
    });
    if (event.code === "Escape") {
      browserInputActive = false;
      nativeBattleStatus.textContent = "Native tactical renderer closed.";
    }
  },
  true,
);

window.addEventListener(
  "pointerdown",
  (event) => {
    if (!browserInputActive || (event.button !== 0 && event.button !== 2)) return;
    consumeInput(event);
    queueTacticalInput({
      kind: "pointerDown",
      button: event.button === 0 ? "primary" : "secondary",
      ...viewportPointer(event),
      ...modifiers(event),
    });
  },
  true,
);

window.addEventListener(
  "pointerup",
  (event) => {
    if (!browserInputActive || (event.button !== 0 && event.button !== 2)) return;
    consumeInput(event);
    queueTacticalInput({
      kind: "pointerUp",
      button: event.button === 0 ? "primary" : "secondary",
      ...viewportPointer(event),
      ...modifiers(event),
    });
  },
  true,
);

window.addEventListener(
  "wheel",
  (event) => {
    if (!browserInputActive) return;
    consumeInput(event);
    queueTacticalInput({ kind: "wheel", deltaY: event.deltaY });
  },
  { capture: true, passive: false },
);

window.addEventListener(
  "contextmenu",
  (event) => {
    if (browserInputActive) consumeInput(event);
  },
  true,
);

nativeBattleButton?.addEventListener("click", async () => {
  if (window.__MEDIEVAL_RUNTIME__ === "wasm") {
    window.location.href = new URL("battle.html", window.location.href).href;
    return;
  }

  const invoke = window.__TAURI__?.core?.invoke;
  if (!invoke) {
    nativeBattleStatus.textContent = "The native renderer preview is available in the desktop app.";
    return;
  }

  nativeBattleButton.disabled = true;
  nativeBattleStatus.textContent = "Opening the Rust/wgpu tactical renderer…";
  try {
    const result = await invoke("open_native_battle_renderer");
    inputSequence = 0;
    inputQueue = Promise.resolve();
    browserInputActive = Boolean(result?.browserInput);
    nativeBattleStatus.textContent = browserInputActive
      ? "Native tactical renderer active: mouse and keyboard input are routed to Rust. Press Escape to close."
      : "Native tactical renderer opened in its own desktop window.";
  } catch (error) {
    browserInputActive = false;
    nativeBattleStatus.textContent = `Could not open native tactical renderer: ${String(error)}`;
  } finally {
    nativeBattleButton.disabled = false;
  }
});
