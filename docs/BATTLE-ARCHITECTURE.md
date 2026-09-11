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

Keeping the current integer ground coordinates is deliberate. A 3D renderer does not require the simulation to adopt floating-point GPU vectors, and terrain elevation can be introduced through an explicit deterministic terrain query instead of coupling simulation truth to presentation types.

## Ownership

### `medieval-core`

Owns battle truth: units, positions on the battlefield, formations, movement, combat, morale, fatigue, routing, future terrain effects, and deterministic ticks.

### `medieval-renderer`

Owns the 3D scene projection and GPU work: camera, world-space render snapshots, terrain meshes, soldier instances, selection/order visualization, lighting, depth, culling/LOD, and `wgpu` resources.

A render snapshot may copy authoritative metadata needed to draw the battle, but it must not contain precomputed pixel or clip-space positions.

### Platform adapters

`src-tauri` and `web-battle-wasm` own surfaces, physical input adaptation, window/canvas lifecycle, and presentation integration. Both must drive the same Rust battle rules and `medieval-renderer`; neither may implement tactical rules independently.

## Target rendering path

```text
TacticalBattle
    ↓
world-space BattleRenderSnapshot
    ↓
BattleScene / terrain / soldier instances
    ↓
Camera3d + lighting + depth
    ↓
GpuBattleRenderer (wgpu)
    ↓
Tauri surface or WebGPU canvas
```

There is one production renderer. Do not maintain a long-lived 2D renderer, Three.js battle renderer, or browser-only gameplay renderer beside it.

## Migration inventory

| Existing assumption | Decision |
| --- | --- |
| Unit render snapshots contain `clip_center` / `clip_half_extent` | **Remove now.** Snapshots become world-space only. |
| One rectangle/quad represents an entire formation | **Remove now.** The first 3D representation expands a formation into instanced soldier geometry. |
| Tactical render pass has no depth target | **Remove now.** 3D rendering always owns a depth buffer. |
| `battlefield.wgsl` receives already-projected 2D positions | **Remove now.** The shader receives 3D world geometry and renderer camera state. |
| `Camera2d` name in shared controls | **Compatibility only for this migration slice.** `Camera3d` is authoritative and the alias must disappear when input migrates. |
| Browser `projected_pixel` / `battlefield_point` affine math | **Next required migration.** Replace with `Camera3d` projection plus ray-to-terrain picking. |
| Native `viewport_to_world`, zoom-scaled hit radius, and world-rectangle drag selection | **Next required migration.** Replace with camera rays / projected selection tests. |
| Flat battlefield has no elevation source | **Keep temporarily as a terrain fixture.** Introduce a deterministic terrain contract before hills affect simulation. |
| `BattlePoint` has two ground axes | **Keep.** It is deterministic ground-domain state, not a 2D rendering commitment. |

## Current migration slice

This slice deliberately changes the structural foundation before changing every player-facing camera behavior:

1. `Camera3d` becomes the renderer camera type.
2. `BattleRenderSnapshot` contains world-space soldier centers instead of clip-space rectangles.
3. `GpuBattleRenderer` draws instanced 3D cuboids for individual soldiers plus a world-space ground mesh.
4. The render pipeline has a real depth attachment and simple directional lighting.
5. Native and browser surfaces continue to consume the same renderer while their existing ground-camera input remains source-compatible.

The temporary ground projection used during this slice is not the final camera model. It exists only so the renderer conversion can land without making pointer commands geometrically incorrect in the same commit.

## Next mandatory slice

Before adding more battle features:

1. Give `Camera3d` a real perspective tactical-camera model (position/target or equivalent, pitch/yaw, field of view, near/far planes).
2. Put world-to-screen projection and viewport-ray construction on that camera.
3. Migrate browser and native picking/order placement to ray/terrain intersection.
4. Migrate drag selection to projected 3D bounds or frustum selection.
5. Remove the `Camera2d` compatibility alias and all duplicated affine camera math.
6. Keep the renderer path singular; do not add a second experimental renderer.

Only after that convergence should formation facing, terrain height, animation, or richer combat presentation build on top.

## Acceptance rules

A tactical rendering change is structurally acceptable only when:

- authoritative game state remains in `medieval-core`;
- render snapshots are world-space and renderer-owned;
- browser and desktop consume the same Rust renderer and semantic commands;
- no player command depends on a visual approximation that disagrees with the camera;
- depth and 3D geometry are first-class, not optional demo modes;
- temporary compatibility is named and has an explicit deletion step.
