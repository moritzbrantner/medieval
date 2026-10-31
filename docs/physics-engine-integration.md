# Tactical geometry ownership and retirement of the physics dependency

## Owner decision (2026-10-16, revised Physics Engine /tech-steer Question 12, option A)

**Medieval does not require `physics-engine` for its current tactical proximity and geometry rules.** Keep battle simulation, movement orders, formations, melee/pursuit thresholds, targeting, morale, terrain legality, casualties and deterministic tick sequencing in `medieval-core`. Use direct checked integer geometry for simple domain thresholds; use existing domain-neutral `rust-kernels` algorithms when more complex reusable geometry or spatial indexing is warranted. Do not introduce a separate geometry implementation when a proved reusable kernel already fits.

`physics-engine` remains responsible for true physical simulation where needed elsewhere: integrating forces/motion, collision response, CCD with physical response, physical constraints and authoritative body/contact lifecycle. A future Medieval feature may adopt such a capability **only on its own explicit gameplay contract and acceptance evidence**; ordinary distances, formation footprints, line-of-sight and pathfinding do not automatically justify the full engine or a physics-owned retained query scene.

## Current code and migration status

[Medieval #76](https://github.com/moritzbrantner/medieval/issues/76) removed the dependency. `measured_points_within_distance` in `crates/medieval-core/src/tactical/geometry.rs` now applies the inclusive squared-distance predicate below directly, after the same axis rejection, with `i64` deltas and `u128` squares. The workspace manifest, the `medieval-core` manifest and `Cargo.lock` no longer reference `physics-engine`. The `physics_contact_queries` work counter keeps its name and meaning (exact checks that pass axis rejection), so the work budgets in `crates/medieval-core/tests/fixtures/tactical-work-budgets-v1.json` stay unchanged.

## Exact proximity contract to preserve

- Battlefield coordinates use `BattlePoint { x_mm: u32, y_mm: u32 }` in integer millimetres and must retain their full `u32` range. Avoid narrowing absolute coordinates to `i32` or converting to floating-point distance.
- For a nonnegative allowed range `r`, the existing inclusive sphere/point overlap corresponds to the **squared distance** predicate `dx² + dy² <= r²`, using checked/widened integer intermediate values. Preserve equality/touching, coincident positions, zero-range behavior where accepted, and early bounding-axis rejection; prevent arithmetic overflow or saturating casts.
- Preserve the actual validated limits (e.g., `attack_range_mm`, `COMBAT_CONTACT_DISTANCE_MM`, `PURSUIT_DISTANCE_MM`) and the existing semantic decisions and canonical ordering for melee, pursuit, charge, volleys and ranged attacks.
- Keep the workload counters meaningful and explain any renamed counter or changed instrumentation separately; do not silently relax performance ratchets, thresholds or canonical fixtures when removing the dependency.

## Scope boundary for future features

- **Formations / tactical footprints:** Medieval defines gameplay occupancy, deterministic collision eligibility and movement orders. Reuse geometry/spatial kernels as needed; a full physics world is not justified by footprints alone.
- **Projectiles:** Tactical hit geometry or continuous segment intersection can use reusable kernels without rigid-body simulation. Add `physics-engine` only if a specifically approved feature needs engine-owned physical motion/response.
- **Terrain:** `medieval-core::TacticalTerrain` already owns deterministic height, ground-cover and movement legality. The renderer only projects this state. Future terrain contact rules remain a Medieval product choice; rendering never becomes collision authority.
- **Presentation:** `medieval-renderer`, desktop adapters and the browser/WASM surface never compute authoritative contact or tactical outcomes.

See [battle architecture](BATTLE-ARCHITECTURE.md), [roadmap](ROADMAP.md) and [issue #76](https://github.com/moritzbrantner/medieval/issues/76). This file retains its old path for stable links, but it is now the **dependency-retirement** record rather than justification for retaining the engine.
