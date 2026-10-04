use bytemuck::{Pod, Zeroable};
use medieval_core::{BattlefieldLocation, FlatBattlefield, TacticalGroundCover, TacticalTerrain};

use crate::terrain::{TERRAIN_GRID_SIZE, terrain_cell_bounds_mm};

const SUBDIVISIONS: u32 = 4;
const BASE_HEIGHT_MM: f32 = -200.0;

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable)]
pub struct TerrainVertex {
    pub position: [f32; 4],
    pub normal: [f32; 4],
    pub color: [f32; 4],
}

/// Presentation geometry follows the exact logical cell tops. Only exposed
/// cliff faces are emitted; their recessed strata stay beneath those tops.
#[derive(Clone, Debug, PartialEq)]
pub struct TerrainMesh {
    pub vertices: Vec<TerrainVertex>,
}

impl TerrainMesh {
    #[must_use]
    pub fn prepare(terrain: TacticalTerrain, field: FlatBattlefield) -> Self {
        let mut mesh = Self {
            vertices: Vec::new(),
        };
        for z in 0..TERRAIN_GRID_SIZE {
            for x in 0..TERRAIN_GRID_SIZE {
                let Some((x0, x1, z0, z1)) = terrain_cell_bounds_mm(field, x, z) else {
                    continue;
                };
                if x0 == x1 || z0 == z1 {
                    continue;
                }
                let height = terrain.cell_height_mm(field, x, z) as f32;
                let cover = terrain.cell_ground_cover(x, z);
                let color = ground_color(terrain.location(), cover, height);
                for row in 0..SUBDIVISIONS {
                    for column in 0..SUBDIVISIONS {
                        let left = partition(x0, x1, column);
                        let right = partition(x0, x1, column + 1);
                        let near = partition(z0, z1, row);
                        let far = partition(z0, z1, row + 1);
                        if left == right || near == far {
                            continue;
                        }
                        mesh.quad(
                            [
                                [left as f32, height, near as f32],
                                [left as f32, height, far as f32],
                                [right as f32, height, far as f32],
                                [right as f32, height, near as f32],
                            ],
                            color,
                        );
                    }
                }
                let edges = [
                    (
                        [x0 as f32, z0 as f32],
                        [x1 as f32, z0 as f32],
                        [0.0, 1.0],
                        (Some(x), z.checked_sub(1)),
                    ),
                    (
                        [x1 as f32, z0 as f32],
                        [x1 as f32, z1 as f32],
                        [-1.0, 0.0],
                        (x.checked_add(1), Some(z)),
                    ),
                    (
                        [x1 as f32, z1 as f32],
                        [x0 as f32, z1 as f32],
                        [0.0, -1.0],
                        (Some(x), z.checked_add(1)),
                    ),
                    (
                        [x0 as f32, z1 as f32],
                        [x0 as f32, z0 as f32],
                        [1.0, 0.0],
                        (x.checked_sub(1), Some(z)),
                    ),
                ];
                for (start, end, inward, (nx, nz)) in edges {
                    let bottom = match (nx, nz) {
                        (Some(nx), Some(nz))
                            if nx < TERRAIN_GRID_SIZE
                                && nz < TERRAIN_GRID_SIZE
                                && terrain_cell_bounds_mm(field, nx, nz)
                                    .is_some_and(|(x0, x1, z0, z1)| x1 > x0 && z1 > z0) =>
                        {
                            terrain.cell_height_mm(field, nx, nz) as f32
                        }
                        _ => BASE_HEIGHT_MM,
                    };
                    if bottom >= height {
                        continue;
                    }
                    mesh.cliff(start, end, inward, height, bottom, color);
                }
            }
        }
        mesh
    }

    fn cliff(
        &mut self,
        start: [f32; 2],
        end: [f32; 2],
        inward: [f32; 2],
        top: f32,
        bottom: f32,
        color: [f32; 3],
    ) {
        let length = ((end[0] - start[0]).powi(2) + (end[1] - start[1]).powi(2)).sqrt();
        let inset = 250.0_f32.min(length * 0.02).min((top - bottom) * 0.15);
        let middle = (top + bottom) * 0.5;
        for segment in 0..SUBDIVISIONS {
            let point = |index: u32| {
                let factor = index as f32 / SUBDIVISIONS as f32;
                [
                    start[0] + (end[0] - start[0]) * factor,
                    start[1] + (end[1] - start[1]) * factor,
                ]
            };
            let a = point(segment);
            let b = point(segment + 1);
            let recess = |point: [f32; 2], index: u32| {
                let variation = if index == 0 || index == SUBDIVISIONS {
                    0.0
                } else if index.is_multiple_of(2) {
                    0.65
                } else {
                    1.0
                };
                [
                    point[0] + inward[0] * inset * variation,
                    middle,
                    point[1] + inward[1] * inset * variation,
                ]
            };
            let mid_a = recess(a, segment);
            let mid_b = recess(b, segment + 1);
            let rock = |factor: f32| {
                [
                    color[0] * factor + 0.08,
                    color[1] * factor + 0.06,
                    color[2] * factor + 0.04,
                ]
            };
            self.quad(
                [[a[0], top, a[1]], [b[0], top, b[1]], mid_b, mid_a],
                rock(0.65),
            );
            self.quad(
                [mid_a, mid_b, [b[0], bottom, b[1]], [a[0], bottom, a[1]]],
                rock(0.48),
            );
        }
    }

    fn quad(&mut self, points: [[f32; 3]; 4], color: [f32; 3]) {
        for triangle in [[0, 1, 2], [0, 2, 3]] {
            let a = points[triangle[0]];
            let b = points[triangle[1]];
            let c = points[triangle[2]];
            let u = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
            let v = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
            let cross = [
                u[1] * v[2] - u[2] * v[1],
                u[2] * v[0] - u[0] * v[2],
                u[0] * v[1] - u[1] * v[0],
            ];
            let length = (cross[0].powi(2) + cross[1].powi(2) + cross[2].powi(2)).sqrt();
            if length == 0.0 {
                continue;
            }
            let normal = [cross[0] / length, cross[1] / length, cross[2] / length, 0.0];
            for index in triangle {
                let p = points[index];
                self.vertices.push(TerrainVertex {
                    position: [p[0], p[1], p[2], 1.0],
                    normal,
                    color: [color[0], color[1], color[2], 1.0],
                });
            }
        }
    }
}

fn partition(start: u32, end: u32, index: u32) -> u32 {
    start
        + u32::try_from(u64::from(end - start) * u64::from(index) / u64::from(SUBDIVISIONS))
            .expect("partition remains inside cell")
}
fn ground_color(
    location: BattlefieldLocation,
    cover: TacticalGroundCover,
    height: f32,
) -> [f32; 3] {
    let base = match location {
        BattlefieldLocation::MountainPass => [0.28, 0.34, 0.16],
        BattlefieldLocation::ForestClearing => [0.24, 0.36, 0.14],
        BattlefieldLocation::RiverFord => [0.27, 0.37, 0.19],
    };
    let factor = (height / 8_000.0).clamp(0.0, 1.0);
    let cover = if cover == TacticalGroundCover::Forest {
        0.78
    } else {
        1.0
    };
    [
        ((base[0] * (1.0 - factor) + 0.44 * factor) * cover),
        ((base[1] * (1.0 - factor) + 0.43 * factor) * cover),
        ((base[2] * (1.0 - factor) + 0.30 * factor) * cover),
    ]
}
