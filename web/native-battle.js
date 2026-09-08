const nativeBattleButton = document.querySelector("#open-native-battle");
const nativeBattleStatus = document.querySelector("#native-battle-status");

nativeBattleButton?.addEventListener("click", async () => {
  const invoke = window.__TAURI__?.core?.invoke;
  if (!invoke) {
    nativeBattleStatus.textContent = "The native renderer preview is available in the desktop app.";
    return;
  }

  nativeBattleButton.disabled = true;
  nativeBattleStatus.textContent = "Opening the Rust/wgpu tactical renderer…";
  try {
    await invoke("open_native_battle_renderer");
    nativeBattleStatus.textContent = "Native tactical renderer opened in its own desktop window.";
  } catch (error) {
    nativeBattleStatus.textContent = `Could not open native tactical renderer: ${String(error)}`;
  } finally {
    nativeBattleButton.disabled = false;
  }
});
