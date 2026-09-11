import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

const [camera, scene, gpu, shader, architecture] = await Promise.all([
  readFile(new URL("../crates/medieval-renderer/src/camera.rs", import.meta.url), "utf8"),
  readFile(new URL("../crates/medieval-renderer/src/scene.rs", import.meta.url), "utf8"),
  readFile(new URL("../crates/medieval-renderer/src/gpu.rs", import.meta.url), "utf8"),
  readFile(new URL("../crates/medieval-renderer/src/battlefield.wgsl", import.meta.url), "utf8"),
  readFile(new URL("../docs/BATTLE-ARCHITECTURE.md", import.meta.url), "utf8"),
]);

test("battle render snapshots are world-space rather than clip-space rectangles", () => {
  assert.match(camera, /pub struct Camera3d/);
  assert.match(scene, /pub soldier_centers_mm: Vec<\[f32; 3\]>/);
  assert.match(scene, /pub fn capture\(/);
  assert.doesNotMatch(scene, /clip_center|clip_half_extent|fn project_rect/);
});

test("the production renderer uses 3D geometry and a depth attachment", () => {
  assert.match(gpu, /CUBE_VERTEX_COUNT/);
  assert.match(gpu, /TextureFormat::Depth24Plus/);
  assert.match(gpu, /depth_stencil: Some/);
  assert.match(gpu, /RenderPassDepthStencilAttachment/);
  assert.match(shader, /fn cube_vertex/);
  assert.match(shader, /world_position/);
  assert.match(shader, /fn cube_normal/);
});

test("the architecture records the compatibility debt and mandatory camera migration", () => {
  assert.match(architecture, /3D mass-battle game/);
  assert.match(architecture, /Camera2d.*Compatibility/s);
  assert.match(architecture, /perspective tactical-camera model/);
  assert.match(architecture, /ray\/terrain intersection/);
  assert.match(architecture, /Do not maintain a long-lived 2D renderer/);
});
