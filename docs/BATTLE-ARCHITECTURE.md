# Tactical battle architecture

## Product invariant

Medieval tactical battles are a **3D mass-battle game**. A temporary asset, primitive mesh, or simple material is acceptable while a subsystem is being built; a 2D renderer or screen-space battle model is not an acceptable substitute for the final architecture.

Every tactical slice must therefore be a smaller piece of the final 3D system rather than a parallel prototype that will later be discarded.

## Coordinate contract

`medieval-core` owns deterministic battlefield ground coordinates. They are gameplay coordinates, not presentation coordinates:

- `BattlePoint.x_mm` maps to world X.
- `BattlePoint.y_mm` maps to world Z (battlefield depth).
- World Y is elevation and is supplied by the terrain system as terrain lands.

Pixels, clip-space coordinates, projection matrices, camera state, GPU buffers, and renderer-only transforms must not enter `medieval-core`.

Keeping the current integer ground coordinates is deliberate. A 3D renderer does not require the simulation to adopt floating-point GPU vectors, and authoritative terrain elevation can later be introduced through an explicit deterministic terrain query instead of coupling simulation truth to presentation types.

## Ownership

### `medieval-core`

Owns battle truth: units, positions on the battlefield, formations, movement, combat, morale, fatigue, routing, future terrain effects, and deterministic ticks.

### `medieval-renderer`

Owns the 3D scene projection and GPU work: perspective camera, world-space render snapshots, viewport rays, renderer-only terrain geometry, soldier instances, selection/order visualization, lighting, depth, culling/LOD, and `wgpu` resources.

The current terrain-height field is intentionally renderer-owned and gameplay-neutral. It proves the geometry, elevation, and ray-intersection path without making movement or combat depend on presentation state. Before height, forests, rivers, or chokepoints affect simulation outcomes, their deterministic contract must move into `medieval-core`.

A render snapshot may copy authoritative metadata needed to draw the battle, but it must not contain precomputed pixel or clip-space positions.

### Platform adapters

`src-tauri` and `web-battle-wasm` own surfaces, physical input adaptation, window/canvas lifecycle, and presentation integration. Both must drive the same Rust battle rules and `medieval-renderer`; neither may implement tactical rules independently.

## Rendering and interaction path

```text
TacticalBattle
    ↓
world-space BattleRenderSnapshot
    ↓
BattleScene / terrain / soldier instances
    ↓
Camera3d perspective basis + viewport rays
    ├──→ GpuBattleRenderer (wgpu) → Tauri surface / WebGPU canvas
    └──→ projected selection + ray/terrain order placement
```

There is one production renderer and one camera geometry contract. Do not maintain a long-lived 2D renderer, Three.js battle renderer, browser-only gameplay renderer, or adapter-owned projection formula beside it.

## Converged 3D foundations

The original tactical preview assumptions have now been removed or contained at the correct domain boundary:

| Former assumption | Current contract |
| --- | --- |
| Unit render snapshots contained `clip_center` / `clip_half_extent` | Render snapshots contain only world-space soldier geometry and authoritative render metadata. |
| One rectangle represented an entire formation | Formations expand deterministically into instanced individual soldier geometry. |
| Tactical render pass had no depth target | The production 3D renderer always owns a depth attachment. |
| Shader received already-projected 2D positions | WGSL receives world geometry plus the renderer-owned perspective camera basis. |
| `Camera2d` existed in shared controls | Removed. Semantic controls own a `Camera3d` directly. |
| Browser used adapter-local affine projection/inverse math | Removed. Visible-unit picking uses `Camera3d::project_world_point`; orders use `ground_point_from_viewport`. |
| Native input used affine viewport conversion and ground-distance hit radii | Removed. Native click/drag selection uses projected visible anchors; orders use the same viewport-ray terrain intersection. |
| Flat renderer ground had no elevation source | Replaced by one deterministic renderer-local height field shared by wgpu geometry, unit elevation, camera targeting, and viewport picking. It remains gameplay-neutral until an authoritative core terrain contract lands. |
| `BattlePoint` has two ground axes | Kept. It is deterministic ground-domain state, not a 2D rendering commitment. |

## Perspective camera contract

`Camera3d` owns the geometry used by both rendering and interaction:

- target position in battlefield ground coordinates, with world-Y taken from the active renderer terrain surface;
- camera distance;
- yaw and pitch;
- vertical field of view;
- near/far planes;
- world-to-screen projection;
- viewport-ray construction;
- ray-to-terrain intersection for order placement.

The GPU uniform is derived from that same camera basis. Platform adapters may provide physical coordinates and semantic pan/zoom intents, but they must not reproduce projection equations.

Zoom is a camera dolly operation, not a scalar applied to clip-space geometry. Pan moves the ground target. Unit hit testing is screen-space against the same projected world anchors used by the visible scene.

## Terrain height foundation

The first terrain slice is intentionally narrow and final-shape compatible:

1. One deterministic 8×8 renderer height field supplies all current visual elevation.
2. `GpuBattleRenderer` renders those cells through the same Rust/wgpu instance pipeline as the rest of the tactical scene.
3. Soldier world geometry and interaction anchors use the same sampled terrain elevation.
4. `Camera3d` looks at the elevated terrain target and intersects viewport rays against that same height field.
5. Browser and native adapters remain unchanged; neither learns terrain math or projection math.

Terrain picking intersects each rendered cell volume directly, including visible height-step faces, and uses a 1 mm renderer-space tolerance only at geometric boundaries so projected battlefield-edge points do not disappear through floating-point roundoff. That tolerance expands only horizontal X/Z cell bounds; elevation bounds remain exact so top-surface intersections do not shift tactical destinations.

This does **not** make terrain a tactical rule. Movement, combat, morale, routing, and legality remain independent of elevation until a deterministic terrain representation is owned by `medieval-core`.

## Next vertical slices

The next work should deepen the same architecture rather than add another compatibility layer:

1. Promote terrain data into an explicit deterministic `medieval-core` contract before any terrain-dependent gameplay rule lands.
2. Add forests and rivers as core-owned terrain features with renderer projection kept separate from effects.
3. Add chokepoints and deployment zones through explicit tactical legality queries.
4. Add explicit orbit/rotation input using the existing yaw/pitch camera state.
5. Add formation facing so soldier geometry and movement direction can become meaningful in 3D.
6. Replace primitive soldier cuboids incrementally with asset-tooling-backed meshes/animation while retaining instancing/LOD boundaries.

Terrain height must become authoritative through an explicit deterministic interface before hills affect movement, combat, or line of sight.

## Acceptance rules

A tactical rendering change is structurally acceptable only when:

- authoritative game state remains in `medieval-core`;
- render snapshots are world-space and renderer-owned;
- browser and desktop consume the same Rust renderer and semantic commands;
- GPU projection, unit picking, and order placement derive from the same camera geometry;
- no player command depends on a visual approximation that disagrees with the camera;
- depth and 3D geometry are first-class, not optional demo modes;
- terrain/presentation concerns do not leak floating-point GPU types into deterministic core state.