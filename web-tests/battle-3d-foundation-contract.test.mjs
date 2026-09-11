import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

const [camera, scene, gpu, shader, browserBattle, controls, nativeInput, architecture] = await Promise.all([
  readFile(new URL("../crates/medieval-renderer/src/camera.rs", import.meta.url), "utf8"),
  readFile(new URL("../crates/medieval-renderer/src/scene.rs", import.meta.url), "utf8"),
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
  assert.doesNotMatch(camera, /Camera2d|center_x_mm|center_y_mm|pub zoom:/);
  assert.doesNotMatch(controls, /Camera2d|MIN_CAMERA_ZOOM|MAX_CAMERA_ZOOM/);
});

test("the production renderer consumes the same perspective basis with depth", () => {
  assert.match(gpu, /CameraUniform/);
  assert.match(gpu, /projection\(battlefield\)/);
  assert.match(gpu, /TextureFormat::Depth24Plus/);
  assert.match(gpu, /RenderPassDepthStencilAttachment/);
  assert.match(shader, /eye_near/);
  assert.match(shader, /right_tan_half_fov/);
  assert.match(shader, /view_z/);
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

test("the architecture records perspective convergence rather than compatibility debt", () => {
  assert.match(architecture, /3D mass-battle game/);
  assert.match(architecture, /Perspective camera contract/);
  assert.match(architecture, /Camera2d.*Removed/s);
  assert.match(architecture, /ray-to-flat-ground intersection/);
  assert.match(architecture, /deterministic terrain contract/);
  assert.match(architecture, /one production renderer and one camera geometry contract/);
});
