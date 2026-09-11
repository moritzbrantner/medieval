use medieval_core::{BattlePoint, FlatBattlefield};

use crate::terrain::{
    TERRAIN_GRID_SIZE, terrain_cell_bounds_mm, terrain_cell_height_mm, terrain_height_mm,
};

const DEFAULT_FOV_Y_RADIANS: f32 = std::f32::consts::FRAC_PI_4;
const DEFAULT_PITCH_RADIANS: f32 = 0.872_664_63;
const DEFAULT_YAW_RADIANS: f32 = 0.0;
const FIT_DISTANCE_MARGIN: f32 = 1.35;
const MIN_DISTANCE_FRACTION: f32 = 0.04;
const MAX_DISTANCE_FRACTION: f32 = 5.0;
const MIN_PAN_STEP_FRACTION: f32 = 0.01;
const MAX_PAN_STEP_FRACTION: f32 = 0.1;
const NEAR_PLANE_MM: f32 = 100.0;
const TERRAIN_PICK_EPSILON_MM: f32 = 1.0;

#[derive(Copy, Clone, Debug, PartialEq)]
pub struct ViewportRay {
    pub origin_mm: [f32; 3],
    pub direction: [f32; 3],
}

/// Renderer-owned perspective tactical camera.
///
/// Ground X remains world X, ground Y becomes world Z, and world Y is
/// reserved for terrain/soldier elevation. Camera state never enters
/// `medieval-core`.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct Camera3d {
    target_x_mm: f32,
    target_z_mm: f32,
    distance_mm: f32,
    yaw_radians: f32,
    pitch_radians: f32,
    fov_y_radians: f32,
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub(crate) struct CameraProjection {
    pub eye_mm: [f32; 3],
    pub right: [f32; 3],
    pub up: [f32; 3],
    pub forward: [f32; 3],
    pub tan_half_fov_y: f32,
    pub near_mm: f32,
    pub far_mm: f32,
}

impl Camera3d {
    #[must_use]
    pub fn fit(battlefield: FlatBattlefield) -> Self {
        let max_span = battlefield.width_mm.max(battlefield.depth_mm) as f32;
        let half_span = max_span / 2.0;
        let distance_mm = half_span / (DEFAULT_FOV_Y_RADIANS / 2.0).tan() * FIT_DISTANCE_MARGIN;
        Self {
            target_x_mm: battlefield.width_mm as f32 / 2.0,
            target_z_mm: battlefield.depth_mm as f32 / 2.0,
            distance_mm,
            yaw_radians: DEFAULT_YAW_RADIANS,
            pitch_radians: DEFAULT_PITCH_RADIANS,
            fov_y_radians: DEFAULT_FOV_Y_RADIANS,
        }
    }

    #[must_use]
    pub const fn target_x_mm(self) -> f32 {
        self.target_x_mm
    }

    #[must_use]
    pub const fn target_z_mm(self) -> f32 {
        self.target_z_mm
    }

    #[must_use]
    pub const fn distance_mm(self) -> f32 {
        self.distance_mm
    }

    #[must_use]
    pub const fn yaw_radians(self) -> f32 {
        self.yaw_radians
    }

    #[must_use]
    pub const fn pitch_radians(self) -> f32 {
        self.pitch_radians
    }

    #[must_use]
    pub const fn fov_y_radians(self) -> f32 {
        self.fov_y_radians
    }

    pub fn pan_ground(&mut self, battlefield: FlatBattlefield, delta_x_mm: f32, delta_z_mm: f32) {
        self.target_x_mm = (self.target_x_mm + delta_x_mm).clamp(0.0, battlefield.width_mm as f32);
        self.target_z_mm = (self.target_z_mm + delta_z_mm).clamp(0.0, battlefield.depth_mm as f32);
    }

    pub fn dolly(&mut self, battlefield: FlatBattlefield, factor: f32) {
        let max_span = battlefield.width_mm.max(battlefield.depth_mm) as f32;
        let min_distance = (max_span * MIN_DISTANCE_FRACTION).max(1.0);
        let max_distance = (max_span * MAX_DISTANCE_FRACTION).max(min_distance);
        self.distance_mm = (self.distance_mm / factor).clamp(min_distance, max_distance);
    }

    #[must_use]
    pub fn pan_step_mm(self, battlefield: FlatBattlefield) -> f32 {
        let max_span = battlefield.width_mm.max(battlefield.depth_mm) as f32;
        let min_step = (max_span * MIN_PAN_STEP_FRACTION).max(1.0);
        let max_step = (max_span * MAX_PAN_STEP_FRACTION).max(min_step);
        (self.distance_mm * 0.04).clamp(min_step, max_step)
    }

    #[must_use]
    pub fn project_world_point(
        self,
        battlefield: FlatBattlefield,
        world_position_mm: [f32; 3],
        viewport_width_px: f32,
        viewport_height_px: f32,
    ) -> Option<[f32; 2]> {
        if !world_position_mm.into_iter().all(f32::is_finite)
            || !valid_viewport(viewport_width_px, viewport_height_px)
        {
            return None;
        }
        let projection = self.projection(battlefield);
        let relative = sub(world_position_mm, projection.eye_mm);
        let view_x = dot(relative, projection.right);
        let view_y = dot(relative, projection.up);
        let view_z = dot(relative, projection.forward);
        if view_z <= projection.near_mm || view_z >= projection.far_mm {
            return None;
        }
        let (tan_half_x, tan_half_y) = viewport_tangents(
            projection.tan_half_fov_y,
            viewport_width_px,
            viewport_height_px,
        );
        let ndc_x = view_x / (view_z * tan_half_x);
        let ndc_y = view_y / (view_z * tan_half_y);
        if !ndc_x.is_finite() || !ndc_y.is_finite() {
            return None;
        }
        Some([
            (ndc_x + 1.0) * viewport_width_px / 2.0,
            (1.0 - ndc_y) * viewport_height_px / 2.0,
        ])
    }

    #[must_use]
    pub fn project_ground_point(
        self,
        battlefield: FlatBattlefield,
        point: BattlePoint,
        viewport_width_px: f32,
        viewport_height_px: f32,
    ) -> Option<[f32; 2]> {
        self.project_world_point(
            battlefield,
            [
                point.x_mm as f32,
                terrain_height_mm(battlefield, point) as f32,
                point.y_mm as f32,
            ],
            viewport_width_px,
            viewport_height_px,
        )
    }

    #[must_use]
    pub fn viewport_ray(
        self,
        battlefield: FlatBattlefield,
        x_px: f32,
        y_px: f32,
        viewport_width_px: f32,
        viewport_height_px: f32,
    ) -> Option<ViewportRay> {
        if ![x_px, y_px].into_iter().all(f32::is_finite)
            || !valid_viewport(viewport_width_px, viewport_height_px)
        {
            return None;
        }
        let projection = self.projection(battlefield);
        let ndc_x = x_px / viewport_width_px * 2.0 - 1.0;
        let ndc_y = 1.0 - y_px / viewport_height_px * 2.0;
        let (tan_half_x, tan_half_y) = viewport_tangents(
            projection.tan_half_fov_y,
            viewport_width_px,
            viewport_height_px,
        );
        let direction = normalize(add(
            projection.forward,
            add(
                scale(projection.right, ndc_x * tan_half_x),
                scale(projection.up, ndc_y * tan_half_y),
            ),
        ))?;
        Some(ViewportRay {
            origin_mm: projection.eye_mm,
            direction,
        })
    }

    /// Intersects the renderer-owned viewport ray with the deterministic
    /// renderer terrain surface. Platform adapters continue to supply only
    /// physical viewport coordinates and receive a ground-space `BattlePoint`.
    #[must_use]
    pub fn ground_point_from_viewport(
        self,
        battlefield: FlatBattlefield,
        x_px: f32,
        y_px: f32,
        viewport_width_px: f32,
        viewport_height_px: f32,
    ) -> Option<BattlePoint> {
        let ray = self.viewport_ray(
            battlefield,
            x_px,
            y_px,
            viewport_width_px,
            viewport_height_px,
        )?;
        let hit = terrain_ray_hit(battlefield, ray)?;
        terrain_point_from_hit(battlefield, ray, hit)
    }

    #[must_use]
    pub(crate) fn projection(self, battlefield: FlatBattlefield) -> CameraProjection {
        let cos_pitch = self.pitch_radians.cos();
        let target_point = BattlePoint::new(
            self.target_x_mm
                .round()
                .clamp(0.0, battlefield.width_mm as f32) as u32,
            self.target_z_mm
                .round()
                .clamp(0.0, battlefield.depth_mm as f32) as u32,
        );
        let target = [
            self.target_x_mm,
            terrain_height_mm(battlefield, target_point) as f32,
            self.target_z_mm,
        ];
        let offset = [
            self.yaw_radians.sin() * cos_pitch * self.distance_mm,
            self.pitch_radians.sin() * self.distance_mm,
            self.yaw_radians.cos() * cos_pitch * self.distance_mm,
        ];
        let eye = add(target, offset);
        let forward = normalize(sub(target, eye)).expect("camera eye and target are distinct");
        let right = normalize(cross(forward, [0.0, 1.0, 0.0]))
            .expect("tactical camera pitch never aligns with world up");
        let up = cross(right, forward);
        let max_span = battlefield.width_mm.max(battlefield.depth_mm) as f32;
        CameraProjection {
            eye_mm: eye,
            right,
            up,
            forward,
            tan_half_fov_y: (self.fov_y_radians / 2.0).tan(),
            near_mm: NEAR_PLANE_MM,
            far_mm: self.distance_mm + max_span * 4.0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
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

fn ray_aabb_entry_distance(ray: ViewportRay, minimum: [f32; 3], maximum: [f32; 3]) -> Option<f32> {
    let mut entry = 0.0_f32;
    let mut exit = f32::INFINITY;
    for axis in 0..3 {
        let epsilon = if axis == 1 {
            0.0
        } else {
            TERRAIN_PICK_EPSILON_MM
        };
        let minimum = minimum[axis] - epsilon;
        let maximum = maximum[axis] + epsilon;
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

fn valid_viewport(width: f32, height: f32) -> bool {
    width.is_finite() && height.is_finite() && width > 0.0 && height > 0.0
}

fn viewport_tangents(base_tan_half_fov: f32, width: f32, height: f32) -> (f32, f32) {
    let aspect = width / height;
    if aspect >= 1.0 {
        (base_tan_half_fov * aspect, base_tan_half_fov)
    } else {
        (base_tan_half_fov, base_tan_half_fov / aspect)
    }
}

fn add(left: [f32; 3], right: [f32; 3]) -> [f32; 3] {
    [left[0] + right[0], left[1] + right[1], left[2] + right[2]]
}

fn sub(left: [f32; 3], right: [f32; 3]) -> [f32; 3] {
    [left[0] - right[0], left[1] - right[1], left[2] - right[2]]
}

fn scale(vector: [f32; 3], factor: f32) -> [f32; 3] {
    [vector[0] * factor, vector[1] * factor, vector[2] * factor]
}

fn dot(left: [f32; 3], right: [f32; 3]) -> f32 {
    left[0] * right[0] + left[1] * right[1] + left[2] * right[2]
}

fn cross(left: [f32; 3], right: [f32; 3]) -> [f32; 3] {
    [
        left[1] * right[2] - left[2] * right[1],
        left[2] * right[0] - left[0] * right[2],
        left[0] * right[1] - left[1] * right[0],
    ]
}

fn normalize(vector: [f32; 3]) -> Option<[f32; 3]> {
    let length_squared = dot(vector, vector);
    if !length_squared.is_finite() || length_squared <= f32::EPSILON {
        return None;
    }
    Some(scale(vector, length_squared.sqrt().recip()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn projected_ground_point_round_trips_through_the_terrain_ray() {
        let battlefield = FlatBattlefield::new(100_000, 80_000);
        let camera = Camera3d::fit(battlefield);
        let point = BattlePoint::new(34_000, 61_000);
        let pixel = camera
            .project_ground_point(battlefield, point, 1_600.0, 900.0)
            .unwrap();
        let round_trip = camera
            .ground_point_from_viewport(battlefield, pixel[0], pixel[1], 1_600.0, 900.0)
            .unwrap();
        let dx = i64::from(round_trip.x_mm) - i64::from(point.x_mm);
        let dy = i64::from(round_trip.y_mm) - i64::from(point.y_mm);
        assert!(dx.abs() <= 2);
        assert!(dy.abs() <= 2);
    }

    #[test]
    fn zero_height_perimeter_ground_point_round_trips() {
        let battlefield = FlatBattlefield::new(100_000, 100_000);
        let camera = Camera3d::fit(battlefield);
        let point = BattlePoint::new(50_000, 95_000);
        assert_eq!(terrain_height_mm(battlefield, point), 0);
        let pixel = camera
            .project_ground_point(battlefield, point, 1_600.0, 900.0)
            .unwrap();
        let round_trip = camera
            .ground_point_from_viewport(battlefield, pixel[0], pixel[1], 1_600.0, 900.0)
            .unwrap();
        let dx = i64::from(round_trip.x_mm) - i64::from(point.x_mm);
        let dy = i64::from(round_trip.y_mm) - i64::from(point.y_mm);
        assert!(dx.abs() <= 2);
        assert!(dy.abs() <= 2);
    }

    #[test]
    fn horizontal_pick_tolerance_does_not_shift_top_surface_round_trip() {
        let battlefield = FlatBattlefield::new(100_000, 100_000);
        let camera = Camera3d::fit(battlefield);
        let point = BattlePoint::new(55_000, 45_000);
        let pixel = camera
            .project_ground_point(battlefield, point, 1_600.0, 900.0)
            .unwrap();
        assert_eq!(
            camera.ground_point_from_viewport(battlefield, pixel[0], pixel[1], 1_600.0, 900.0,),
            Some(point)
        );
    }

    #[test]
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

    #[test]
    fn center_viewport_ray_hits_the_elevated_camera_target() {
        let battlefield = FlatBattlefield::new(100_000, 100_000);
        let camera = Camera3d::fit(battlefield);
        let target = BattlePoint::new(50_000, 50_000);
        assert!(terrain_height_mm(battlefield, target) > 0);
        assert_eq!(
            camera.ground_point_from_viewport(battlefield, 500.0, 500.0, 1_000.0, 1_000.0),
            Some(target)
        );
    }

    #[test]
    fn fit_keeps_battlefield_corners_visible_on_portrait_viewports() {
        let battlefield = FlatBattlefield::new(100_000, 100_000);
        let camera = Camera3d::fit(battlefield);
        for point in [
            BattlePoint::new(0, 0),
            BattlePoint::new(100_000, 0),
            BattlePoint::new(0, 100_000),
            BattlePoint::new(100_000, 100_000),
        ] {
            let [x, y] = camera
                .project_ground_point(battlefield, point, 300.0, 1_000.0)
                .unwrap();
            assert!((0.0..=300.0).contains(&x));
            assert!((0.0..=1_000.0).contains(&y));
        }
    }

    #[test]
    fn elevation_changes_perspective_projection() {
        let battlefield = FlatBattlefield::new(100_000, 100_000);
        let camera = Camera3d::fit(battlefield);
        let point = BattlePoint::new(50_000, 50_000);
        let terrain_y = terrain_height_mm(battlefield, point) as f32;
        let ground = camera
            .project_world_point(
                battlefield,
                [50_000.0, terrain_y, 50_000.0],
                1_000.0,
                1_000.0,
            )
            .unwrap();
        let elevated = camera
            .project_world_point(
                battlefield,
                [50_000.0, terrain_y + 1_800.0, 50_000.0],
                1_000.0,
                1_000.0,
            )
            .unwrap();
        assert_eq!(elevated[0], ground[0]);
        assert_ne!(elevated[1], ground[1]);
    }

    #[test]
    fn pan_and_dolly_remain_bounded_by_the_battlefield() {
        let battlefield = FlatBattlefield::new(100_000, 80_000);
        let mut camera = Camera3d::fit(battlefield);
        let fit_distance = camera.distance_mm();
        camera.pan_ground(battlefield, 1_000_000.0, -1_000_000.0);
        assert_eq!(camera.target_x_mm(), 100_000.0);
        assert_eq!(camera.target_z_mm(), 0.0);
        camera.dolly(battlefield, 100.0);
        assert!(camera.distance_mm() < fit_distance);
        assert!(camera.distance_mm() >= 1.0);
    }

    #[test]
    fn small_battlefield_pan_step_has_ordered_bounds() {
        let battlefield = FlatBattlefield::new(2_000, 3_000);
        let camera = Camera3d::fit(battlefield);
        let step = camera.pan_step_mm(battlefield);
        assert!(step.is_finite());
        assert!((30.0..=300.0).contains(&step));
    }
}
