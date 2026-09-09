# Medieval

A Rust-first strategy game inspired by the campaign-and-battle structure of *Medieval: Total War* (2002), built as an original implementation rather than a reuse of proprietary game code, names, or assets.

The project starts deliberately small: a turn-based province campaign with armies, movement, recruitment, economy, and deterministic auto-resolved battles. Real-time tactical battles come only after the campaign loop is fun and stable.

The current product and acceptance target is desktop. Android/iOS packaging, mobile-specific input, and mobile layout validation are explicitly deferred until mobile becomes a deliberate product priority.

## Architecture

- `crates/medieval-core` — authoritative deterministic game rules and state transitions in Rust.
- `src-tauri` — thin Tauri 2 application shell and command adapter.
- `web` — presentation/input layer; it renders Rust-owned state and sends player intents back to Rust.
- `docs/ROADMAP.md` — vertical MVP roadmap and explicit non-goals.

The core crate stays UI- and platform-independent so it can be tested cheaply and reused by the desktop shell today or future multiplayer/server/platform work later.

## MVP definition

A player can start a tiny campaign, inspect provinces, recruit units, move an army, end turns, fight deterministic auto-resolved battles, capture provinces, and win by controlling the map. Save/load, AI turns, and a small event log complete the first playable loop.

The initial foundation already proves the ownership boundary: the UI can render campaign state and request an end turn, while Rust owns the actual state transition.

See [`docs/ROADMAP.md`](docs/ROADMAP.md) for the implementation sequence.

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
```
