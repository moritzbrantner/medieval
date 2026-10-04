use medieval_core::{BattlefieldLocation, FlatBattlefield, TacticalTerrain};
use medieval_renderer::TerrainMesh;
use std::{hint::black_box, time::Instant};
fn main() {
    let field = FlatBattlefield::new(100_003, 80_005);
    println!("location,vertices,bytes,mean_prepare_us");
    for location in BattlefieldLocation::ALL {
        let terrain = TacticalTerrain::for_location(location);
        let mesh = TerrainMesh::prepare(terrain, field);
        let start = Instant::now();
        for _ in 0..1_000 {
            black_box(TerrainMesh::prepare(terrain, field));
        }
        println!(
            "{location:?},{},{},{:.2}",
            mesh.vertices.len(),
            std::mem::size_of_val(mesh.vertices.as_slice()),
            start.elapsed().as_secs_f64() * 1_000.0
        );
    }
}
