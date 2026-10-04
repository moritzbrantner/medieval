use medieval_core::{BattlePoint, BattlefieldLocation, FlatBattlefield, TacticalTerrain};
use medieval_renderer::TerrainMesh;

#[test]
fn representative_geometry_is_deterministic_and_has_a_fixed_budget() {
    let field = FlatBattlefield::new(100_003, 80_005);
    for location in BattlefieldLocation::ALL {
        let terrain = TacticalTerrain::for_location(location);
        let mesh = TerrainMesh::prepare(terrain, field);
        assert_eq!(mesh, TerrainMesh::prepare(terrain, field));
        assert!(!mesh.vertices.is_empty());
        assert!(
            mesh.vertices.len() <= 16_384,
            "{location:?} exceeds terrain geometry budget"
        );
        assert!(std::mem::size_of_val(mesh.vertices.as_slice()) <= 768 * 1024);
        for vertex in &mesh.vertices {
            assert!(
                vertex
                    .position
                    .iter()
                    .chain(vertex.normal.iter())
                    .chain(vertex.color.iter())
                    .all(|value| value.is_finite())
            );
            assert!((0.0..=field.width_mm as f32).contains(&vertex.position[0]));
            assert!((0.0..=field.depth_mm as f32).contains(&vertex.position[2]));
            assert!(vertex.position[1] >= -200.0);
            let length = vertex.normal[..3]
                .iter()
                .map(|component| component * component)
                .sum::<f32>();
            assert!((length - 1.0).abs() < 0.0001);
        }
    }
}

#[test]
fn every_top_triangle_matches_the_authoritative_logical_surface() {
    let field = FlatBattlefield::new(100_003, 80_005);
    for location in BattlefieldLocation::ALL {
        let terrain = TacticalTerrain::for_location(location);
        let mesh = TerrainMesh::prepare(terrain, field);
        for triangle in mesh
            .vertices
            .as_chunks::<3>()
            .0
            .iter()
            .filter(|triangle| triangle[0].normal[1] > 0.99)
        {
            let x = triangle
                .iter()
                .map(|vertex| vertex.position[0])
                .sum::<f32>()
                / 3.0;
            let z = triangle
                .iter()
                .map(|vertex| vertex.position[2])
                .sum::<f32>()
                / 3.0;
            let expected = terrain.height_mm(field, BattlePoint::new(x as u32, z as u32)) as f32;
            assert!(
                triangle.iter().all(|vertex| vertex.position[1] == expected),
                "{location:?}: top differs at ({x},{z})"
            );
        }
    }
}

#[test]
fn tiny_cells_do_not_generate_nonfinite_or_degenerate_top_triangles() {
    for field in [FlatBattlefield::new(1, 1), FlatBattlefield::new(7, 5)] {
        let mesh = TerrainMesh::prepare(
            TacticalTerrain::for_location(BattlefieldLocation::ForestClearing),
            field,
        );
        assert!(!mesh.vertices.is_empty());
        assert!(
            mesh.vertices
                .iter()
                .all(|vertex| vertex.normal.iter().all(|value| value.is_finite()))
        );
    }
}

#[test]
fn collapsed_grid_neighbors_do_not_leave_tiny_field_perimeters_open() {
    for field in [FlatBattlefield::new(1, 1), FlatBattlefield::new(7, 5)] {
        let mesh = TerrainMesh::prepare(
            TacticalTerrain::for_location(BattlefieldLocation::ForestClearing),
            field,
        );
        for [x, z] in [[-1.0, 0.0], [1.0, 0.0], [0.0, -1.0], [0.0, 1.0]] {
            assert!(
                mesh.vertices
                    .iter()
                    .any(|vertex| vertex.normal[0] * x + vertex.normal[2] * z > 0.5),
                "field {field:?} is missing exposed perimeter direction ({x},{z})"
            );
        }
    }
}
