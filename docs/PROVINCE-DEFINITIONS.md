# Province definitions

`crates/medieval-core/data/provinces-v2.json` defines the starting province map.
The core parses and validates this packaged document once; `new_campaign()` uses
it to build runtime provinces. `ProvinceDefinitions::from_json` accepts custom
documents, and `new_campaign_with_province_definitions` constructs the current
campaign with those definitions. Starting factions and armies retain their
existing configuration.

Each province has a stable machine `id`, a separate display `name`, a
`startingOwner` faction ID, reciprocal `neighbors` containing province IDs, and a
`battlefieldProfile` ID. IDs may contain ASCII letters, digits, `-`, `_`, `:`,
`.`, and `/`. Movement, army positions, ownership, and adjacency reference IDs;
renaming a display name does not change those references.

From schema version 2, each `settlement.level` is required and is one of
`village`, `town`, `city`, or `majorCity`; it becomes the province's starting settlement level (see
[Settlement levels](#settlement-levels)).

Named `battlefieldProfiles` carry the core's location and fortification context.
Locations are `mountainPass`, `forestClearing`, or `riverFord`. Each province's
`settlement.fortified` must agree with its profile's `context.fortified`.
`baseEconomy.wealth` supplies the initial runtime wealth. Province, settlement,
and economy definitions accept optional `extensions` objects with stable keys
and arbitrary JSON values. Those values remain available in the read-only
definition document; they have no gameplay effect until a consumer implements
that extension.

Validation rejects duplicate IDs, empty display names, dangling or repeated
neighbors, self-adjacency, nonreciprocal borders, disconnected maps, unknown battlefield profiles,
invalid contexts, inconsistent fortifications, and an economy beyond the
campaign's income range. Campaign construction additionally validates starting
owners against factions and starting army positions against owned provinces.
Unknown fields are rejected outside extension objects.

## Settlement levels

Settlement level is core-owned runtime province state (`settlementLevel`).
`medieval_core::SettlementLevel::spec()` is the single source of each level's
effects and upgrade requirements; the UI only displays the
`SettlementUpgradeOption` the core returns, including the reason an upgrade is
unavailable.

| Level | Income bonus per faction turn | Building slots | Upgrade cost | Upgrade time | Minimum wealth |
| --- | --- | --- | --- | --- | --- |
| Village | 0 | 1 | — | — | — |
| Town | +50 gold | 2 | 400 gold | 1 round | 4 |
| City | +100 gold | 3 | 800 gold | 2 rounds | 6 |
| Major city | +200 gold | 4 | 1,600 gold | 3 rounds | 8 |

The income bonus is added to `wealth × 50` provincial income. Building slots
are the capacity [buildings](#buildings-and-construction) consume.

Only the active faction may upgrade a province it controls, one level at a
time, with at most one upgrade queued per province, and never while a battle is
pending. Queueing pays the cost immediately and schedules completion for the
start of the owner's turn the given number of rounds later (`readyOnTurn`).
Completion happens in the turn-start step guarded by `lastEconomyTurn`, before
that turn's income is paid, and removes the order, so it is applied exactly
once. The deterministic AI queues at most one
upgrade per turn through the same commands, in its wealthiest eligible
province, while keeping 500 gold in reserve. A captured province keeps its level; the former owner's queued upgrade is
cancelled without refund.

The packaged map starts Normandy as a town, Paris as a city, and every other
province as a village.

## Buildings and construction

Buildings are core-owned runtime province state (`buildings`, one entry per
standing building with its level, ordered by building id).
`medieval_core::BuildingId::spec()` is the single source of each building's
category, levels, costs, durations, prerequisites, and effects; the UI only
displays the `ProvinceConstruction` view the core returns
(`construction_options`), including the reason each choice is unavailable, and
sends `queue_construction` intents.

| Building | Category | Level 1 | Level 2 |
| --- | --- | --- | --- |
| Farms | Economy | Farmland: 250 gold, 1 round, village, +40 income | Irrigated fields: 600 gold, 2 rounds, town, Reeve's hall, +100 income |
| Town hall | Administration | Reeve's hall: 300 gold, 1 round, town | Guildhall: 800 gold, 2 rounds, city |
| Barracks | Military | Muster field: 300 gold, 1 round, village | Barracks: 700 gold, 2 rounds, town, Reeve's hall |
| Walls | Defense | Palisade: 400 gold, 2 rounds, town | Stone walls: 1,000 gold, 3 rounds, city, Reeve's hall |

Each level names a minimum current settlement level and any other building
levels that must already stand. A level's income bonus replaces the lower
level's. Barracks unlock units and deepen recruitment pools (see below); walls
select the siege profile of battles for the province (see below). Every
distinct building uses one of the settlement's building slots; raising a
standing building to its next level needs no new slot.

Only the active faction may build in a province it controls, one level at a
time, with at most one construction order per province (independent of a
settlement upgrade), and never while a battle is pending. Queueing pays the cost
immediately and schedules completion for the start of the owner's turn the
given number of rounds later (`readyOnTurn`). Completion runs in the
`lastEconomyTurn`-guarded turn-start step after settlement upgrades and before
income, and removes the order, so it is applied exactly once and the completing
turn already pays the new income. The deterministic AI queues at most one
construction per turn through the same commands, in its wealthiest province with
a legal choice, preferring farms, then town hall, walls, and barracks, while
keeping 500 gold in reserve. A captured province keeps its buildings; the
former owner's queued construction is cancelled without refund. New campaigns
start without buildings.

## Unit unlocks and recruitment pools

Recruitment is local: `medieval_core::UnitKind::unlock()` names each unit's
single unlock source, and every province carries a `recruitmentPool` of
recruitable batches per unit. The UI shows the `RecruitmentOption` list the core
returns (`recruitment_options`) — unlock source, pool and capacity, and the
reason a unit is unavailable — and sends `queue_recruitment` intents.

| Unit | Unlock source | Pool capacity |
| --- | --- | --- |
| Levy | any settlement | 1 + settlement levels above village + barracks level |
| Spearmen | Muster field (Barracks 1) | 1 + settlement levels above village + barracks levels above 1 |
| Archers | town | 1 + settlement levels above town + barracks level |
| Knights | town with Barracks (Barracks 2) | 1 + settlement levels above town |

A locked unit has capacity zero. Queueing a batch consumes one from the pool
(several batches of one unit may be queued while the pool lasts). In the
`lastEconomyTurn`-guarded turn-start step, after settlement upgrades and
construction complete and before income, every province of the active faction
regains one batch per unlocked unit, up to capacity, so a newly completed
barracks offers its first batch on the turn it completes. New campaigns start
with full pools. A captured province keeps its pool; the former owner's queued
recruitment there is cancelled without refunding the batch. The AI recruits
through the same options and commands. Numbers are a first pass for the V1
balance pass (#174).

## Fortification and siege profiles

A province's fortification level is its standing walls level, or 2 for a
settlement defined as `fortified` (stone walls), whichever is higher. Level 0
is fought as a field battle; higher levels are sieges with a versioned,
core-owned `medieval_core::SiegeProfile` that alone determines wall, gate,
tower, and capture geometry (renderers only draw the resulting layout):

| Level | Profile | Wall band (width) | Gate span (depth) | Tower radius | Capture point |
| --- | --- | --- | --- | --- | --- |
| 1 (Palisade) | `palisadeV1` | 49.5–50.5 % | 43–57 % | shorter side / 80 | 80 %, 50 %; shorter side / 20 |
| 2 (Stone walls or defined fortified) | `stoneWallsV1` | 49–51 % | 45–55 % | shorter side / 50 | 80 %, 50 %; shorter side / 20 |

Both profiles start with a closed gate and four towers at 15, 35, 65 and 85 %
of the depth. `stoneWallsV1` is the original prototype layout. Campaign seeds
record the profile as `battlefieldProfile.fortification` and battles retain it
in their siege state; seeds and battles recorded before profiles existed load
as `stoneWallsV1`, which is exactly the geometry they were fought on. The
sandbox keeps choosing explicit fixtures (`deploy_siege_at_location` is the
stone-walls fixture; `deploy_siege_with_profile` selects any profile).

## Versions and saved campaigns

The definition document has `schemaVersion: 2`. Version 1 documents remain supported: they must not declare `settlement.level`, every settlement starts as a village, and the loaded document is migrated to version 2. Unsupported versions are
reported before decoding the current document shape. This configuration version
is independent of the campaign save schema. Save schema version 3 adds
`settlementLevel` to provinces and the `settlementUpgrades` queue; schema 1 and
2 saves still load, with every province as a village; they must not contain
settlement levels or upgrades, and schema 3 saves must give every province a
level. Save validation also rejects upgrade orders whose `readyOnTurn` is not
on the owner's turn or lies beyond the target level's build duration. Save
schema version 4 adds `buildings` to provinces (required) and the
`constructionQueue`; older saves load without buildings and must not contain
either. Validation rejects duplicate or out-of-range buildings, buildings
whose settlement or building prerequisites are missing, more buildings than
slots, and construction orders that are foreign, skip a level, share a
province, or do not complete exactly on the owner's turn. Save schema version
5 adds the required `recruitmentPool` to provinces; older saves must not
contain it and load with every unlocked pool full. Validation rejects pools
above their current capacity.

Saved campaigns contain their own province state. Loading a save does not
replace names, borders, wealth, ownership, settlement level, or battlefield context with current
starting definitions. Historical saves missing battlefield metadata retain the
existing default context. Extension configuration is not stored in campaign
saves because it has no runtime state yet.

The regression fixture `tests/fixtures/six-provinces-save-v2.json` was captured
from the previous bootstrap before extraction. Tests compare the entire new
campaign against it (with the packaged initial settlement levels applied), exercise legacy migration, and demonstrate expansion to a
seventh province through data alone.
