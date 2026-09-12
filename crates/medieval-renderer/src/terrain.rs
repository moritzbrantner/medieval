use medieval_core::{BattlePoint, FlatBattlefield, TACTICAL_TERRAIN_GRID_SIZE, TacticalTerrain};

pub(crate) const TERRAIN_GRID_SIZE: u32 = TACTICAL_TERRAIN_GRID_SIZE;
#[cfg(test)]
pub(crate) const TERRAIN_BASE_DEPTH_MM: f32 = 200.0;

const fn terrain() -> TacticalTerrain {
    TacticalTerrain::battlefield_foundation()
}

#[must_use]
pub(crate) fn terrain_height_mm(battlefield: FlatBattlefield, point: BattlePoint) -> u32 {
    terrain().height_mm(battlefield, point)
}

#[must_use]
pub(crate) fn terrain_cell_height_mm(
    battlefield: FlatBattlefield,
    cell_x: u32,
    cell_z: u32,
) -> u32 {
    terrain().cell_height_mm(battlefield, cell_x, cell_z)
}

#[must_use]
pub(crate) fn terrain_cell_bounds_mm(
    battlefield: FlatBattlefield,
    cell_x: u32,
    cell_z: u32,
) -> Option<(u32, u32, u32, u32)> {
    terrain().cell_bounds_mm(battlefield, cell_x, cell_z)
}

#[cfg(test)]
#[must_use]
pub(crate) fn terrain_cell_world_bounds(
    battlefield: FlatBattlefield,
    cell_x: u32,
    cell_z: u32,
) -> Option<([f32; 3], [f32; 3])> {
    let (x0, x1, z0, z1) = terrain_cell_bounds_mm(battlefield, cell_x, cell_z)?;
    if x1 <= x0 || z1 <= z0 {
        return None;
    }
    Some((
        [x0 as f32, -TERRAIN_BASE_DEPTH_MM, z0 as f32],
        [
            x1 as f32,
            terrain_cell_height_mm(battlefield, cell_x, cell_z) as f32,
            z1 as f32,
        ],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renderer_adapter_matches_the_core_terrain_contract() {
        let battlefield = FlatBattlefield::new(100_003, 80_005);
        let point = BattlePoint::new(12_500, 40_002);
        let core = TacticalTerrain::height_foundation();
        assert_eq!(
            terrain_height_mm(battlefield, point),
            core.height_mm(battlefield, point)
        );
        assert_eq!(
            terrain_cell_bounds_mm(battlefield, 1, 4),
            core.cell_bounds_mm(battlefield, 1, 4)
        );
        assert_eq!(
            terrain_cell_height_mm(battlefield, 1, 4),
            core.cell_height_mm(battlefield, 1, 4)
        );
    }

    #[test]
    fn world_bounds_match_the_rendered_cell_volume() {
        let battlefield = FlatBattlefield::new(100_000, 80_000);
        let (minimum, maximum) = terrain_cell_world_bounds(battlefield, 4, 4).unwrap();
        assert_eq!(minimum, [50_000.0, -TERRAIN_BASE_DEPTH_MM, 40_000.0]);
        assert_eq!(maximum[0], 62_500.0);
        assert_eq!(maximum[2], 50_000.0);
        assert_eq!(maximum[1], terrain_cell_height_mm(battlefield, 4, 4) as f32);
    }
}
