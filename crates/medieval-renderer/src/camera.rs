use medieval_core::{BattlePoint, FlatBattlefield};

const DEFAULT_FOV_Y_RADIANS: f32 = 0.785_398_2;
const DEFAULT_PITCH_RADIANS: f32 = 0.872_664_63;
const DEFAULT_YAW_RADIANS: f32 = 0.0;
const FIT_DISTANCE_MARGIN: f32 = 1.35;
const MIN_DISTANCE_FRACTION: f32 = 0.04;
const MAX_DISTANCE_FRACTION: f32 = 5.0;
const NEAR_PLANE_MM: f32 = 100.0;

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
        self.target_x_mm =
            (self.target_x_mm + delta_x_mm).clamp(0.0, battlefield.width_mm as f32);
        self.target_z_mm =
            (self.target_z_mm + delta_z_mm).clamp(0.0, battlefield.depth_mm as f32);
    }

    pub fn dolly(&mut self, battlefield: FlatBattlefield, factor: f32) {
        let max_span = battlefield.width_mm.max(battlefield.depth_mm) as f32;
        let min_distance = (max_span * MIN_DISTANCE_FRACTION).max(1_000.0);
        let max_distance = (max_span * MAX_DISTANCE_FRACTION).max(min_distance);
        self.distance_mm = (self.distance_mm / factor).clamp(min_distance, max_distance);
    }

    #[must_use]
    pub fn pan_step_mm(self, battlefield: FlatBattlefield) -> f32 {
        let max_span = battlefield.width_mm.max(battlefield.depth_mm) as f32;
        (self.distance_mm * 0.04).clamp(1_000.0, max_span * 0.1)
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
        let aspect = viewport_width_px / viewport_height_px;
        let ndc_x = view_x / (view_z * projection.tan_half_fov_y * aspect);
        let ndc_y = view_y / (view_z * projection.tan_half_fov_y);
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
            [point.x_mm as f32, 0.0, point.y_mm as f32],
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
        let aspect = viewport_width_px / viewport_height_px;
        let ndc_x = x_px / viewport_width_px * 2.0 - 1.0;
        let ndc_y = 1.0 - y_px / viewport_height_px * 2.0;
        let direction = normalize(add(
            projection.forward,
            add(
                scale(
                    projection.right,
                    ndc_x * projection.tan_half_fov_y * aspect,
                ),
                scale(projection.up, ndc_y * projection.tan_half_fov_y),
            ),
        ))?;
        Some(ViewportRay {
            origin_mm: projection.eye_mm,
            direction,
        })
    }

    /// Intersects the renderer-owned viewport ray with the current flat ground
    /// plane. Terrain can replace this intersection without changing platform
    /// input adapters.
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
        if ray.direction[1] >= -f32::EPSILON {
            return None;
        }
        let distance = -ray.origin_mm[1] / ray.direction[1];
        if !distance.is_finite() || distance < 0.0 {
            return None;
        }
        let hit = add(ray.origin_mm, scale(ray.direction, distance));
        if hit[0] < 0.0
            || hit[2] < 0.0
            || hit[0] > battlefield.width_mm as f32
            || hit[2] > battlefield.depth_mm as f32
        {
            return None;
        }
        Some(BattlePoint::new(hit[0].round() as u32, hit[2].round() as u32))
    }

    #[must_use]
    pub(crate) fn projection(self, battlefield: FlatBattlefield) -> CameraProjection {
        let cos_pitch = self.pitch_radians.cos();
        let offset = [
            self.yaw_radians.sin() * cos_pitch * self.distance_mm,
            self.pitch_radians.sin() * self.distance_mm,
            self.yaw_radians.cos() * cos_pitch * self.distance_mm,
        ];
        let target = [self.target_x_mm, 0.0, self.target_z_mm];
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

fn valid_viewport(width: f32, height: f32) -> bool {
    width.is_finite() && height.is_finite() && width > 0.0 && height > 0.0
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
    fn projected_ground_point_round_trips_through_the_viewport_ray() {
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
        assert!(dx.abs() <= 1);
        assert!(dy.abs() <= 1);
    }

    #[test]
    fn center_viewport_ray_hits_the_camera_target() {
        let battlefield = FlatBattlefield::new(100_000, 100_000);
        let camera = Camera3d::fit(battlefield);
        assert_eq!(
            camera.ground_point_from_viewport(battlefield, 500.0, 500.0, 1_000.0, 1_000.0),
            Some(BattlePoint::new(50_000, 50_000))
        );
    }

    #[test]
    fn elevation_changes_perspective_projection() {
        let battlefield = FlatBattlefield::new(100_000, 100_000);
        let camera = Camera3d::fit(battlefield);
        let ground = camera
            .project_world_point(battlefield, [50_000.0, 0.0, 50_000.0], 1_000.0, 1_000.0)
            .unwrap();
        let elevated = camera
            .project_world_point(battlefield, [50_000.0, 1_800.0, 50_000.0], 1_000.0, 1_000.0)
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
        assert!(camera.distance_mm() >= 1_000.0);
    }
}
