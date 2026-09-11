# Physics engine integration

`medieval-core` consumes `physics-engine` from the exact revision `26f75db7c033d9c25e0c16cc88e8e9c6197d3b4c`. The integration is deliberately narrow so shared physics replaces duplicate geometry without changing battle rules by accident.

## Ownership boundary

- `medieval-core` remains authoritative for tactical state, movement orders, combat thresholds, morale, fatigue, routing, casualties, and deterministic tick sequencing.
- `physics-engine` is authoritative for reusable integer collision/contact geometry. The first production consumer is melee and pursuit proximity.
- `medieval-renderer` remains projection-only. Camera, GPU geometry, and presentation floats do not become simulation truth.
- Terrain remains renderer-owned until a later gameplay slice explicitly promotes terrain collision into the core contract.

## Coordinate adapter

Tactical `BattlePoint` values use `u32` millimetres and can span a wider absolute range than the physics engine's compact public `Vec3i`. Contact is translation invariant, so the adapter rebases the left tactical point to the physics origin and maps battlefield X/Y to physics X/Z. Axis rejection happens before the checked `i32` conversion, preserving the full tactical battlefield range while keeping contact arithmetic inside the shared engine.

## Deliberate non-goals of this slice

This does not make rigid-body response authoritative for formations, add pathfinding, change fixed-step movement, add individual soldier bodies, or couple terrain height to gameplay. Those are separate contracts that should move only when their gameplay semantics are explicit and covered by deterministic regression tests.

## Next physics consumers

1. Give formations explicit gameplay footprints and use physics sweeps/contact queries to prevent tunnelling and overlap while preserving deterministic order-independent resolution.
2. Add projectiles through the engine's continuous collision detection rather than frame-sampled hit checks.
3. Promote terrain collision to `medieval-core` only when terrain affects movement or combat rules; keep rendering as a pure projection of that authoritative state.
