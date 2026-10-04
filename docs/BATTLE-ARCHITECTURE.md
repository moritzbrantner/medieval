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
## Campaign battle results

`TacticalBattle::campaign_result()` produces the versioned `TacticalBattleResult`
only after a campaign-seeded battle finishes. New seeds retain canonical rosters
for each source army. Tactical units retain that army ID and their initial
strength, including every chunk of a large roster, so survivors and losses are
attributed directly to the army that supplied them. Results retain the seed,
terminal winner/reason/tick, per-army and per-kind counts, and a settlement capture
when the siege objective ends the battle. Applying those results to campaign
state remains a separate core operation.

Every count conserves `initial = survivors + casualties`. Routed and escaped
counts are subsets of survivors and can overlap; pursuit casualties are a subset
of total casualties. `validate`, `from_json`, and `to_json` reject unsupported
result versions, inconsistent source rosters, and nonconserving counts. Generation
also checks that deployed initial strength matches every source roster.

Older seeds and battle records remain loadable. Newly deploying an older seed
with one source army can attach exact provenance. Aggregated older seeds with
multiple sources, and older battle records without unit provenance, cannot
produce a campaign result: the core returns `MissingArmyProvenance` rather than
inventing a division of losses. These compatibility paths preserve old battles'
composition and replay semantics.

## Campaign casualty reconciliation

`CampaignState::reconcile_tactical_casualties` checks the completed result against
its pending battle, current turn, target battlefield profile, factions, army
identities, and original per-kind rosters before committing any change. It applies
exact survivor counts to each source army and removes only destroyed sources.
The result is retained as `pending_tactical_result`; the matching pending battle
stays blocked until the strategic capture/retreat operation finishes it.

An identical retry validates the recorded projection and changes nothing.
Conflicting results, source changes, or drift after reconciliation fail before
mutation. Autoresolve and new tactical deployment reject an already reconciled
pending battle, so another casualty calculation cannot replace its outcome.
Campaign saves validate this intermediate phase, including a destroyed attacker,
and historical saves without the optional result retain their existing behavior.

Campaign saves containing these semantics use schema version 2. The reader accepts
version 1 only for unreconciled historical state and upgrades it before the next
write. A version-1 document claiming reconciled casualties is rejected. Older
readers therefore reject a version-2 save instead of ignoring the recorded result
and autoresolving an already-reduced army again.

## Versioned troop stats

`UnitCombatProfile` records a `UnitStatsVersion` and authoritative troop kind.
`unitStatsV1` defines integer attack, defense, armor and formation-resistance
factors with a 1,000 base, initial morale, movement in millimeters per tick,
charge impact, and optional missile damage/range/ammunition. Levy, spearmen,
archers and knights each have an explicit core-owned entry. Missile ammunition
counts volleys; charge impact, ammunition and formation resistance reserve the
vocabulary for their later stateful mechanics rather than adding passive charges
or ammunition depletion in this foundation.

New campaign deployment selects V1. Initialization applies its morale, movement
and attack range. Simultaneous casualty resolution composes existing frontage,
fatigue, morale and terrain with attack divided by defense plus armor; ranged
resistance uses base defense 1,000 plus armor. All scaling uses deterministic
integer arithmetic and the existing minimum casualty rule.

Tactical units serialize their profile. Historical records missing it retain
legacy behavior, including their explicit movement/range and optional kind
metadata. Unknown profile versions fail deserialization. The profile owns the
kind when present. Browser status and render snapshots project core stat values
without maintaining another stat table or computing combat in presentation code.

V1 ranged damage retains thousandths through the casualty divisor, then carries
fractional damage per attacker/target pair across combat pulses. This makes armor
observable in the default ten-file formations without rounding every shot up to
one casualty. The carry is bounded below 1,000 and serialized for replay. Historical
combat without stat profiles keeps its existing minimum-one-casualty behavior.


Formation facing is an optional canonical integer direction owned by the core.
New campaign units and newly built sandbox formations face their opposing side;
historical records without facing retain their previous contact behavior. Actual
formed movement updates facing, and a typed rotation order changes it explicitly.
The core classifies incoming contact using integer dot and cross products: the
front and rear include their 45-degree boundaries; intervening directions are
flanks. Melee contact receives a bounded flank or rear advantage scaled by the
defender's formation resistance, capped at a 75% bonus. Ranged damage is unchanged.
Renderer gold markers project that core direction; browser status projects the
core contact arc without calculating gameplay angles.

## Directional troop matchups

Explicit V1 profiles compose troop interactions with the existing integer melee
pipeline, after morale, fatigue, terrain, frontage, armor and contact direction.
Spearmen facing cavalry in their front arc deal 1,750/1,000 of their normal melee
impact; cavalry attacking a spearman front deals 600/1,000. Knights attacking levy
or archers from a flank or rear deal another 1,250/1,000, composed with the bounded
contact bonus. Facing boundaries use the same core classifier as other contact.
These are formation interactions, independent of faction, IDs and storage order;
kind metadata without a combat profile cannot activate them. Historical missing
facing receives neutral matchup factors. Missile damage continues to compose the
core armor divisor with terrain cover, including fractional volley carry. Charge
momentum remains a separate subsequent mechanic rather than a passive matchup.

Profiled melee preserves thousandths through all factors and carries the remaining
fraction per attacker/defender pair, bounded below 1,000 and serialized. Thus
frontal spear resistance also matters at the default ten-file campaign frontage
instead of every weak contact being rounded up to one casualty. Historical combat
without profiles retains that minimum-casualty rule.

## Cavalry charge transitions

New profiled knights begin `ready`. Engaging a formed enemy advances through
`approaching` to `charging` after 4,000 mm of uninterrupted movement on open ground,
with the target in the facing front cone and no pathing detour or intercepting
formation. Run-up uses integer displacement, conservatively taking the larger
coordinate change for diagonal travel, and saturates at the threshold.

Arrival records `contact` momentum. Only a full run-up adds the profile's bounded
charge impact to that engagement target's next simultaneous melee pulse. Frontal
profiled spearmen suppress the added impact while retaining the normal matchup
counter. Short approaches receive no charge bonus. The pulse consumes contact and
starts 40 ticks of `recovering`; stopping, turning, changing formation or target,
cover, detours and interception interrupt momentum. Repeating the same engagement
order does not restart momentum or recovery. Core serialization and browser status
retain charge state; historical records lacking it do not acquire charge bonuses.

## Finite missile ammunition

New V1 missile formations initialize the core profile's volley budget (30 for
archers). An authoritative combat pulse spends exactly one volley only when the
formed unit fires at a valid formed enemy in range and is not in melee. Fractional
damage still consumes its volley; selection, orders, paused frames, invalid targets,
out-of-range movement and melee do not. Zero ammunition suppresses ranged firing
and reduces engagement stopping distance to melee range, so exhausted archers can
close and fight with their existing melee stats. Ammunition cannot underflow.

Remaining ammunition serializes with the tactical unit and replay. Historical
records lacking it retain previous unlimited firing, including legacy sandbox
fixtures. Render snapshots and browser status project remaining volleys; the unit
row displays the count or exhaustion without deciding firing eligibility.

Played source-aware results aggregate initial and remaining volleys per source
army and troop kind, including chunked formations. Validation checks the canonical
profile budget and remaining <= initial. Result/save records without the optional
resource field remain readable. Campaign boundary saves preserve recorded tactical
ammunition; recruitment and future battle deployment initialize a fresh tactical
budget rather than treating arrows as a strategic campaign inventory.

### Precise formation orders

`FormationOrder` is a core-owned, serializable semantic command. Rotation halts
travel and engagement; quarter turns use exact integer facing vectors. A
move-and-face order turns along its actual travel direction, then applies the
requested facing on arrival. Omitting that facing preserves the facing held when
the order was issued. The queued arrival facing survives serialization and is
cleared by a replacement movement, engagement, withdrawal, or routing order.

Requested frontage resolves to a line with whole one-metre files, rounded down
and capped at the surviving soldier count. Widths below one metre are rejected.
The existing formation footprint must fit the battlefield and avoid impassable
terrain, siege walls, and a closed gate at the current and requested positions.
Facing determines combat arcs and the gold direction markers; footprint geometry
uses the existing file/rank axes. Every multi-unit command is validated on a core
candidate and committed atomically. Browser and native adapters submit these
commands through shared tactical controls: Q/E turns, Shift + right-click moves
with the current facing on arrival, and the frontage request supplies a width.

### Group movement and queued attack-move

`GroupMovementOrder` uses the selected units' integer centroid as its destination
anchor. Stable unit-ID ordering and relative offsets preserve mixed formations'
layout. Queued orders use the last planned destination for each unit, preserving
that layout even while units are moving at different speeds. Coincident legacy
positions get deterministic grid slots with one-metre spacing around the widest
and deepest selected footprint. Full-footprint validation applies to every
resolved destination; one rejection leaves the entire group unchanged.

Each unit carries at most 16 pending `MovementWaypoint`s, excluding its current
order. Pending waypoints and movement mode survive serialization. Normal orders,
engagement, Stop, rotation, withdrawal, routing, and destruction clear the queue
as appropriate. Formation changes also validate pending destinations. Ctrl +
right-click appends a waypoint. F or the Attack-move button arms the next ground
order; Ctrl can append that attack-move waypoint too. A successfully issued
movement order consumes the arming state, and Stop or losing selection clears it.

Attack-move acquires only formed enemy units within the greater of five metres
and the unit's effective attack range. It chooses the nearest by squared integer
distance, breaking ties by unit ID, and keeps that target while it remains formed
and inside that radius. The original destination remains authoritative while
combat interrupts travel. When the target is defeated, routed, or leaves the
radius, acquisition runs again and travel resumes. Reaching the destination ends
attack-move and allows the next waypoint to start. Explicit engagement retains
its existing pursuit semantics.
