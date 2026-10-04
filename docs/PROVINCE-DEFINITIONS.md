# Province definitions

`crates/medieval-core/data/provinces-v1.json` defines the starting province map.
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

## Versions and saved campaigns

The definition document has `schemaVersion: 1`. Unsupported versions are
reported before decoding the current document shape. This configuration version
is independent of campaign save schema version 2, whose runtime province shape
is unchanged by this extraction. Existing schema 1 saves still migrate to 2.

Saved campaigns contain their own province state. Loading a save does not
replace names, borders, wealth, ownership, or battlefield context with current
starting definitions. Historical saves missing battlefield metadata retain the
existing default context. Extension configuration is not stored in campaign
saves because it has no runtime state yet.

The regression fixture `tests/fixtures/six-provinces-save-v2.json` was captured
from the previous bootstrap before extraction. Tests compare the entire new
campaign against it, exercise legacy migration, and demonstrate expansion to a
seventh province through data alone.
