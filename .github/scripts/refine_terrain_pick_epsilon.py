from pathlib import Path

path = Path("crates/medieval-renderer/src/camera.rs")
text = path.read_text()
old = """    for axis in 0..3 {\n        let minimum = minimum[axis] - TERRAIN_PICK_EPSILON_MM;\n        let maximum = maximum[axis] + TERRAIN_PICK_EPSILON_MM;\n        let origin = ray.origin_mm[axis];\n"""
new = """    for axis in 0..3 {\n        let epsilon = if axis == 1 { 0.0 } else { TERRAIN_PICK_EPSILON_MM };\n        let minimum = minimum[axis] - epsilon;\n        let maximum = maximum[axis] + epsilon;\n        let origin = ray.origin_mm[axis];\n"""
if text.count(old) != 1:
    raise RuntimeError(f"expected one ray slab tolerance anchor, found {text.count(old)}")
text = text.replace(old, new)

test_anchor = """    #[test]\n    fn exact_battlefield_boundary_round_trips_with_pick_tolerance() {\n"""
new_test = """    #[test]\n    fn horizontal_pick_tolerance_does_not_shift_top_surface_round_trip() {\n        let battlefield = FlatBattlefield::new(100_000, 100_000);\n        let camera = Camera3d::fit(battlefield);\n        let point = BattlePoint::new(55_000, 45_000);\n        let pixel = camera\n            .project_ground_point(battlefield, point, 1_600.0, 900.0)\n            .unwrap();\n        assert_eq!(\n            camera.ground_point_from_viewport(\n                battlefield,\n                pixel[0],\n                pixel[1],\n                1_600.0,\n                900.0,\n            ),\n            Some(point)\n        );\n    }\n\n"""
if text.count(test_anchor) != 1:
    raise RuntimeError(f"expected one test insertion anchor, found {text.count(test_anchor)}")
text = text.replace(test_anchor, new_test + test_anchor)
path.write_text(text)
