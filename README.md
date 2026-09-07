# Medieval

A Rust-first strategy game inspired by the campaign-and-battle structure of *Medieval: Total War* (2002), built as an original implementation rather than a reuse of proprietary game code or assets.

The project starts deliberately small: a turn-based province campaign with armies, movement, recruitment, economy, and deterministic auto-resolved battles. Real-time tactical battles come only after the campaign loop is fun and stable.

## Architecture

- `crates/medieval-core` — authoritative deterministic game rules and state transitions in Rust.
- `src-tauri` — thin Tauri 2 application shell and command adapter.
- `src` — presentation/input layer; it renders Rust-owned state and sends player intents back to Rust.
- `docs/ROADMAP.md` — vertical MVP roadmap and explicit non-goals.

The core crate must stay UI- and platform-independent so it can be tested cheaply and reused by desktop/mobile shells or future multiplayer/server work.

## MVP definition

A player can start a tiny historical-inspired campaign, inspect provinces, recruit units, move an army, end turns, fight deterministic auto-resolved battles, capture provinces, and win by controlling the map. Save/load, AI turns, and a small event log complete the first playable loop.

See [`docs/ROADMAP.md`](docs/ROADMAP.md) for the implementation sequence.

## Development

Prerequisites: a current stable Rust toolchain, Node.js, and the platform prerequisites for Tauri 2.

```sh
npm install
npm run tauri dev
```

Rust-only validation:

```sh
cargo fmt --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
```
