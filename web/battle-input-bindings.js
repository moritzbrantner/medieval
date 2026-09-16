export const INPUT_BINDINGS_BUNDLE_URL =
  "https://moritzbrantner.github.io/input-bindings/input-bindings-browser.js";

const CONTEXT_ID = "battleSandbox";

const physical = (id, action, code) => ({
  id,
  action,
  sequence: [{ key: { kind: "physical", value: code }, modifiers: {} }],
  when: { op: "context", id: CONTEXT_ID },
  priority: 0,
});

export const BATTLE_INPUT_REGISTRY = Object.freeze({
  actions: [
    {
      id: "battle.camera.panLeft",
      title: "Pan camera left",
      categoryPath: ["Battle", "Camera"],
      repeatPolicy: "allow",
      allowedDevices: ["keyboard"],
      defaults: [
        physical("battle.camera.panLeft.arrow", "battle.camera.panLeft", "ArrowLeft"),
        physical("battle.camera.panLeft.a", "battle.camera.panLeft", "KeyA"),
      ],
      provenance: { source: "medieval/battle-sandbox", version: "1" },
    },
    {
      id: "battle.camera.panRight",
      title: "Pan camera right",
      categoryPath: ["Battle", "Camera"],
      repeatPolicy: "allow",
      allowedDevices: ["keyboard"],
      defaults: [
        physical("battle.camera.panRight.arrow", "battle.camera.panRight", "ArrowRight"),
        physical("battle.camera.panRight.d", "battle.camera.panRight", "KeyD"),
      ],
      provenance: { source: "medieval/battle-sandbox", version: "1" },
    },
    {
      id: "battle.camera.panUp",
      title: "Pan camera up",
      categoryPath: ["Battle", "Camera"],
      repeatPolicy: "allow",
      allowedDevices: ["keyboard"],
      defaults: [
        physical("battle.camera.panUp.arrow", "battle.camera.panUp", "ArrowUp"),
        physical("battle.camera.panUp.w", "battle.camera.panUp", "KeyW"),
      ],
      provenance: { source: "medieval/battle-sandbox", version: "1" },
    },
    {
      id: "battle.camera.panDown",
      title: "Pan camera down",
      categoryPath: ["Battle", "Camera"],
      repeatPolicy: "allow",
      allowedDevices: ["keyboard"],
      defaults: [
        physical("battle.camera.panDown.arrow", "battle.camera.panDown", "ArrowDown"),
        physical("battle.camera.panDown.s", "battle.camera.panDown", "KeyS"),
      ],
      provenance: { source: "medieval/battle-sandbox", version: "1" },
    },
    {
      id: "battle.camera.zoomIn",
      title: "Zoom camera in",
      categoryPath: ["Battle", "Camera"],
      repeatPolicy: "allow",
      allowedDevices: ["keyboard"],
      defaults: [
        physical("battle.camera.zoomIn.equal", "battle.camera.zoomIn", "Equal"),
        physical("battle.camera.zoomIn.numpad", "battle.camera.zoomIn", "NumpadAdd"),
      ],
      provenance: { source: "medieval/battle-sandbox", version: "1" },
    },
    {
      id: "battle.camera.zoomOut",
      title: "Zoom camera out",
      categoryPath: ["Battle", "Camera"],
      repeatPolicy: "allow",
      allowedDevices: ["keyboard"],
      defaults: [
        physical("battle.camera.zoomOut.minus", "battle.camera.zoomOut", "Minus"),
        physical("battle.camera.zoomOut.numpad", "battle.camera.zoomOut", "NumpadSubtract"),
      ],
      provenance: { source: "medieval/battle-sandbox", version: "1" },
    },
    {
      id: "battle.units.stop",
      title: "Stop selected units",
      categoryPath: ["Battle", "Orders"],
      repeatPolicy: "never",
      allowedDevices: ["keyboard"],
      defaults: [physical("battle.units.stop.default", "battle.units.stop", "Space")],
      provenance: { source: "medieval/battle-sandbox", version: "1" },
    },
    {
      id: "battle.formation.line",
      title: "Line formation",
      categoryPath: ["Battle", "Orders"],
      repeatPolicy: "never",
      allowedDevices: ["keyboard"],
      defaults: [physical("battle.formation.line.default", "battle.formation.line", "KeyL")],
      provenance: { source: "medieval/battle-sandbox", version: "1" },
    },
    {
      id: "battle.formation.column",
      title: "Column formation",
      categoryPath: ["Battle", "Orders"],
      repeatPolicy: "never",
      allowedDevices: ["keyboard"],
      defaults: [
        physical("battle.formation.column.default", "battle.formation.column", "KeyC"),
      ],
      provenance: { source: "medieval/battle-sandbox", version: "1" },
    },
    {
      id: "battle.camera.fit",
      title: "Fit camera",
      categoryPath: ["Battle", "Camera"],
      repeatPolicy: "never",
      allowedDevices: ["keyboard"],
      defaults: [physical("battle.camera.fit.default", "battle.camera.fit", "Digit0")],
      provenance: { source: "medieval/battle-sandbox", version: "1" },
    },
    {
      id: "battle.pause",
      title: "Pause or resume battle",
      categoryPath: ["Battle", "Simulation"],
      repeatPolicy: "never",
      allowedDevices: ["keyboard"],
      defaults: [physical("battle.pause.default", "battle.pause", "KeyP")],
      provenance: { source: "medieval/battle-sandbox", version: "1" },
    },
  ],
});

export function attachBattleInputBindings({
  canvas,
  pan,
  zoom,
  stopSelected,
  setFormation,
  fitCamera,
  togglePause,
}) {
  let disposed = false;
  let detachRuntime = () => {};

  const ready = import(INPUT_BINDINGS_BUNDLE_URL).then(
    ({ InputRuntimeController, attachKeyboardRuntime }) => {
      if (disposed) return;

      const controller = new InputRuntimeController({
        registry: BATTLE_INPUT_REGISTRY,
        getActiveContexts: () => new Set([CONTEXT_ID]),
        consumePolicy: "dispatched",
        onDispatch: (dispatch) => {
          if (dispatch.phase === "release") return;
          switch (dispatch.action) {
            case "battle.camera.panLeft":
              pan("left");
              break;
            case "battle.camera.panRight":
              pan("right");
              break;
            case "battle.camera.panUp":
              pan("up");
              break;
            case "battle.camera.panDown":
              pan("down");
              break;
            case "battle.camera.zoomIn":
              zoom(1.15);
              break;
            case "battle.camera.zoomOut":
              zoom(0.87);
              break;
            case "battle.units.stop":
              stopSelected();
              break;
            case "battle.formation.line":
              setFormation("line");
              break;
            case "battle.formation.column":
              setFormation("column");
              break;
            case "battle.camera.fit":
              fitCamera();
              break;
            case "battle.pause":
              togglePause();
              break;
          }
        },
      });

      detachRuntime = attachKeyboardRuntime(controller, {
        keyTarget: canvas,
        focusTarget: window,
        visibilityTarget: document,
        ignoreTextEntry: true,
        mode: "physical",
      });
    },
    (error) => {
      console.error("Failed to load shared input-bindings runtime", error);
      canvas.dataset.inputBindings = "unavailable";
    },
  );

  return {
    ready,
    destroy() {
      disposed = true;
      detachRuntime();
    },
  };
}
