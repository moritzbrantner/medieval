(() => {
  if (typeof window.__TAURI__?.core?.invoke === "function") {
    return;
  }

  const runtime = import("./wasm-runtime.js").then(({ createWasmInvoke }) => createWasmInvoke());
  window.__MEDIEVAL_RUNTIME__ = "wasm";
  window.__TAURI__ = {
    ...(window.__TAURI__ ?? {}),
    core: {
      ...(window.__TAURI__?.core ?? {}),
      invoke: async (command, args) => (await runtime)(command, args),
    },
  };
})();
