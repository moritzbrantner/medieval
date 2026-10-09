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
are the capacity the construction system will consume; nothing consumes them
yet.

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

## Versions and saved campaigns

The definition document has `schemaVersion: 2`. Version 1 documents remain supported: they must not declare `settlement.level`, every settlement starts as a village, and the loaded document is migrated to version 2. Unsupported versions are
reported before decoding the current document shape. This configuration version
is independent of the campaign save schema. Save schema version 3 adds
`settlementLevel` to provinces and the `settlementUpgrades` queue; schema 1 and
2 saves still load, with every province as a village; they must not contain
settlement levels or upgrades, and schema 3 saves must give every province a
level. Save validation also rejects upgrade orders whose `readyOnTurn` is not
on the owner's turn or lies beyond the target level's build duration.

Saved campaigns contain their own province state. Loading a save does not
replace names, borders, wealth, ownership, settlement level, or battlefield context with current
starting definitions. Historical saves missing battlefield metadata retain the
existing default context. Extension configuration is not stored in campaign
saves because it has no runtime state yet.

The regression fixture `tests/fixtures/six-provinces-save-v2.json` was captured
from the previous bootstrap before extraction. Tests compare the entire new
campaign against it (with the packaged initial settlement levels applied), exercise legacy migration, and demonstrate expansion to a
seventh province through data alone.
