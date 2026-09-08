# Medieval roadmap

## Product direction

Medieval should reproduce the *shape* that made the first Medieval: Total War compelling — a turn-based province campaign feeding into battles — without trying to reproduce proprietary assets, source code, data, or presentation.

Campaign is not the application shell. Medieval opens on a simple mode menu so tactical battles can later exist independently as Online Battle, without requiring a campaign save.

The first single-player target remains deliberately compact: a two-faction, six-province campaign that can be finished in roughly 30–60 minutes. Every campaign slice must improve that loop before the campaign expands outward.

## Ownership boundaries

- **`medieval-core` owns truth:** campaign state, adjacency, movement legality, economy, recruitment rules, combat resolution, AI decisions, seeded randomness, victory conditions, serialization versions, and future tactical simulation state.
- **Tauri owns platform integration:** application lifecycle, constrained local save-file access, native packaging, and later mobile/desktop integrations.
- **GitHub Pages is a Three.js demo/projection surface:** browser rendering, selection, camera/view state, accessibility, menu/navigation state, and translating gestures into explicit player intents may live there. It must not silently reimplement game rules, simulation, or trust decisions.
- **The production desktop renderer is Rust + `wgpu`:** tactical rendering, GPU resource management, batching/instancing, and frame scheduling stay on the Rust side so desktop performance does not depend on the browser or Three.js demo.
- **Future real-time battles remain Rust-owned:** unit state, formation rules, morale, pathing, collision/combat outcomes, and deterministic simulation ticks. A renderer may project the simulation but cannot become the authority.
- **`multiplayer-setup-service` owns rendezvous only:** lobby capabilities, targeted opaque WebRTC signaling, resilience helpers, TURN policy hooks, and optional peer-content coordination. It never owns Medieval gameplay truth or asset authority.

## MVP vertical slices

### 0. Foundation — complete

- Rust workspace with platform-independent `medieval-core`.
- Tauri 2 shell suitable for desktop and mobile targets.
- Tiny campaign bootstrap with factions, provinces, armies, and event log.
- First Rust-owned state transition: end turn.
- Minimal campaign UI that only projects Rust state.
- Rust format, Clippy, and test CI.

**Exit:** the app opens, renders the six-province campaign, and ending a turn changes authoritative Rust state.

### 0.5. Main menu and mode shell — complete

- Open Medieval on a small mode chooser instead of directly entering Campaign.
- Campaign loads the Rust-owned campaign only after the player chooses it.
- Add Online Battle as a first-class destination without pretending its network controls are implemented yet.
- Preview the future lobby and verified-asset readiness gate.
- Keep mode/navigation state presentation-owned; no campaign rule migrates into JavaScript.

**Exit:** Campaign and future Online Battle have independent entry points and the existing campaign remains playable through the menu.

### 1. Army movement — complete in stacked dependency

- Select one army and request its legal destinations from Rust.
- Rust validates active faction, adjacency, one-move-per-turn state, and pending-battle constraints.
- Friendly movement relocates the army immediately in authoritative Rust state.
- Movement into an enemy-held province creates a Rust-owned pending battle without resolving combat.
- UI highlights only destination IDs returned by Rust and sends explicit move intents back to Rust.
- End turn is rejected while a battle remains pending.

**Exit:** the player can create a conflict by moving an army across a border without moving campaign rules into JavaScript.

### 2. Economy and recruitment — complete in stacked dependency

- Provincial income is computed in Rust from controlled territory and paid once at each faction-turn start.
- Turn-start economy processing is idempotent for the same faction/turn pair.
- Small unit roster: levy, spearmen, archers, knights.
- Rust owns recruitment batch sizes, prices, treasury checks, duplicate-queue rejection, and one-turn completion timing.
- Recruitment orders deduct their price immediately and complete when that faction next becomes active.
- Completed recruits reinforce an army in the province or create a deterministic local army when none exists.
- Province UI displays treasury, queue state, costs, availability, and rejection reasons returned by Rust.

**Exit:** territory generates resources and those resources become military force without duplicating economic rules in JavaScript.

### 3. Deterministic auto-resolve — complete

- A Rust-owned pending battle is resolved only through an explicit `u64` seed.
- The battle kernel derives scores from troop composition and a small fixed defender advantage.
- Rust applies deterministic casualty percentages to attacker and defending armies.
- Attacker victory captures the target province; surviving defenders retreat to the first adjacent friendly province or are destroyed if no retreat exists.
- Every resolution persists a `BattleReport` containing seed, inputs, scores, before/after rosters, casualty rules, outcome, capture, and retreat information.
- The browser only submits the seed and renders the persisted report; no combat arithmetic is duplicated in JavaScript.
- Tests cover deterministic replay, casualty conservation, capture, and the browser ownership boundary.

**Exit:** moving into hostile territory completes a reproducible strategic conquest loop without a tactical renderer.

### 4. Opponent and victory — complete

- Narrow AI that recruits, reinforces, and attacks using the same legal commands as the player.
- AI scoring remains deterministic for a given state/seed.
- Win when all provinces are controlled; lose when no controlled province/army remains.
- New-campaign flow selects player faction.

**Exit:** a complete single-player campaign can be won or lost.

### 5. Save/load and platform acceptance — complete

- Versioned Rust serialization for campaign saves.
- Tauri file persistence with a constrained capability surface.
- Autosave at turn boundaries plus explicit manual save/load.
- Desktop keyboard/mouse acceptance and mobile touch-layout acceptance.
- Package smoke tests for Linux, Windows, macOS, Android, and iOS where runners/devices are available.

**Exit:** the campaign MVP is durable enough to play across sessions and package on target platforms.

## Tactical battle track

A real Online Battle depends on this deterministic battle foundation. It does **not** need to wait for the full campaign handoff.

The production renderer foundation now projects immutable `medieval-core` tactical snapshots through `medieval-renderer` into renderer-owned camera/view metadata and a batched `wgpu` instance stream. The native integration slice hosts that renderer in a dedicated desktop Tauri window with lazy GPU initialization, bounded main-thread frame scheduling, resize and lost-surface recovery, and explicit cleanup when the application window is destroyed. The next production-renderer slice is to replace the fixed preview snapshot with live Rust-owned tactical snapshots and view intents; the browser remains a separate projection surface.

1. **Battle simulation kernel — complete:** flat test battlefield, fixed-step clock, units, formations, movement orders.
2. **Morale and combat — complete:** frontage, fatigue, simultaneous casualties, morale shocks, routs, pursuit.
3. **Production desktop renderer — current:** Rust + `wgpu` battlefield projection, camera, selection, order previews, GPU batching/instancing, and native Tauri window/surface/frame lifecycle are in place; next feed live tactical snapshots and view intents through that boundary.
4. **GitHub Pages demo renderer:** Three.js projection of the same authoritative battle state/contracts for browser dogfood and public demos; no duplicate simulation truth.
5. **Terrain:** height, forests, rivers, chokepoints, deployment zones.
6. **Sieges:** walls, gates, towers, capture points, pathing constraints.
7. **Campaign handoff:** campaign army composition seeds tactical battle; tactical outcome returns casualties and control changes.

## Online Battle track

The entry point exists early, but real networking should reuse `multiplayer-setup-service` and only launch once a deterministic tactical kernel can run the same battle on both peers.

See [`ONLINE-BATTLE.md`](ONLINE-BATTLE.md) for the full contract.

1. **Private two-player lobby:** host/join/invite flow through `/lobbies` and `LobbySession`; private participant capabilities never appear in invite links.
2. **Release readiness:** both peers identify a compatible Medieval release/battle protocol and exchange compact ready state.
3. **Verified asset gate:** trusted release manifest, persistent verified cache, optional P2P seeding, multi-source chunk download, and progress based only on verified chunks.
4. **Start gate:** battle cannot start until both peers have the same compatible release and all required battle assets verified.
5. **Battle command transport:** deterministic fixed ticks, compact commands/inputs, sequence validation, and periodic state hashes over direct WebRTC.
6. **Resilience:** reuse signaling reconnect, ICE restart, and TURN fallback; gameplay bandwidth always outranks bulk asset transfer.
7. **Divergence handling:** detect state-hash mismatch and fail/recover explicitly rather than silently drifting.
8. **Competitive authority, later if needed:** friendly peer matches do not claim cheat resistance; add an authoritative game server only if competitive anti-cheat becomes a product requirement.

## Later campaign depth

After the tactical handoff is proven, add depth incrementally: more factions and provinces, buildings, commanders/traits, diplomacy, agents, religion, rebellions, naval transport, historical events, and larger campaign maps.

## Explicit non-goals for the campaign MVP

- Full Europe/North Africa/Middle East campaign map.
- Thousands of independently simulated soldiers.
- Real-time tactical battles inside campaign slices 1–5.
- Sieges, naval battles, diplomacy, agents, dynasties, religion, or crusades.
- Shipping Online Battle before the deterministic tactical kernel exists.
- Historical-accuracy content pass beyond a coherent medieval-inspired sandbox.

These are valuable later, but none should delay a small complete strategy loop.
