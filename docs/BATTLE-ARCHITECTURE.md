# Tactical battle architecture

## Product invariant

Medieval tactical battles are a **3D mass-battle game**. A temporary asset, primitive mesh, or simple material is acceptable while a subsystem is being built; a 2D renderer or screen-space battle model is not an acceptable substitute for the final architecture.

Every tactical slice must therefore be a smaller piece of the final 3D system rather than a parallel prototype that will later be discarded.

## Coordinate contract

`medieval-core` owns deterministic battlefield ground coordinates. They are gameplay coordinates, not presentation coordinates:

- `BattlePoint.x_mm` maps to world X.
- `BattlePoint.y_mm` maps to world Z (battlefield depth).
- World Y is elevation. Its deterministic ground-to-height contract is owned by `medieval-core`; renderers only project it into world geometry.

Pixels, clip-space coordinates, projection matrices, camera state, GPU buffers, and renderer-only transforms must not enter `medieval-core`.

Keeping the current integer ground coordinates is deliberate. A 3D renderer does not require the simulation to adopt floating-point GPU vectors. Authoritative elevation is queried through the deterministic `TacticalTerrain` contract without coupling core truth to presentation types.

## Ownership

### `medieval-core`

Owns battle truth: units, positions on the battlefield, formations, movement, combat, morale, fatigue, routing, deterministic ticks, and the deterministic `TacticalTerrain` elevation/ground-cover contract. Forest movement is the first terrain-owned gameplay modifier; additional terrain effects remain explicit core rules.

### `medieval-renderer`

Owns the 3D scene projection and GPU work: perspective camera, world-space render snapshots, viewport rays, terrain mesh/volume projection, soldier instances, selection/order visualization, lighting, depth, culling/LOD, and `wgpu` resources.

The current terrain profile is core-owned. `medieval-core::TacticalTerrain` owns deterministic height, cell-boundary, and ground-cover queries; `medieval-renderer` is only a projection adapter for geometry and picking. Elevation is still gameplay-neutral, while forest cover now applies one explicit movement-speed rule in core. Combat, morale, and line-of-sight remain independent of terrain.

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
| Flat renderer ground had no elevation source | Replaced by the core-owned deterministic `TacticalTerrain::HeightFoundationV1` profile. wgpu geometry, unit elevation, camera targeting, viewport picking, and forest projection consume the same contract; forest cover now also feeds an explicit core movement modifier. |
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

1. `medieval-core::TacticalTerrain::HeightFoundationV1` owns the deterministic 8×8 elevation and cell-boundary contract.
2. `medieval-renderer` contains only a thin adapter over that core contract; `GpuBattleRenderer` renders the returned cells through the shared Rust/wgpu instance pipeline.
3. Soldier world geometry and interaction anchors use the same sampled terrain elevation.
4. `Camera3d` looks at the elevated terrain target and intersects viewport rays against that same height field.
5. Browser and native adapters remain unchanged; neither learns terrain math or projection math.

Terrain picking intersects each rendered cell volume directly, including visible height-step faces, and uses a 1 mm renderer-space tolerance only at geometric boundaries so projected battlefield-edge points do not disappear through floating-point roundoff. That tolerance expands only horizontal X/Z cell bounds; elevation bounds remain exact so top-surface intersections do not shift tactical destinations.

Deployment legality is also core-owned. `medieval-core` deterministically defines the attacker and defender back-third deployment zones and `TacticalBattle::deploy` rejects initial units outside their side's zone. Renderers consume those exact zones and may visualize their inner boundaries, but they do not decide legal setup positions. Arbitrary `TacticalBattle::new` construction remains available for deterministic mid-battle fixtures and replay/state reconstruction where deployment-phase validation is not applicable.

Forest cover is the first tactical terrain modifier. `ForestMovementV2` owns six deterministic forest cells. A unit that starts a simulation tick in a forest cell receives half of its normal movement budget for that tick, rounded up; the same rule is applied to normal, pursuit, and routed movement. `TacticalBattle` serializes the terrain profile so replay semantics stay explicit, while legacy battle documents that predate the field default to `HeightFoundationV1`, which remains height-only. The renderer consumes the exact forest cells and draws primitive tree proxies, but those proxies do not own collision or movement rules. Forests currently do not modify combat, morale, line-of-sight, or deployment legality, and elevation itself remains gameplay-neutral.

## Next vertical slices

The next work should deepen the same architecture rather than add another compatibility layer:

1. Add rivers as the next core-owned terrain feature, with crossing legality/effects defined before renderer decoration.
2. Derive chokepoints from explicit movement/pathing legality rather than renderer geometry.
3. Add explicit orbit/rotation input using the existing yaw/pitch camera state.
4. Add formation facing so soldier geometry and movement direction can become meaningful in 3D.
5. Replace primitive soldier cuboids incrementally with asset-tooling-backed meshes/animation while retaining instancing/LOD boundaries.

Terrain height and forest cover are authoritative through the explicit deterministic `TacticalTerrain` interface. Hills remain gameplay-neutral; any future hill, river, combat, or line-of-sight effects must land as separate core rules rather than renderer behavior.

## Acceptance rules

A tactical rendering change is structurally acceptable only when:

- authoritative game state remains in `medieval-core`;
- render snapshots are world-space and renderer-owned;
- browser and desktop consume the same Rust renderer and semantic commands;
- browser controls have end-to-end acceptance that drives real pointer/keyboard events through JavaScript, WASM, and Rust-owned control/battle state without calling control commands directly from the test;
- GPU projection, unit picking, and order placement derive from the same camera geometry;
- no player command depends on a visual approximation that disagrees with the camera;
- depth and 3D geometry are first-class, not optional demo modes;
- terrain generation/query semantics live in `medieval-core`; the renderer may own only projection/picking geometry and must not duplicate the deterministic terrain algorithm;
- terrain/presentation concerns do not leak floating-point GPU types into deterministic core state.