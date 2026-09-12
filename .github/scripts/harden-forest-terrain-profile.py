from pathlib import Path


def replace_once(path: str, old: str, new: str) -> None:
    file = Path(path)
    text = file.read_text()
    if old not in text:
        raise SystemExit(f"missing expected block in {path}: {old[:160]!r}")
    file.write_text(text.replace(old, new, 1))


replace_once(
    "crates/medieval-core/src/terrain.rs",
    '''pub enum TacticalTerrainProfile {
    #[default]
    HeightFoundationV1,
}''',
    '''pub enum TacticalTerrainProfile {
    #[default]
    HeightFoundationV1,
    ForestMovementV2,
}''',
)

replace_once(
    "crates/medieval-core/src/terrain.rs",
    '''    pub const fn battlefield_foundation() -> Self {
        Self {
            profile: TacticalTerrainProfile::HeightFoundationV1,
        }
    }

    #[must_use]
    pub const fn height_foundation() -> Self {
        Self::battlefield_foundation()
    }''',
    '''    pub const fn battlefield_foundation() -> Self {
        Self {
            profile: TacticalTerrainProfile::ForestMovementV2,
        }
    }

    #[must_use]
    pub const fn height_foundation() -> Self {
        Self {
            profile: TacticalTerrainProfile::HeightFoundationV1,
        }
    }''',
)

replace_once(
    "crates/medieval-core/src/terrain.rs",
    '''    pub const fn forest_cells(self) -> [TacticalTerrainCell; TACTICAL_FOREST_CELL_COUNT] {
        let _profile = self.profile;
        TACTICAL_FOREST_CELLS
    }''',
    '''    pub const fn forest_cells(self) -> &'static [TacticalTerrainCell] {
        match self.profile {
            TacticalTerrainProfile::HeightFoundationV1 => &[],
            TacticalTerrainProfile::ForestMovementV2 => &TACTICAL_FOREST_CELLS,
        }
    }''',
)

replace_once(
    "crates/medieval-core/src/terrain.rs",
    '''    pub fn cell_ground_cover(self, cell_x: u32, cell_z: u32) -> TacticalGroundCover {
        let _profile = self.profile;
        if TACTICAL_FOREST_CELLS.contains(&TacticalTerrainCell { cell_x, cell_z }) {
            TacticalGroundCover::Forest
        } else {
            TacticalGroundCover::Open
        }
    }''',
    '''    pub fn cell_ground_cover(self, cell_x: u32, cell_z: u32) -> TacticalGroundCover {
        if self
            .forest_cells()
            .contains(&TacticalTerrainCell { cell_x, cell_z })
        {
            TacticalGroundCover::Forest
        } else {
            TacticalGroundCover::Open
        }
    }''',
)

replace_once(
    "crates/medieval-core/src/terrain.rs",
    '''        assert_eq!(decoded, terrain);
        assert_eq!(decoded.height_mm(battlefield, point), first);
        assert_eq!(decoded.forest_cells(), terrain.forest_cells());
    }

    #[test]
    fn height_lookup_uses_the_same_floored_boundaries_as_cell_geometry() {''',
    '''        assert_eq!(decoded, terrain);
        assert_eq!(decoded.height_mm(battlefield, point), first);
        assert_eq!(decoded.forest_cells(), terrain.forest_cells());
        assert_eq!(decoded.profile(), TacticalTerrainProfile::ForestMovementV2);
    }

    #[test]
    fn height_foundation_v1_remains_height_only_for_legacy_replays() {
        let terrain = TacticalTerrain::height_foundation();
        let battlefield = FlatBattlefield::new(80_000, 80_000);
        let former_forest_point = BattlePoint::new(25_000, 15_000);
        assert_eq!(terrain.profile(), TacticalTerrainProfile::HeightFoundationV1);
        assert!(terrain.forest_cells().is_empty());
        assert_eq!(
            terrain.ground_cover_at(battlefield, former_forest_point),
            TacticalGroundCover::Open
        );
        assert_eq!(
            terrain.movement_speed_mm_per_tick(battlefield, former_forest_point, 1_201),
            1_201
        );
    }

    #[test]
    fn height_lookup_uses_the_same_floored_boundaries_as_cell_geometry() {''',
)

replace_once(
    "crates/medieval-core/src/tactical.rs",
    '''pub struct TacticalBattle {
    tick: u64,
    battlefield: FlatBattlefield,
    units: Vec<TacticalUnit>,
}''',
    '''pub struct TacticalBattle {
    tick: u64,
    battlefield: FlatBattlefield,
    terrain: TacticalTerrain,
    units: Vec<TacticalUnit>,
}''',
)

replace_once(
    "crates/medieval-core/src/tactical.rs",
    '''struct TacticalBattleWire {
    tick: u64,
    battlefield: FlatBattlefield,
    units: Vec<TacticalUnit>,
}''',
    '''struct TacticalBattleWire {
    tick: u64,
    battlefield: FlatBattlefield,
    #[serde(default)]
    terrain: TacticalTerrain,
    units: Vec<TacticalUnit>,
}''',
)

replace_once(
    "crates/medieval-core/src/tactical.rs",
    '''        Self {
            tick: wire.tick,
            battlefield: wire.battlefield,
            units: wire.units,
        }''',
    '''        Self {
            tick: wire.tick,
            battlefield: wire.battlefield,
            terrain: wire.terrain,
            units: wire.units,
        }''',
)

replace_once(
    "crates/medieval-core/src/tactical.rs",
    '''        Ok(Self {
            tick: 0,
            battlefield,
            units,
        })''',
    '''        Ok(Self {
            tick: 0,
            battlefield,
            terrain: TacticalTerrain::battlefield_foundation(),
            units,
        })''',
)

replace_once(
    "crates/medieval-core/src/tactical.rs",
    '''    pub const fn terrain(&self) -> TacticalTerrain {
        TacticalTerrain::battlefield_foundation()
    }''',
    '''    pub const fn terrain(&self) -> TacticalTerrain {
        self.terrain
    }''',
)

replace_once(
    "crates/medieval-core/src/tactical.rs",
    '''    #[test]
    fn movement_never_exceeds_speed_and_arrives_exactly() {''',
    '''    #[test]
    fn legacy_battle_without_terrain_preserves_height_only_v1_semantics() {
        let battle = sample_battle();
        let mut encoded = serde_json::to_value(&battle).unwrap();
        encoded.as_object_mut().unwrap().remove("terrain");

        let decoded: TacticalBattle = serde_json::from_value(encoded).unwrap();
        assert_eq!(
            decoded.terrain().profile(),
            crate::terrain::TacticalTerrainProfile::HeightFoundationV1
        );
        assert!(decoded.terrain().forest_cells().is_empty());
    }

    #[test]
    fn movement_never_exceeds_speed_and_arrives_exactly() {''',
)

replace_once(
    "docs/BATTLE-ARCHITECTURE.md",
    '''Forest cover is the first tactical terrain modifier. The current profile owns six deterministic forest cells. A unit that starts a simulation tick in a forest cell receives half of its normal movement budget for that tick, rounded up; the same rule is applied to normal, pursuit, and routed movement. The renderer consumes those exact cells and draws primitive tree proxies, but those proxies do not own collision or movement rules. Forests currently do not modify combat, morale, line-of-sight, or deployment legality, and elevation itself remains gameplay-neutral.''',
    '''Forest cover is the first tactical terrain modifier. `ForestMovementV2` owns six deterministic forest cells. A unit that starts a simulation tick in a forest cell receives half of its normal movement budget for that tick, rounded up; the same rule is applied to normal, pursuit, and routed movement. `TacticalBattle` serializes the terrain profile so replay semantics stay explicit, while legacy battle documents that predate the field default to `HeightFoundationV1`, which remains height-only. The renderer consumes the exact forest cells and draws primitive tree proxies, but those proxies do not own collision or movement rules. Forests currently do not modify combat, morale, line-of-sight, or deployment legality, and elevation itself remains gameplay-neutral.''',
)
