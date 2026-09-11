import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

const [camera, scene, terrain, gpu, shader, browserBattle, controls, nativeInput, architecture] = await Promise.all([
  readFile(new URL("../crates/medieval-renderer/src/camera.rs", import.meta.url), "utf8"),
  readFile(new URL("../crates/medieval-renderer/src/scene.rs", import.meta.url), "utf8"),
  readFile(new URL("../crates/medieval-renderer/src/terrain.rs", import.meta.url), "utf8"),
  readFile(new URL("../crates/medieval-renderer/src/gpu.rs", import.meta.url), "utf8"),
  readFile(new URL("../crates/medieval-renderer/src/battlefield.wgsl", import.meta.url), "utf8"),
  readFile(new URL("../web-battle-wasm/src/lib.rs", import.meta.url), "utf8"),
  readFile(new URL("../shared/tactical_controls.rs", import.meta.url), "utf8"),
  readFile(new URL("../src-tauri/src/native_battle/input.rs", import.meta.url), "utf8"),
  readFile(new URL("../docs/BATTLE-ARCHITECTURE.md", import.meta.url), "utf8"),
]);

test("battle render snapshots remain world-space rather than clip-space rectangles", () => {
  assert.match(camera, /pub struct Camera3d/);
  assert.match(scene, /pub soldier_centers_mm: Vec<\[f32; 3\]>/);
  assert.match(scene, /pub fn interaction_anchor_mm\(/);
  assert.match(scene, /pub fn capture\(/);
  assert.doesNotMatch(scene, /clip_center|clip_half_extent|fn project_rect/);
});

test("one perspective camera owns projection and viewport rays", () => {
  assert.match(camera, /target_x_mm/);
  assert.match(camera, /distance_mm/);
  assert.match(camera, /yaw_radians/);
  assert.match(camera, /pitch_radians/);
  assert.match(camera, /fov_y_radians/);
  assert.match(camera, /pub fn project_world_point\(/);
  assert.match(camera, /pub fn viewport_ray\(/);
  assert.match(camera, /pub fn ground_point_from_viewport\(/);
  assert.match(camera, /fn viewport_tangents\(/);
  assert.match(camera, /fn terrain_ray_hit\(/);
  assert.match(camera, /fn ray_aabb_entry_distance\(/);
  assert.match(camera, /TERRAIN_PICK_EPSILON_MM/);
  assert.doesNotMatch(camera, /refine_terrain_hit\(|TERRAIN_RAY_MARCH_STEPS/);
  assert.match(camera, /small_battlefield_pan_step_has_ordered_bounds/);
  assert.doesNotMatch(camera, /Camera2d|center_x_mm|center_y_mm|pub zoom:/);
  assert.doesNotMatch(controls, /Camera2d|MIN_CAMERA_ZOOM|MAX_CAMERA_ZOOM/);
});

test("one deterministic renderer terrain surface drives geometry and interaction", () => {
  assert.match(terrain, /TERRAIN_GRID_SIZE/);
  assert.match(terrain, /terrain_height_mm\(/);
  assert.match(terrain, /terrain_cell_height_mm\(/);
  assert.match(scene, /terrain_height_mm\(battlefield/);
  assert.match(gpu, /terrain_cell_height_mm/);
  assert.match(camera, /terrain_height_mm\(battlefield/);
  assert.match(camera, /terrain_cell_bounds_mm/);
  assert.match(camera, /terrain_cell_height_mm/);
  assert.match(architecture, /cell volumes directly/);
  assert.match(architecture, /gameplay-neutral/);
  assert.match(architecture, /must move into `medieval-core`/);
});

test("the production renderer consumes the same aspect-safe perspective basis with depth", () => {
  assert.match(gpu, /CameraUniform/);
  assert.match(gpu, /projection\(battlefield\)/);
  assert.match(gpu, /TextureFormat::Depth24Plus/);
  assert.match(gpu, /RenderPassDepthStencilAttachment/);
  assert.match(shader, /eye_near/);
  assert.match(shader, /right_tan_half_fov/);
  assert.match(shader, /view_z/);
  assert.match(shader, /tan_half_x/);
  assert.match(shader, /tan_half_y/);
  assert.match(shader, /max\(aspect, 1\.0\)/);
  assert.match(shader, /min\(aspect, 1\.0\)/);
  assert.match(shader, /output\.position = vec4<f32>/);
  assert.doesNotMatch(shader, /center_zoom_elevation|elevation_lift/);
});

test("browser and native input use renderer-owned perspective geometry", () => {
  assert.match(browserBattle, /unit\.interaction_anchor_mm\(\)/);
  assert.match(browserBattle, /snapshot\.camera\.project_world_point\(/);
  assert.match(browserBattle, /snapshot\.camera\.ground_point_from_viewport\(/);
  assert.doesNotMatch(browserBattle, /fn sanitized_zoom\(|half_width.*clip_x/s);

  assert.match(nativeInput, /nearest_unit_at_pointer/);
  assert.match(nativeInput, /project_world_point\(/);
  assert.match(nativeInput, /ground_point_from_viewport\(/);
  assert.match(nativeInput, /units_in_viewport_rect/);
  assert.doesNotMatch(nativeInput, /CLICK_RADIUS_FRACTION|MIN_CLICK_RADIUS_MM|MAX_CLICK_RADIUS_MM/);
});

test("the architecture records perspective and terrain convergence rather than compatibility debt", () => {
  assert.match(architecture, /3D mass-battle game/);
  assert.match(architecture, /Perspective camera contract/);
  assert.match(architecture, /Camera2d.*Removed/s);
  assert.match(architecture, /ray-to-terrain intersection/);
  assert.match(architecture, /Terrain height foundation/);
  assert.match(architecture, /one production renderer and one camera geometry contract/);
});