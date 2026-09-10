const SAVE_KEY = "medieval-campaign-save-v1";
const encoder = new TextEncoder();
const decoder = new TextDecoder();

async function instantiateRuntime() {
  const url = new URL("./pkg/medieval_web_wasm.wasm", import.meta.url);
  const response = await fetch(url);
  if (!response.ok) {
    throw new Error(`Could not load Medieval WASM (${response.status}).`);
  }

  if (WebAssembly.instantiateStreaming) {
    try {
      const { instance } = await WebAssembly.instantiateStreaming(response.clone(), {});
      return instance;
    } catch {
      // Static hosts that do not send application/wasm still work through the byte fallback.
    }
  }

  const { instance } = await WebAssembly.instantiate(await response.arrayBuffer(), {});
  return instance;
}

function browserStorage() {
  try {
    if (!window.localStorage) {
      throw new Error("localStorage is unavailable");
    }
    return window.localStorage;
  } catch (error) {
    throw new Error(`Browser campaign persistence is unavailable: ${String(error)}`);
  }
}

export async function createWasmInvoke() {
  const instance = await instantiateRuntime();
  const exports = instance.exports;

  function readResult() {
    const pointer = exports.medieval_response_ptr();
    const length = exports.medieval_response_len();
    const text = decoder.decode(new Uint8Array(exports.memory.buffer, pointer, length));
    if (exports.medieval_response_ok() !== 1) {
      throw new Error(text || "Medieval WASM command failed.");
    }
    return JSON.parse(text);
  }

  function allocateString(value) {
    const bytes = encoder.encode(String(value));
    const pointer = exports.medieval_alloc(bytes.length);
    if (!pointer && bytes.length > 0) {
      throw new Error("Medieval WASM could not allocate command input.");
    }
    new Uint8Array(exports.memory.buffer, pointer, bytes.length).set(bytes);
    return { pointer, length: bytes.length };
  }

  function releaseString(input) {
    exports.medieval_dealloc(input.pointer, input.length);
  }

  function call(name) {
    exports[name]();
    return readResult();
  }

  function callWithString(name, value) {
    const input = allocateString(value);
    try {
      exports[name](input.pointer, input.length);
      return readResult();
    } finally {
      releaseString(input);
    }
  }

  function callWithTwoStrings(name, first, second) {
    const firstInput = allocateString(first);
    const secondInput = allocateString(second);
    try {
      exports[name](
        firstInput.pointer,
        firstInput.length,
        secondInput.pointer,
        secondInput.length,
      );
      return readResult();
    } finally {
      releaseString(secondInput);
      releaseString(firstInput);
    }
  }

  function callWithSeed(name, seed) {
    exports[name](Number(seed));
    return readResult();
  }

  return async function invoke(command, args = {}) {
    switch (command) {
      case "campaign_state":
        return call("medieval_campaign_state");
      case "campaign_player_faction":
        return call("medieval_campaign_player_faction");
      case "campaign_winner":
        return call("medieval_campaign_winner");
      case "start_new_campaign":
        return callWithString("medieval_start_new_campaign", args.playerFaction);
      case "legal_army_destinations":
        return callWithString("medieval_legal_army_destinations", args.armyId);
      case "move_army":
        return callWithTwoStrings("medieval_move_army", args.armyId, args.destination);
      case "recruitment_options":
        return callWithString("medieval_recruitment_options", args.provinceId);
      case "queue_recruitment":
        return callWithTwoStrings("medieval_queue_recruitment", args.provinceId, args.unit);
      case "resolve_pending_battle":
        return callWithSeed("medieval_resolve_pending_battle", args.seed);
      case "save_campaign": {
        const document = call("medieval_export_save");
        browserStorage().setItem(SAVE_KEY, document);
        return undefined;
      }
      case "load_campaign": {
        const document = browserStorage().getItem(SAVE_KEY);
        if (document === null) {
          throw new Error("No browser campaign save exists yet.");
        }
        return callWithString("medieval_import_save", document);
      }
      case "end_player_turn": {
        const previousDocument = call("medieval_export_save");
        const nextCampaign = callWithSeed("medieval_end_player_turn", args.seed);
        try {
          const nextDocument = call("medieval_export_save");
          browserStorage().setItem(SAVE_KEY, nextDocument);
          return nextCampaign;
        } catch (error) {
          callWithString("medieval_import_save", previousDocument);
          throw error;
        }
      }
      case "open_native_battle_renderer":
      case "control_native_battle":
        throw new Error("The native tactical renderer remains desktop-only; its Rust renderer is WASM-compilable but this Pages slice does not create a browser WebGPU surface yet.");
      default:
        throw new Error(`Unsupported Medieval runtime command: ${command}`);
    }
  };
}
