from pathlib import Path


def replace_once(text: str, old: str, new: str, label: str) -> str:
    count = text.count(old)
    if count != 1:
        raise RuntimeError(f"{label}: expected one anchor, found {count}")
    return text.replace(old, new)


path = Path("crates/medieval-renderer/src/camera.rs")
text = path.read_text()

text = replace_once(
    text,
    "use crate::terrain::terrain_height_mm;\n",
    "use crate::terrain::{\n"
    "    TERRAIN_GRID_SIZE, terrain_cell_bounds_mm, terrain_cell_height_mm, terrain_height_mm,\n"
    "};\n",
    "terrain imports",
)

text = replace_once(
    text,
    "const NEAR_PLANE_MM: f32 = 100.0;\n"
    "const ZERO_HEIGHT_ENDPOINT_EPSILON_MM: f32 = 1.0;\n"
    "const TERRAIN_RAY_MARCH_STEPS: u32 = 256;\n"
    "const TERRAIN_RAY_REFINEMENT_STEPS: u32 = 18;\n",
    "const NEAR_PLANE_MM: f32 = 100.0;\n"
    "const TERRAIN_PICK_EPSILON_MM: f32 = 1.0;\n",
    "terrain picking constants",
)

old_ground = '''        if ray.direction[1] >= -f32::EPSILON {
            return None;
        }
        let zero_plane_distance = -ray.origin_mm[1] / ray.direction[1];
        if !zero_plane_distance.is_finite() || zero_plane_distance < 0.0 {
            return None;
        }

        let mut previous_inside: Option<(f32, f32)> = None;
        for step in 0..=TERRAIN_RAY_MARCH_STEPS {
            let distance = zero_plane_distance * step as f32 / TERRAIN_RAY_MARCH_STEPS as f32;
            let Some((point, clearance)) = terrain_clearance(battlefield, ray, distance) else {
                continue;
            };
            if clearance <= 0.0 {
                if let Some((previous_distance, previous_clearance)) = previous_inside
                    && previous_clearance > 0.0
                {
                    let hit_distance =
                        refine_terrain_hit(battlefield, ray, previous_distance, distance);
                    return terrain_point_at_distance(battlefield, ray, hit_distance);
                }
                return Some(point);
            }
            if step == TERRAIN_RAY_MARCH_STEPS
                && terrain_height_mm(battlefield, point) == 0
                && clearance <= ZERO_HEIGHT_ENDPOINT_EPSILON_MM
            {
                return Some(point);
            }
            previous_inside = Some((distance, clearance));
        }
        None
'''
new_ground = '''        let hit = terrain_ray_hit(battlefield, ray)?;
        terrain_point_from_hit(battlefield, ray, hit)
'''
text = replace_once(text, old_ground, new_ground, "terrain ray march")

old_helpers = '''fn terrain_clearance(
    battlefield: FlatBattlefield,
    ray: ViewportRay,
    distance: f32,
) -> Option<(BattlePoint, f32)> {
    let hit = add(ray.origin_mm, scale(ray.direction, distance));
    let point = terrain_point_from_world(battlefield, hit[0], hit[2])?;
    let surface_y = terrain_height_mm(battlefield, point) as f32;
    Some((point, hit[1] - surface_y))
}

fn refine_terrain_hit(
    battlefield: FlatBattlefield,
    ray: ViewportRay,
    mut low: f32,
    mut high: f32,
) -> f32 {
    for _ in 0..TERRAIN_RAY_REFINEMENT_STEPS {
        let middle = (low + high) / 2.0;
        match terrain_clearance(battlefield, ray, middle) {
            Some((_, clearance)) if clearance > 0.0 => low = middle,
            Some(_) => high = middle,
            None => low = middle,
        }
    }
    high
}

fn terrain_point_at_distance(
    battlefield: FlatBattlefield,
    ray: ViewportRay,
    distance: f32,
) -> Option<BattlePoint> {
    let hit = add(ray.origin_mm, scale(ray.direction, distance));
    terrain_point_from_world(battlefield, hit[0], hit[2])
}

fn terrain_point_from_world(
    battlefield: FlatBattlefield,
    world_x_mm: f32,
    world_z_mm: f32,
) -> Option<BattlePoint> {
    if !world_x_mm.is_finite()
        || !world_z_mm.is_finite()
        || world_x_mm < 0.0
        || world_z_mm < 0.0
        || world_x_mm > battlefield.width_mm as f32
        || world_z_mm > battlefield.depth_mm as f32
    {
        return None;
    }
    Some(BattlePoint::new(
        world_x_mm.round() as u32,
        world_z_mm.round() as u32,
    ))
}
'''
new_helpers = '''#[derive(Clone, Copy, Debug, PartialEq)]
struct TerrainRayHit {
    distance: f32,
    x0_mm: u32,
    x1_mm: u32,
    z0_mm: u32,
    z1_mm: u32,
}

fn terrain_ray_hit(battlefield: FlatBattlefield, ray: ViewportRay) -> Option<TerrainRayHit> {
    if battlefield.width_mm == 0 || battlefield.depth_mm == 0 {
        return None;
    }

    let mut nearest: Option<TerrainRayHit> = None;
    for cell_x in 0..TERRAIN_GRID_SIZE {
        for cell_z in 0..TERRAIN_GRID_SIZE {
            let Some((x0_mm, x1_mm, z0_mm, z1_mm)) =
                terrain_cell_bounds_mm(battlefield, cell_x, cell_z)
            else {
                continue;
            };
            let maximum_y = terrain_cell_height_mm(battlefield, cell_x, cell_z) as f32;
            let Some(distance) = ray_aabb_entry_distance(
                ray,
                [x0_mm as f32, 0.0, z0_mm as f32],
                [x1_mm as f32, maximum_y, z1_mm as f32],
            ) else {
                continue;
            };
            let candidate = TerrainRayHit {
                distance,
                x0_mm,
                x1_mm,
                z0_mm,
                z1_mm,
            };
            if nearest.is_none_or(|current| candidate.distance < current.distance) {
                nearest = Some(candidate);
            }
        }
    }
    nearest
}

fn ray_aabb_entry_distance(
    ray: ViewportRay,
    minimum: [f32; 3],
    maximum: [f32; 3],
) -> Option<f32> {
    let mut entry = 0.0_f32;
    let mut exit = f32::INFINITY;
    for axis in 0..3 {
        let minimum = minimum[axis] - TERRAIN_PICK_EPSILON_MM;
        let maximum = maximum[axis] + TERRAIN_PICK_EPSILON_MM;
        let origin = ray.origin_mm[axis];
        let direction = ray.direction[axis];
        if direction.abs() <= f32::EPSILON {
            if origin < minimum || origin > maximum {
                return None;
            }
            continue;
        }

        let first = (minimum - origin) / direction;
        let second = (maximum - origin) / direction;
        entry = entry.max(first.min(second));
        exit = exit.min(first.max(second));
        if exit < entry || exit < 0.0 {
            return None;
        }
    }
    let distance = entry.max(0.0);
    distance.is_finite().then_some(distance)
}

fn terrain_point_from_hit(
    battlefield: FlatBattlefield,
    ray: ViewportRay,
    hit: TerrainRayHit,
) -> Option<BattlePoint> {
    let world = add(ray.origin_mm, scale(ray.direction, hit.distance));
    terrain_point_from_world(
        battlefield,
        world[0].clamp(hit.x0_mm as f32, hit.x1_mm as f32),
        world[2].clamp(hit.z0_mm as f32, hit.z1_mm as f32),
    )
}

fn terrain_point_from_world(
    battlefield: FlatBattlefield,
    world_x_mm: f32,
    world_z_mm: f32,
) -> Option<BattlePoint> {
    let x_mm = tolerant_battlefield_coordinate(world_x_mm, battlefield.width_mm)?;
    let z_mm = tolerant_battlefield_coordinate(world_z_mm, battlefield.depth_mm)?;
    Some(BattlePoint::new(x_mm, z_mm))
}

fn tolerant_battlefield_coordinate(coordinate_mm: f32, span_mm: u32) -> Option<u32> {
    let span = span_mm as f32;
    if !coordinate_mm.is_finite()
        || coordinate_mm < -TERRAIN_PICK_EPSILON_MM
        || coordinate_mm > span + TERRAIN_PICK_EPSILON_MM
    {
        return None;
    }
    Some(coordinate_mm.clamp(0.0, span).round() as u32)
}
'''
text = replace_once(text, old_helpers, new_helpers, "terrain helpers")

test_anchor = '''    #[test]
    fn center_viewport_ray_hits_the_elevated_camera_target() {
'''
new_tests = '''    #[test]
    fn exact_battlefield_boundary_round_trips_with_pick_tolerance() {
        let battlefield = FlatBattlefield::new(100_000, 100_000);
        let camera = Camera3d::fit(battlefield);
        let point = BattlePoint::new(0, 2_500);
        let pixel = camera
            .project_ground_point(battlefield, point, 1_600.0, 900.0)
            .unwrap();
        let round_trip = camera
            .ground_point_from_viewport(battlefield, pixel[0], pixel[1], 1_600.0, 900.0)
            .unwrap();
        assert!(i64::from(round_trip.x_mm).abs() <= 1);
        assert!((i64::from(round_trip.y_mm) - i64::from(point.y_mm)).abs() <= 2);
    }

    #[test]
    fn close_center_ray_hits_the_visible_height_step() {
        let battlefield = FlatBattlefield::new(100_000, 100_000);
        let mut camera = Camera3d::fit(battlefield);
        camera.pan_ground(battlefield, 0.0, -37_500.0);
        camera.dolly(battlefield, 100.0);
        let target = BattlePoint::new(50_000, 12_500);
        let hit = camera
            .ground_point_from_viewport(battlefield, 500.0, 500.0, 1_000.0, 1_000.0)
            .unwrap();
        assert!((i64::from(hit.x_mm) - i64::from(target.x_mm)).abs() <= 2);
        assert!((i64::from(hit.y_mm) - i64::from(target.y_mm)).abs() <= 2);
    }

'''
text = replace_once(text, test_anchor, new_tests + test_anchor, "terrain regression tests")

path.write_text(text)
