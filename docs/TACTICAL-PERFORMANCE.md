# Tactical workload evidence

Run the bounded matrix with the repository-pinned toolchain:

```sh
cargo run --locked -p medieval-core --example tactical_work_matrix --release
```

Each JSON line identifies schema version 1, fixture revision 1, the scenario,
unit count, requested ticks, final state, survivors, deterministic work, and the
median duration of three identical runs. The example checks repeated state and
counter equality before reporting. Timing includes construction and engagement
orders; counters cover tick advancement only. Timing is advisory on shared
runners and is never a correctness threshold.

| Scenario | Units | Initial soldiers | Requested ticks | Work shape |
| --- | ---: | ---: | ---: | --- |
| Small | 2 | 320 | 600 | Unequal melee ending in attacker victory |
| Medium | 12 | 1,440 | 600 | Moving paired engagements |
| Large | 64 | 7,680 | 600 | Same engagement policy at greater scale |
| Dense melee | 32 | 3,840 | 120 | All pairs begin at contact |
| Ranged heavy | 24 | 2,880 | 120 | Finite-ammunition archers begin at range |
| Terrain heavy | 16 | 1,920 | 600 | Mixed cavalry/infantry seek river crossings |
| Siege | 8 | 960 | 600 | Open gate, walls, and capture state |
| Idle | 32 | 3,840 | 40 | No movement or engagement orders |

Fixtures live in `crates/medieval-core/tests/support/tactical_scenarios.rs` and
are shared by the example and integration tests. Advance the fixture revision
when changing positions, orders, profiles, scale, terrain, or tick counts.
The unit profiles use explicit version-one core combat rules.

## Counters and cost model

`TacticalBattle::advance_ticks_measured` applies the same rules as
`advance_ticks` and returns `TacticalWorkCounters`. These counters are local to
that call: they do not change battle equality, saves, snapshots, or game rules.
Completed battles report zero further tick work. Callers can sum individual
fields across chunks; the integration suite verifies the same totals when
advancing one tick at a time.

- `ticks` and `movementUnitVisits` count executed ticks and the movement phase's
  unit visits, including units that require no movement.
- `snapshotClones` counts whole unit-vector clones during tick advancement and
  must stay zero. `snapshotUnitCopies` counts individual units copied by the
  movement phase's copy-on-write before-state: a unit is copied right before
  its own step only when that step may change it (it has a destination or
  engagement target, has fatigue to recover, or is routed/withdrawing). Combat
  pulses read live units because nothing changes until casualties, fatigue and
  spent ammunition are applied after the volley and melee loops. Neither
  counter measures bytes or allocations inside copied units.
- `targetCandidateVisits` counts visits in the remaining linear candidate scans:
  attack-move acquisition, routed nearest-enemy search, and cavalry charge
  interception. `indexedUnitLookups` counts id lookups, which binary-search the
  id-ordered unit vector. Unrelated validation scans are outside both metrics.
- `proximityQueries` counts tactical distance requests; `physicsContactQueries`
  counts requests that pass the adapter's axis rejection and call the shared
  physics kernel. Physics internals remain upstream-owned.
- `pathRequests` counts terrain/siege waypoint requests; `terrainSpeedQueries`
  counts movement-speed sampling. These are requests, not cells searched.
- `combatPulses`, `engagementPairs`, `rangedVolleys`, `meleeContacts`, and
  `pursuitContacts` distinguish pulse processing, unique engagement pairs, and
  actual combat work. A melee contact is one pair, rather than two damage calls.

Movement visits grow linearly with units and ticks; that per-tick visit scan is
the declared linear cost of a tick. Copies follow the units that move, fight, or
recover. Id lookups are logarithmic; the remaining candidate scans are linear
per scanning unit and grow quadratically only when many units scan at once.
Path request count follows moving units and charge checks; each request may do
additional terrain/siege work. Combat scans follow pulse count and engaged
units, while exact physics calls depend on proximity. These metrics expose
composition growth without duplicating upstream microbenchmarks.

## Operation scaling contracts

`tests/tactical_operation_scaling.rs` keeps one active set fixed — a melee pair
in contact and one marching unit — and adds 8, 64, and 256 idle unrelated
units. For movement ticks, the combat-pulse tick, and the tick after one
retarget, every counter except `movementUnitVisits` must be identical across
populations, no whole unit vector is cloned, only active units are copied, and
the active units reach identical state. A regression back to whole-battle
clones or linear id lookups fails these assertions.

Single-unit move, facing, engagement and formation orders find units by binary
search and do not copy the battle. Multi-unit formation and group orders still
apply to a cloned candidate battle so a rejected order leaves the battle
unchanged; that copy is a deliberate transactional cost of one command, not of
recurring ticks. A full `BattleRenderSnapshot` remains allowed to scale with
its output.

## Regression gates and reproducibility

`tests/fixtures/tactical-work-budgets-v1.json` records the baseline core revision,
fixture revision, and per-scenario subsystem limits. Nonzero limits allow ten
percent above the recorded work, rounded up; zero-work metrics remain zero.
No elapsed time appears in that artifact. The snapshot and lookup limits were
re-recorded after #200 replaced vector snapshots and linear id lookups; the
other limits keep the baseline revision. Review a deliberate budget change
alongside its fixture or algorithm change; do not regenerate limits merely to
make a failing check pass.

Separate tests validate terminal outcome, bounded soldiers/morale/fatigue,
passable positions, finite ammunition, siege state, idle preservation,
serialization, deterministic replay, and chunked advancement. Counter budgets
provide work-growth evidence rather than replacing those behavioral checks.

For recorded timing comparisons, retain the command output with the exact git
revision, toolchain, target, build profile, features, and environment. Compare
equivalent fixtures and treat different fingerprints as incomparable. The
existing `performance_smoke` example remains a quick legacy engagement sample;
its workload differs from this matrix and its timing is not a matrix baseline.
