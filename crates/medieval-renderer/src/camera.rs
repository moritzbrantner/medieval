use medieval_core::{BattlePoint, FlatBattlefield};

const MIN_CAMERA_ZOOM: f32 = 0.05;
pub(crate) const COMPATIBILITY_ELEVATION_SCALE: f32 = 0.72;

/// Renderer-owned tactical camera state.
///
/// Ground X remains world X, ground Y becomes world Z, and world Y is
/// reserved for terrain/soldier elevation. Camera state never enters
/// `medieval-core`.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct Camera3d {
    pub center_x_mm: f32,
    pub center_y_mm: f32,
    pub zoom: f32,
}

impl Camera3d {
    #[must_use]
    pub fn fit(battlefield: FlatBattlefield) -> Self {
        Self {
            center_x_mm: battlefield.width_mm as f32 / 2.0,
            center_y_mm: battlefield.depth_mm as f32 / 2.0,
            zoom: 1.0,
        }
    }

    #[must_use]
    pub(crate) fn sanitized_zoom(self) -> f32 {
        if self.zoom.is_finite() && self.zoom > 0.0 {
            self.zoom.max(MIN_CAMERA_ZOOM)
        } else {
            1.0
        }
    }

    /// Current compatibility world projection. This is renderer-owned so the
    /// next perspective-camera slice has one projection boundary to replace.
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
        let half_width = battlefield.width_mm as f32 / 2.0;
        let half_depth = battlefield.depth_mm as f32 / 2.0;
        let zoom = self.sanitized_zoom();
        let clip_x = (world_position_mm[0] - self.center_x_mm) / half_width * zoom;
        let elevation_lift = world_position_mm[1] * COMPATIBILITY_ELEVATION_SCALE;
        let clip_y = (self.center_y_mm - world_position_mm[2] + elevation_lift) / half_depth * zoom;
        Some([
            (clip_x + 1.0) * viewport_width_px / 2.0,
            (1.0 - clip_y) * viewport_height_px / 2.0,
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

    /// Current compatibility flat-ground intersection. Terrain-aware ray
    /// intersection replaces this in the next mandatory camera slice.
    #[must_use]
    pub fn ground_point_from_viewport(
        self,
        battlefield: FlatBattlefield,
        x_px: f32,
        y_px: f32,
        viewport_width_px: f32,
        viewport_height_px: f32,
    ) -> Option<BattlePoint> {
        if ![x_px, y_px].into_iter().all(f32::is_finite)
            || !valid_viewport(viewport_width_px, viewport_height_px)
        {
            return None;
        }
        let clip_x = x_px / viewport_width_px * 2.0 - 1.0;
        let clip_y = 1.0 - y_px / viewport_height_px * 2.0;
        let zoom = self.sanitized_zoom();
        let half_width = battlefield.width_mm as f32 / 2.0;
        let half_depth = battlefield.depth_mm as f32 / 2.0;
        let x = self.center_x_mm + clip_x * half_width / zoom;
        let y = self.center_y_mm - clip_y * half_depth / zoom;
        if x < 0.0 || y < 0.0 || x > battlefield.width_mm as f32 || y > battlefield.depth_mm as f32
        {
            return None;
        }
        Some(BattlePoint::new(x.round() as u32, y.round() as u32))
    }
}

fn valid_viewport(width: f32, height: f32) -> bool {
    width.is_finite() && height.is_finite() && width > 0.0 && height > 0.0
}

/// Temporary source compatibility for the shared tactical controls. New
/// renderer code must use `Camera3d`; this alias is deleted with affine input.
pub type Camera2d = Camera3d;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ground_projection_round_trips_during_camera_migration() {
        let battlefield = FlatBattlefield::new(100_000, 80_000);
        let camera = Camera3d::fit(battlefield);
        let point = BattlePoint::new(34_000, 61_000);
        let pixel = camera
            .project_ground_point(battlefield, point, 1_600.0, 900.0)
            .unwrap();
        let round_trip = camera
            .ground_point_from_viewport(battlefield, pixel[0], pixel[1], 1_600.0, 900.0)
            .unwrap();
        assert_eq!(round_trip, point);
    }

    #[test]
    fn elevated_world_point_projects_above_its_ground_anchor() {
        let battlefield = FlatBattlefield::new(100_000, 100_000);
        let camera = Camera3d::fit(battlefield);
        let ground = camera
            .project_world_point(battlefield, [50_000.0, 0.0, 50_000.0], 1_000.0, 1_000.0)
            .unwrap();
        let elevated = camera
            .project_world_point(
                battlefield,
                [50_000.0, 1_800.0, 50_000.0],
                1_000.0,
                1_000.0,
            )
            .unwrap();
        assert_eq!(elevated[0], ground[0]);
        assert!(elevated[1] < ground[1]);
    }

    #[test]
    fn invalid_zoom_fails_safe() {
        let mut camera = Camera3d::fit(FlatBattlefield::new(10_000, 10_000));
        camera.zoom = f32::NAN;
        assert_eq!(camera.sanitized_zoom(), 1.0);
    }
}
