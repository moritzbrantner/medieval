use medieval_core::{BattlePoint, FlatBattlefield};

pub(crate) const TERRAIN_GRID_SIZE: u32 = 8;
const TERRAIN_MAX_HEIGHT_DIVISOR: u32 = 25;

/// Renderer-owned elevation surface for the first terrain slice.
///
/// The tactical simulation still reasons in `BattlePoint` ground coordinates;
/// this module only defines deterministic world-space elevation used by the
/// shared Rust renderer, camera picking, and interaction anchors. Terrain that
/// changes movement/combat rules must become `medieval-core` truth before those
/// mechanics depend on it.
#[must_use]
pub(crate) fn terrain_height_mm(battlefield: FlatBattlefield, point: BattlePoint) -> u32 {
    if battlefield.width_mm == 0 || battlefield.depth_mm == 0 {
        return 0;
    }
    terrain_cell_height_mm(
        battlefield,
        terrain_cell_index(point.x_mm, battlefield.width_mm),
        terrain_cell_index(point.y_mm, battlefield.depth_mm),
    )
}

#[must_use]
pub(crate) fn terrain_cell_height_mm(
    battlefield: FlatBattlefield,
    cell_x: u32,
    cell_z: u32,
) -> u32 {
    let cell_x = cell_x.min(TERRAIN_GRID_SIZE - 1);
    let cell_z = cell_z.min(TERRAIN_GRID_SIZE - 1);
    let sample_x = cell_x * 2 + 1;
    let sample_z = cell_z * 2 + 1;
    let center = TERRAIN_GRID_SIZE;
    let distance = sample_x.abs_diff(center).max(sample_z.abs_diff(center));
    let max_distance = TERRAIN_GRID_SIZE - 1;
    let elevation_scale = max_distance.saturating_sub(distance);
    let peak_scale = TERRAIN_GRID_SIZE.saturating_sub(2).max(1);
    let max_height = battlefield.width_mm.min(battlefield.depth_mm) / TERRAIN_MAX_HEIGHT_DIVISOR;
    max_height.saturating_mul(elevation_scale) / peak_scale
}

#[must_use]
pub(crate) fn terrain_cell_bounds_mm(
    battlefield: FlatBattlefield,
    cell_x: u32,
    cell_z: u32,
) -> Option<(u32, u32, u32, u32)> {
    if battlefield.width_mm == 0
        || battlefield.depth_mm == 0
        || cell_x >= TERRAIN_GRID_SIZE
        || cell_z >= TERRAIN_GRID_SIZE
    {
        return None;
    }
    let x0 = scaled_boundary(battlefield.width_mm, cell_x);
    let x1 = scaled_boundary(battlefield.width_mm, cell_x + 1);
    let z0 = scaled_boundary(battlefield.depth_mm, cell_z);
    let z1 = scaled_boundary(battlefield.depth_mm, cell_z + 1);
    Some((x0, x1, z0, z1))
}

fn terrain_cell_index(coordinate_mm: u32, span_mm: u32) -> u32 {
    if span_mm == 0 {
        return 0;
    }
    ((u64::from(coordinate_mm.min(span_mm)) * u64::from(TERRAIN_GRID_SIZE))
        / u64::from(span_mm))
    .min(u64::from(TERRAIN_GRID_SIZE - 1)) as u32
}

fn scaled_boundary(span_mm: u32, index: u32) -> u32 {
    ((u64::from(span_mm) * u64::from(index)) / u64::from(TERRAIN_GRID_SIZE)) as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terrain_is_flat_at_the_outer_edge_and_elevated_in_the_center() {
        let battlefield = FlatBattlefield::new(100_000, 80_000);
        assert_eq!(terrain_height_mm(battlefield, BattlePoint::new(0, 0)), 0);
        assert!(terrain_height_mm(battlefield, BattlePoint::new(50_000, 40_000)) > 0);
    }

    #[test]
    fn terrain_height_is_cell_deterministic() {
        let battlefield = FlatBattlefield::new(100_000, 80_000);
        let left = terrain_height_mm(battlefield, BattlePoint::new(50_000, 40_000));
        let right = terrain_height_mm(battlefield, BattlePoint::new(50_001, 40_001));
        assert_eq!(left, right);
    }

    #[test]
    fn cell_bounds_cover_the_exact_battlefield_extent() {
        let battlefield = FlatBattlefield::new(100_003, 80_005);
        assert_eq!(
            terrain_cell_bounds_mm(battlefield, 0, 0),
            Some((0, 12_500, 0, 10_000))
        );
        assert_eq!(
            terrain_cell_bounds_mm(battlefield, 7, 7),
            Some((87_502, 100_003, 70_004, 80_005))
        );
    }
}