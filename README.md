# Medieval

A Rust-first strategy game inspired by the campaign-and-battle structure of *Medieval: Total War* (2002), built as an original implementation rather than a reuse of proprietary game code, names, or assets.

The current vertical target is a real 3D mass-battle layer backed by deterministic Rust simulation. The compact province campaign remains the strategic layer that will feed those battles once the tactical architecture is ready. GitHub Pages and the desktop shell are product surfaces for the same Rust-owned battle rules and `wgpu` renderer; browser JavaScript is only a platform/input adapter.

The current product and acceptance target is desktop plus WebGPU browser dogfood. Android/iOS packaging, mobile-specific input, and mobile layout validation remain deferred until mobile becomes a deliberate product priority.

## Architecture

- `crates/medieval-core` — authoritative deterministic campaign and battle rules/state transitions in Rust.
- `crates/medieval-renderer` — renderer-owned world-space battle snapshots, 3D scene preparation, and `wgpu` GPU rendering.
- `src-tauri` — desktop platform adapter and native tactical surface/input integration.
- `web-battle-wasm` — WebGPU/WASM tactical surface adapter using the same Rust core and renderer.
- `web` — campaign/menu/browser presentation and physical-input adaptation; it does not own game rules.
- `docs/BATTLE-ARCHITECTURE.md` — non-negotiable 3D tactical architecture and the active cleanup/migration boundary.
- `docs/ROADMAP.md` — broader campaign, tactical, and online-battle roadmap.

The core remains UI- and platform-independent. Tactical rendering is also singular: desktop and browser consume `medieval-renderer` rather than maintaining separate gameplay renderers.

## Current playable surfaces

The compact campaign already supports armies, movement, recruitment, economy, deterministic auto-resolved battles, AI turns, victory conditions, and save/load.

A single-player tactical sandbox is also available on GitHub Pages and through the desktop battle surface. Its battle simulation, semantic controls, and GPU rendering are Rust-owned. The tactical renderer is now being migrated from the original 2D formation projection to the final 3D architecture; see [`docs/BATTLE-ARCHITECTURE.md`](docs/BATTLE-ARCHITECTURE.md) for the exact migration contract.

## Development

Prerequisites: rustup using the repository-pinned Rust toolchain in `rust-toolchain.toml`, plus the platform prerequisites for Tauri 2 on desktop.

```sh
cargo install tauri-cli --version "^2.0.0" --locked
cargo tauri dev
```

Validation:

```sh
cargo fmt --check
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
cargo test --locked --workspace
node --test web-tests/*.test.mjs
```
