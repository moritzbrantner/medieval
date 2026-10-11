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

## Pause, simulation speed and tick scheduling

The browser battle offers four documented simulation speed multipliers: **0.5×**, **1×** (default), **2×** and **4×**. Pause and simulation speed are tick scheduling only. They decide how many fixed `TACTICAL_TICKS_PER_SECOND` ticks run for a span of wall time, never what a tick does. A battle at the same tick has the same simulation truth (units, siege capture state, battle state and outcome) at every speed and while paused.

- `web-battle-wasm` owns the scheduler. JavaScript passes the raw `requestAnimationFrame` timestamp to `battle_sandbox_frame(timestamp)` and selects a speed with `battle_sandbox_set_speed(multiplier)`, which only accepts the documented multipliers. JavaScript does no tick math.
- At speed `s`, each frame adds `elapsed_ms × 20 / 1000 × s` ticks to an accumulator and runs its whole ticks. A frame delta is clamped to 250 ms and the per-frame budget is `ceil(5 × s)` ticks, so one slow frame cannot cause a catch-up spiral.
- Pause runs no ticks. Resuming starts a new frame interval, so the time spent paused is not replayed.
- Ticks run in batches that split at the once-per-second opponent replanning boundary. The opponent replans at the same tick no matter how frames group the ticks (for example 4× with 37.5 ms frames gives three ticks per frame). Combat pulses and siege capture are already keyed to the core tick.
- A terminal outcome stops the scheduler mid-batch and pauses the battle.

Speed and pause are sandbox presentation and scheduling state. They are reported in the sandbox status (`paused`, `speedMultiplier`) but are not part of `TacticalBattle`, campaign results or saves.

## Tactical battle HUD

The browser HUD only renders the Rust sandbox status and forwards input:

- **Battle status** shows the run state, the player and opponent sides (`playerSide` / `opponentSide`: attacker or defender) and, only in a siege, a capture progress bar from `siege.capture.progress` out of `siegeCaptureMaxProgress` (core `SIEGE_CAPTURE_MAX_PROGRESS`). A terminal state with reason `siegeCapture` gets its own text, separate from a defeated force or a withdrawal.
- **Selected units** shows one card per Rust-selected unit with its kind, soldiers, morale, fatigue, ammunition, formation and current order. The order is derived in Rust from core unit state (`order.kind`: `hold`, `move`, `attackMove`, `engage` with `targetUnitId`, `attackGate`, `withdraw`, `rout`, `destroyed` or `escaped`).
- **Control groups** are shared `TacticalControls` state and are reported as `controlGroups`. As on desktop, Ctrl+1–9 assigns the selection, 1–9 recalls a group and Shift+1–9 adds it to the selection. Digit 0 stays bound to fit camera in the browser. Each group is a focusable button that recalls it. Resetting the battle clears the groups.

## Deployment phase

Supported single-player battles (the browser sandbox at every battlefield location and for every legal muster, a campaign Fight in the browser, and both desktop battle windows) open in the core-owned deployment phase, `TacticalBattleState::Deployment` (`{"phase":"deployment"}` in status and saves). `TacticalBattle::into_deployment` enters it from a freshly built battle and rejects an initial layout that already breaks the placement rule on either side.

- **Frozen clock.** No tick runs before the start: `advance_ticks` is a no-op in deployment and every order except formation edits and Withdraw is refused (`BattleNotStarted`). The schedulers in `web-battle-wasm` and `src-tauri` also drop the wall time spent deploying, so it is not replayed afterwards. JavaScript keeps forwarding raw frame timestamps and never gates the clock itself.
- **Start battle exactly once.** `TacticalBattle::begin_battle` (shared control `startBattle`) leaves deployment once: the deployed layout is the tick-0 layout, the completion rules become active and ticks run at the normal schedule from the next frame. A second start, and any deployment edit after the start, is refused (`DeploymentClosed`) and changes nothing. The browser's Start battle button and the desktop window's Enter key both send `startBattle`; on the start the opponent plans its first orders on the deployed layout.
- **Placement rule (owner decision 2026-10-11, option A).** A formation is legal only if its whole `FormationFootprint` lies inside its own side's deployment zone (edges inclusive) and does not overlap or touch another formation of the same side. The rule is checked by the authoritative core, not the page, for every deployment edit: `deploy_formation` / `deploy_formations` placements (shared control `deploySelected`, and a plain right-click move during deployment), rotation and facing changes (`Rotate`, `Turn`, Q/E), and frontage and formation changes (`Frontage`, Line/Column). A rejected edit fails with `InvalidDeploymentPlacement { unit_id, reason }`, where the reason is `OutsideZone` or `FriendlyOverlap` (zone first when both apply); it changes nothing, and batches are atomic.
- **In-place edits.** During deployment, Rotate/Turn change only the facing, Frontage and Line/Column only the formation, and `MoveAndFace` places the formation instantly with the requested facing. Placement is instant: no destination, no march, no tick.
- **Footprint model.** The footprint is today's axis-aligned file/rank rectangle, independent of facing, so a rotation in place always keeps a legal footprint. A facing-dependent footprint would be checked by the same rule without API changes.
- **Several selected formations.** `deploySelected` with one selected formation places it exactly; with several, they keep their relative anchor offsets and their anchor centroid lands on the target. The resulting layout is validated as a whole, so a group may shift over its own former ground.
- **Withdraw.** Withdraw is allowed during deployment. A side that withdraws before the start refuses battle: every surviving formation escapes at once with full strength, and the battle finishes at tick 0 as a `withdrawal` won by the other side.
- **Terrain and structures.** The owner rule covers zone and friendly overlap only. Construction already requires each anchor to stand on passable ground and off siege structures, and placements keep that requirement (`UnitOnImpassableTerrain` / `UnitOnImpassableSiegeStructure`) after the two rule checks.
- **Enemy placement.** The opponent uses the existing deterministic layouts (the sandbox's fixed opponent list and campaign packing), held to the same rule; player edits never move it. Selectable AI deployment profiles are #143.
- **Starting layouts.** The browser sandbox stacks the muster along x = 16 m: each footprint starts 3 m after the previous one, beginning 8 m from the near edge, which keeps every affordable muster legal at every location. The sandbox opponents stand at x = 83, 84 and 80 m, and the desktop sample at x = 16 m and 84 m.
- **Browser.** The status reports `battleState.phase` and each unit's `footprint` (the core `FormationFootprint`). The page shows a region named Deployment with the Start battle button while deploying, reports Rust rejections in the page alert, and does no zone or footprint maths. Fog (#132 B): during a campaign deployment the unit list does not print exact opponent soldier counts. Redacting the status JSON itself is left to #132.
- **Desktop.** Both desktop battle windows open in deployment through the same shared controls, so a right click places formations there too. The minimal Start control is Enter. A full desktop deployment editor is a follow-up.

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

Deployment legality is also core-owned. `medieval-core` deterministically defines the attacker and defender back-third deployment zones and `TacticalBattle::deploy` rejects initial units outside their side's zone; the [deployment phase](#deployment-phase) then holds whole formation footprints to those zones. Renderers consume those exact zones and may visualize their inner boundaries, but they do not decide legal setup positions. Arbitrary `TacticalBattle::new` construction remains available for deterministic mid-battle fixtures and replay/state reconstruction where deployment-phase validation is not applicable.

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

## Strategic tactical outcomes

`CampaignState::apply_tactical_battle_result` commits casualty reconciliation and
strategic consequences atomically. Field battles and sieges on the versioned
fortification profiles (see `docs/PROVINCE-DEFINITIONS.md`) follow the existing
campaign rule: an attacker victory captures
the target province and moves its surviving attacking army there. Capture cancels
the former owner's recruitment in that province. Losing survivors retreat to a
friendly neighbor of the battle province, preferring the attacker's entry province
when legal and otherwise choosing the lowest stable province ID. This choice is
independent of province, army, and adjacency storage order.

A force with no friendly retreat destination surrenders and is removed. Its exact
survivor roster is recorded separately from tactical casualties, preserving the
accounting boundary between battlefield losses and strategic surrender. A draw
preserves ownership, returns surviving attackers through the same legality check,
and leaves defending survivors in their province.

The matching pending battle and intermediate result clear once. A persisted
`TacticalCampaignReport` retains the tactical result, capture, retreats, surrender
rosters, and campaign winner queried after all consequences. An identical replay
returns that report without applying casualties, moving armies, or adding another
report/log entry. Historical report validation does not depend on armies still
existing in later turns. Tactical outcomes are written only in save schema 2.

## Pending battle durability

Campaign save schema 2 preserves the pre-battle boundary through pending battle
identity, campaign army rosters and province battlefield context. Loading rebuilds
exactly the same source-aware tactical seed; schema 1 pending saves migrate to 2.
Save validation rejects illegal hostile-movement provenance, incompatible staged
results and future completed reports before replacing the live session.

Hostile movement persists its pending candidate before making it live. Fight and
Auto-resolve persist their completed campaign candidate through the same save
boundary. Browser storage failures restore the previous Rust save; desktop writes
complete before committing the candidate session. Leaving an unfinished tactical
session keeps the pre-battle save, so loading offers either choice again. This
boundary intentionally does not serialize a running renderer or mid-battle state.

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

## Siege gate assault

A siege gate has core-owned integrity (`SiegeBattleState::gate_integrity`,
serialized as `gateIntegrity`) out of `SIEGE_GATE_MAX_INTEGRITY` (1,000, the
same thousandths scale as capture progress). Sieges recorded without the field
load with an intact gate; a gate recorded as destroyed loads with zero.

`TacticalBattle::issue_gate_attack_order(unit_id)` is the semantic attack-gate
order. It is legal only for a formed attacker unit while the gate is closed; a
defender, an open or destroyed gate, a field battle and a finished battle are
rejected without changing the battle. Like an engagement order the unit
approaches from any distance, to the gate's center line just outside the gate on
its own side of the wall, and holds within reach
(`COMBAT_CONTACT_DISTANCE_MM` of the gate footprint). A move, engagement,
withdrawal, rout or destruction replaces the order. A unit pressed against the
gate under any other order never damages it.

On each combat pulse every formed attacker unit holding the order within reach
deals its profile's gate damage: 100 for a palisade (one unit breaches it in
10 s) and 50 for stone walls (20 s). Like capture it counts units, not soldiers.
Integrity only falls, and on the pulse it reaches zero the gate becomes
`SiegeGateState::Destroyed`, which is traversable through the existing siege
pathing contract (`is_passable_at`, `movement_waypoint`); every gate-attack
order then ends. The opponent planner gives an attacking unit whose nearest
enemy is behind a closed gate the same order; defenders never target their own
gate. The renderer keeps projecting `gate_traversable`, and the HUD shows the
order as `attackGate`.

## Siege walls and towers

Walls are core-owned barriers for movement and melee. A wall position is never
a legal destination and formed units on opposite sides of the wall line do not
fight in melee or pursue through it, even across the 1 m palisade (thinner than
`COMBAT_CONTACT_DISTANCE_MM`) (`SiegeBattleState::wall_separates`). They meet
only where the line between them crosses the wall line inside an open or
destroyed gate or a placed ladder cell (see "Siege ladders"); a closed gate
separates them too and is attacked through the gate assault order instead. A melee unit
whose engagement target stands across a wall keeps following the siege path
toward the gate instead of holding against the wall. Missiles still fly over
walls, and an archer pressed against the wall is not in melee contact, so it
keeps firing.

A siege tower is *staffed* while at least one formed defender unit stands inside
its footprint circle, the same position count the capture point uses; attackers
never staff it. On each authoritative combat pulse a staffed tower fires one
volley at the nearest formed attacker unit within `SiegeProfile::tower_range_mm`
of the tower center (ties broken by unit id). The volley uses the stat-profile
ranged rule with the tower as the shooter: the profile's volley frontage at
base missile damage, elevation from the tower to the target, the target's
ground cover and the target's armor, with fractional damage carried per tower
and target (`towerDamageCredit`, omitted while empty, so older saves load
unchanged). Losses are capped per tower per pulse, flow through the normal
casualty, morale and rout application, never hit defenders, and never spend the
staffing unit's ammunition or fatigue it.

| Profile | Range | Volley frontage | Cap per pulse |
| --- | --- | --- | --- |
| `PalisadeV1` | 20 m | 12 slots (≈1 unarmored soldier per pulse) | 2 |
| `StoneWallsV1` | 30 m | 24 slots (≈2 unarmored soldiers per pulse) | 3 |

For comparison, a 10-file V1 archer line fires 10 slots up to 25 m. Tower
activity is derived from authoritative unit state, so it needs no stored state:
`TacticalBattle::siege_tower_activity()` returns one `SiegeTowerActivity`
(`staffed`, `target_unit_id`) per layout tower, or `None` outside a siege. The
renderer copies `staffed` into `RenderSiegeTower::active`; presentation never
reads the tower range or casualty rules.

## Siege ladders

Scaling ladders are the one wall-access mechanic (#125). They are not an
engine entity: every formed attacker foot unit carries one ladder per battle;
mounted `Knights` and defenders never place or climb ladders
(`TacticalUnit::can_use_ladders`).

`TacticalBattle::issue_ladder_order(unit_id, at)` is legal only in a running
siege, for an eligible formed unit that has not placed its ladder, with `at`
inside a wall segment (never the gate, open ground or outside the battlefield)
and the ladder foot on the unit's side passable. Like the gate-attack order the
unit approaches from any distance to that foot and a move, engagement, gate
attack, withdrawal, rout or destruction replaces the order. Rejected orders
(`LadderUnavailable`, `InvalidLadderPosition` and the shared errors) change
nothing. The ladder exists from the tick the unit stands on the foot; it is
never destroyed or removed.

A ladder opens one cell (`SiegeLadder::area`): the full wall thickness across
and 3 m along the wall centered on `at` (`SIEGE_LADDER_HALF_SPAN_MM`, capped at
a quarter of the gate span so it stays narrower than the gate), shifted to stay
inside its wall segment. Only units that can use ladders stand in it; it is
still never a move destination, and a destination on a ladder is approached at
its foot. While the gate is not traversable, an eligible unit whose path
crosses the wall routes to the nearer foot of the nearest ladder (ties by
placement order), steps onto the ladder edge, crosses the wall thickness at
`thickness / SiegeProfile::ladder_climb_ticks()` mm per tick (at least 1) and
continues from the far foot. Climbing is position-only state, so a save made
mid-climb continues exactly.

| Profile | `ladder_climb_ticks` | Wall thickness (100 m field) | Ticks on the ladder |
| --- | --- | --- | --- |
| `PalisadeV1` | 60 (3 s) | 1 m | 63 |
| `StoneWallsV1` | 100 (5 s) | 2 m | 101 |

A walking V1 foot unit crosses the same thickness in about a second. The climb
takes at least `ladder_climb_ticks` whenever the wall is at least that many
millimetres thick (every V1 profile on a battlefield 10 m wide or more).

Defenders contest a ladder in two ways. Melee is allowed where the line
between two units crosses the wall line inside a ladder cell (exact rational
comparison, independent of argument order); the wall elsewhere still
separates. A formed defender within `COMBAT_CONTACT_DISTANCE_MM` of the inner
(east) foot *holds* the ladder: climbers stop at the top edge of the wall and
no attacker leaves the ladder onto the defenders' side until the top is clear.
Holding is derived from the unit state at the start of each movement phase, so
it needs no stored state. `TacticalBattle::siege_ladders()` returns the
`SiegeLadder` read model (`area`, `held_by_defender`) in placement order, or
`None` outside a siege. Placed ladders serialize as `siegeLadders` and a pending
order as the unit's `ladderOrder`, both omitted while empty, so battles without
ladders keep the exact pre-ladder document. Presentation never reads the climb
time rule; ladder drawing in 3D and opponent-AI ladder use (#128) are
follow-ups, and withdrawal still needs a traversable gate.

## Siege engines

Siege depth 5 (#126) adds a battering ram and one ranged engine, a catapult
(`SiegeEngineKind::{BatteringRam, Catapult}`). Like ladders, engines are not
free entities: each is crewed by one existing attacker unit, stands wherever
its crew stands, moves only with the crew's ordinary movement and is used only
through orders issued to the crew.

**Availability is explicit.** Only
`TacticalBattle::deploy_siege_with_engines(battlefield, units, location,
profile, Vec<SiegeEngineSetup>)` assigns engines. A crew must be a formed
attacker foot unit (never a defender or mounted `Knights`) and one unit crews
at most one engine; an unknown crew is `UnitNotFound` and every other invalid
setup is `SiegeEngineUnavailable(crew)`. An empty setup is exactly
`deploy_siege_with_profile`, and every other construction path (sandbox
`deploy_siege*`, campaign seeds) has no engines. Choosing equipment in the
browser or sandbox UI is a follow-up. Engine crews do not climb ladders
(`LadderUnavailable`, and they never route over placed ladders).

**Battering ram.** `issue_ram_order(unit)` is the gate-attack order for the
formed crew of a ram (`SiegeEngineUnavailable` for any other unit, then the
usual gate-attack legality: closed gate, attacker, running battle). The crew
approaches from any distance and on every combat pulse within
`SIEGE_GATE_ATTACK_DISTANCE_MM` deals `SiegeProfile::ram_damage_per_pulse`
instead of the unit gate damage; a ram crew under a plain gate-attack order
also brings its ram. The order ends with the gate, a move or engagement order,
rout or destruction, exactly like a gate assault.

**Catapult.** `issue_siege_engine_fire_order(unit, SiegeEngineTarget)` holds
the crew in place and fires at the closed gate (aimed at its center) or a
formed enemy unit. It is legal only for the formed crew of a catapult with
stones left (`SiegeEngineUnavailable` otherwise), against a closed gate
(`SiegeGateNotClosed`) or a formed enemy (`UnitNotFound`,
`InvalidSiegeEngineTarget`), within
`CATAPULT_MIN_RANGE_MM..=CATAPULT_MAX_RANGE_MM` of the crew (Euclidean,
inclusive; `SiegeEngineTargetOutOfRange`). Walls never block the indirect
shot. On each combat pulse a formed crew first reloads one pulse; a loaded
catapult then fires if its order is still valid and the target in range, spends
one stone, records `last_shot_tick` and waits exactly `CATAPULT_RELOAD_PULSES`
pulses. Engines start loaded, so the first shot lands on the first pulse after
the order. A gate hit removes exactly `SiegeProfile::catapult_gate_damage_per_shot`
(gate and ram damage of one pulse add up; the gate is destroyed on the pulse
integrity reaches zero). A unit hit reuses the tower volley rule with the crew
as shooter (`CATAPULT_SHOT_FRONTAGE` = 60 frontage slots, elevation, ground
cover and armor, `RANGED_CASUALTY_DIVISOR`), removes at least one soldier and
joins the pulse's shared casualty and morale resolution. A move, engagement,
gate-attack or ladder order, rout, withdrawal or destruction of the crew, a gate
that is no longer closed or a target that is no longer formed ends the fire
order; an empty catapult neither fires nor accepts orders. Rejected orders
change nothing.

| Rule | `PalisadeV1` | `StoneWallsV1` |
| --- | --- | --- |
| Unit `gate_damage_per_pulse` | 100 (10 pulses) | 50 (20 pulses) |
| `ram_damage_per_pulse` | 250 (4 pulses) | 125 (8 pulses) |
| `catapult_gate_damage_per_shot` | 200 (5 stones) | 100 (10 stones) |

| Catapult constant | Value |
| --- | --- |
| `CATAPULT_MIN_RANGE_MM` | 30 000 (30 m) |
| `CATAPULT_MAX_RANGE_MM` | 150 000 (150 m, beyond tower and archer reach) |
| `CATAPULT_RELOAD_PULSES` | 5 (5 s between shots) |
| `CATAPULT_AMMUNITION` | 12 stones (enough for either gate) |
| Unit hit on an unarmored target | 5 soldiers |

A ram is the fast close breach that exposes its crew at the gate; the catapult
is the slow stand-off option (a stone gate in 46 s from out of tower range).

**Read model and persistence.** `TacticalBattle::siege_engines()` returns
`SiegeEngineStatus { kind, crew_unit_id, position, ammunition (None for a ram),
pulses_until_loaded, last_shot_tick }` in setup order, or `None` outside a
siege. Engine state serializes as `siegeEngines` and a pending fire order as
the unit's `siegeEngineFire`, both omitted while empty, so battles without
engines keep the exact pre-engine document; a save made mid-reload continues
identically. Loading clamps stones and reload to the rule bounds and drops
engines from field battles. The renderer copies the read model into
`BattleRenderSnapshot::siege_engines` (`RenderSiegeEngine { kind, position,
ammunition, last_shot_tick }`) and draws a wooden body per engine and a raised
catapult arm for a moment after `last_shot_tick`; presentation never reads the
range, reload, ammunition or damage rules. Tower integrity, engine destruction
by enemy fire, opponent-AI engine use and the browser setup UI are out of scope.

## Staged siege objectives

Siege depth 6 (#127) lets one siege hold several capture points. Every siege
profile owns an ordered list of capture objectives, outer to inner, and
`TacticalBattle::siege_objectives()` returns it as `SiegeObjectiveStatus {
center, radius_mm, progress, capturing_side, captured_by, unlocked }`, or
`None` outside a siege. Like the tower, ladder and engine read models it is
derived from core state; presentation only projects it.

**Profiles.** `PalisadeV1` and `StoneWallsV1` are single-point profiles: one
objective, always unlocked, equal to `layout.capture_point` with the `capture`
state. Their layout and save documents and their capture timeline are exactly
the pre-#127 ones (the new fields below are omitted for them), so older saves
load and resume unchanged. Profiles are never changed in place, so the
multi-objective layout is a new profile, `SiegeProfile::StagedKeepV1`
(serialized `stagedKeepV1`): the `StoneWallsV1` wall, gate, towers and rules
with three objectives on the defender side at 62 %, 76 % and 90 % of the
battlefield width on the center line (outer bailey, inner bailey, keep), each
with the usual radius (a twentieth of the shorter side). The geometry
serializes as `layout.stagedObjectives` and the per-objective state as
`objectiveCaptures`.

**Rules.**

- *Ordered unlock.* Only the first objective starts unlocked. A later
  objective unlocks when the attacker captures the objective before it; until
  then it accrues nothing for either side. Only attacker captures unlock: a
  defender who captures (or holds) an earlier objective never unlocks the next
  one. Because a captured objective stays captured, a defender capture of an
  earlier objective closes the capture path for good, and the battle is then
  decided by rout or withdrawal.
- *Per-objective capture is the existing rule.* Once per combat pulse, formed
  units of exactly one side inside the active objective (the first one the
  attacker has not captured) gain `SIEGE_CAPTURE_PROGRESS_PER_PULSE` for that
  side, restarting from zero when the holding side changes. A contested
  objective (formed units of both sides inside) is left exactly unchanged, so
  contested capture fails closed. A captured objective stays captured, even
  when the enemy reoccupies it.
- *Victory only from the final objective.* Capturing an earlier objective
  never finishes the battle. A started battle finishes with
  `TacticalFinishReason::SiegeCapture` on the pulse the final objective is
  captured, won by its captor. With one objective this is exactly the
  single-point rule.

**Legacy fields.** For `StagedKeepV1`, `layout.capturePoint` is the final
objective (the keep, which decides the battle), and `capture` mirrors the
active objective: the first one the attacker has not captured, or the final
one once all have fallen. `SiegeBattleState::active_objective()` returns that
objective; the renderer projects it as `RenderSiegeSnapshot::capture` and every
objective, in core order, as `RenderSiegeSnapshot::objectives` (a read-only,
iterable and indexable `RenderSiegeObjectives` of `RenderSiegeCapture`, each
with `unlocked`), drawing one marker per objective. Victory and
settlement-capture provenance read the final objective
(`final_objective_captured_by`), never the mirror.

**Presentation.** The sandbox status carries `siegeObjectives` straight from
`siege_objectives()`, and the battle HUD lists them in `#siege-objectives`
(locked, open, capturing with progress, or captured by a side) when a siege has
more than one objective. Presentation never reads the capture rate, contention
or unlock rules.

**Offering.** `StagedKeepV1` is not in `SiegeProfile::ALL` and no campaign
fortification level maps to it (`for_fortification_level` still returns only
palisade or stone walls), so sandbox and campaign offerings are unchanged; it is
reachable through `deploy_siege_with_profile` and saved battles. Mapping a
campaign fortification level to it, a browser profile selector with a
Playwright journey, and opponent-AI objective planning are follow-ups.

**Decisions on the points #127 left open.**

- *Geometry and count:* three objectives (outer bailey, inner bailey, keep)
  at 62 %, 76 % and 90 % of the width on the center line, behind the
  `StoneWallsV1` wall; every other siege rule (gate, towers, ladders, ram,
  catapult) is the stone-wall one.
- *Defender captures:* only attacker captures unlock; a defender capture of an
  earlier objective is permanent and keeps every later objective locked, and
  it never finishes the battle.
- *Legacy fields:* `capturePoint` is the keep, `capture` mirrors the active
  objective; neither decides victory for a staged profile.
- *Opponent AI:* unchanged; it does not plan around objectives yet.
- *Campaign mapping:* none; fortification levels still map to palisade and
  stone walls only.
- *`SiegeProfile::ALL`:* `StagedKeepV1` is not in it.
- *Browser selector and Playwright journey:* not added; the HUD list appears
  only for a siege with more than one objective, so current journeys are
  unchanged.

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

### Battlefield surface geometry

`TerrainMesh` prepares four-by-four top patches per authoritative terrain cell.
Every patch retains the core cell's exact elevation and bounds. Only exposed
cliffs are generated, with recessed middle strata underneath the logical tops;
shared cliff corners remain sealed. Location/cover palettes and soil shading
provide surface variation. Camera rays, orders, soldier elevation, cover, and
passability continue to use `TacticalTerrain` rather than the presentation mesh.

The GPU caches terrain vertices by battlefield dimensions and the complete
terrain value. Terrain changes upload through the queue into an unmapped vertex
buffer, reusing its capacity where possible. This avoids browser limits on
mapped-at-creation buffers during location changes. Unit movement and selection
updates do not prepare or upload terrain again.
The fixed geometry budget is 16,384 vertices / 768 KiB per battlefield, independent
of soldier count. Representative fixtures cover Mountain Pass, Forest Clearing,
and River Ford, including non-divisible dimensions and logical top agreement.

Run `cargo run -p medieval-renderer --example terrain_fixtures --release` to record
vertex counts, bytes, and mean preparation time across 1,000 preparations per
location. `npm run test:e2e:controls` includes screenshots, the geometry budget,
and physical picking/order checks for all three locations alongside tactical
control acceptance.

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
