# Platform acceptance

This document records what Medieval actually verifies for the campaign shell. It is deliberately narrower than the future tactical renderer.

## Renderer boundary

- The GitHub Pages surface is a browser demo/projection. When the tactical 3D demo begins, that surface uses Three.js for rendering and browser dogfood only.
- The production desktop game renderer is Rust + `wgpu`. Three.js is not the desktop rendering authority and must not acquire simulation, collision, combat, formation, or asset-trust rules.
- `medieval-core` remains renderer-independent game truth. A renderer consumes authoritative state and emits player intents.

## Save persistence

- `medieval-save` owns the versioned JSON envelope and structural validation.
- Tauri owns local file I/O under its application-data directory.
- The webview can request only the fixed manual or autosave operations. It cannot provide an arbitrary path or filename.
- A completed player/opponent turn is committed to the in-memory session only after its autosave has been written successfully.
- Unsupported save versions and dangling faction/province/army references fail closed with an explicit error.

## Input/layout acceptance

The static surface keeps native HTML buttons, inputs, and selects so keyboard and touch semantics remain available without a custom interaction layer.

Deterministic browser contract checks require:

- visible `:focus-visible` treatment for buttons, inputs, and selects;
- a responsive single-column layout at narrow widths;
- coarse-pointer controls with a minimum 3rem touch target;
- save/load status announcements through an `aria-live` region.

These checks do not replace real-device dogfood; they prevent obvious regressions before it.

## Native build evidence

Pull requests run `cargo build -p medieval --release` on:

| Target host | CI runner | Evidence |
| --- | --- | --- |
| Linux desktop | `ubuntu-22.04` | Native release build, including Tauri/WebKit prerequisites |
| Windows desktop | `windows-latest` | Native release build |
| macOS desktop | `macos-latest` | Native release build |
| Android | not yet established | Tauri mobile entry point and touch-layout contract exist; Android project/package smoke still needs a configured mobile runner/device path |
| iOS | not yet established | Tauri mobile entry point and touch-layout contract exist; iOS project/package/simulator smoke still needs a configured Apple runner/device path |

Desktop CI is a native compile smoke, not a signed installer acceptance run. Android/iOS packaging must not be reported as passing until those target projects and runner/device checks actually exist.
