# Medieval roadmap

## Product direction

Medieval should reproduce the *shape* that made the first Medieval: Total War compelling — a turn-based province campaign feeding into battles — without trying to reproduce proprietary assets, source code, data, or presentation.

The first playable target is deliberately compact: a two-faction, six-province campaign that can be finished in roughly 30–60 minutes. Every slice must improve that loop before the project expands outward.

## Ownership boundaries

- **`medieval-core` owns truth:** campaign state, adjacency, movement legality, economy, recruitment rules, combat resolution, AI decisions, seeded randomness, victory conditions, and serialization versions.
- **Tauri owns platform integration:** application lifecycle, local save-file access, native packaging, and later mobile/desktop integrations.
- **The webview owns projection and input:** rendering, selection, camera/view state, accessibility, and translating gestures into explicit player intents. It must not silently reimplement game rules.
- **Future real-time battles remain Rust-owned:** unit state, formation rules, morale, pathing, collision/combat outcomes, and deterministic simulation ticks. A renderer may project the simulation but cannot become the authority.

## MVP vertical slices

### 0. Foundation — current slice

- Rust workspace with platform-independent `medieval-core`.
- Tauri 2 shell suitable for desktop and mobile targets.
- Tiny campaign bootstrap with factions, provinces, armies, and event log.
- First Rust-owned state transition: end turn.
- Minimal campaign UI that only projects Rust state.
- Rust format, Clippy, and test CI.

**Exit:** the app opens, renders the six-province campaign, and ending a turn changes authoritative Rust state.

### 1. Army movement

- Select one army and an adjacent friendly/enemy province.
- Rust validates adjacency and ownership constraints.
- One move per army per turn.
- Movement into an enemy-held province creates a pending battle.
- UI exposes legal destinations returned by Rust rather than recomputing them.

**Exit:** the player can create a conflict by moving an army across a border.

### 2. Economy and recruitment

- Province income paid at the start of a faction turn.
- Small unit roster: levy, spearmen, archers, knights.
- Recruitment price, queue, and one-turn completion.
- Province/army panel exposes actionable reasons when recruitment is illegal.

**Exit:** territory generates resources and those resources become military force.

### 3. Deterministic auto-resolve

- Seeded battle resolution based on troop composition plus a small terrain/defender modifier.
- Casualties, retreat, destruction, and province capture.
- Battle report stores inputs, seed, and result for reproducibility.
- Property/unit tests cover conservation and deterministic replay.

**Exit:** moving into hostile territory completes a full strategic conquest loop without a tactical renderer.

### 4. Opponent and victory

- Narrow AI that recruits, reinforces, and attacks using the same legal commands as the player.
- AI scoring remains deterministic for a given state/seed.
- Win when all provinces are controlled; lose when no controlled province/army remains.
- New-campaign flow selects player faction.

**Exit:** a complete single-player campaign can be won or lost.

### 5. Save/load and platform acceptance

- Versioned Rust serialization for campaign saves.
- Tauri file persistence with a constrained capability surface.
- Autosave at turn boundaries plus explicit manual save/load.
- Desktop keyboard/mouse acceptance and mobile touch-layout acceptance.
- Package smoke tests for Linux, Windows, macOS, Android, and iOS where runners/devices are available.

**Exit:** the MVP is durable enough to play across sessions and package on target platforms.

## Post-MVP: tactical battle track

Only start this after slices 1–5 are coherent.

1. **Battle simulation kernel:** flat test battlefield, fixed-step clock, units, formations, movement orders.
2. **Morale and combat:** frontage, fatigue, casualties, morale shocks, routs, pursuit.
3. **Renderer:** 3D battlefield projection with camera, selection, order previews, and large-unit batching/instancing.
4. **Terrain:** height, forests, rivers, chokepoints, deployment zones.
5. **Sieges:** walls, gates, towers, capture points, pathing constraints.
6. **Campaign handoff:** campaign army composition seeds tactical battle; tactical outcome returns casualties and control changes.

## Later campaign depth

After the tactical handoff is proven, add depth incrementally: more factions and provinces, buildings, commanders/traits, diplomacy, agents, religion, rebellions, naval transport, historical events, and larger campaign maps.

## Explicit non-goals for the MVP

- Full Europe/North Africa/Middle East campaign map.
- Thousands of independently simulated soldiers.
- Real-time tactical battles.
- Sieges, naval battles, diplomacy, agents, dynasties, religion, or crusades.
- Online multiplayer.
- Historical-accuracy content pass beyond a coherent medieval-inspired sandbox.

These are valuable later, but none should delay a small complete strategy loop.
