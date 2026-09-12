use std::collections::BTreeSet;

use medieval_core::{
    BattlePoint, BattleSide, DeploymentZone, FlatBattlefield, Formation, TacticalBattle,
    TacticalTerrainCell,
};

use crate::{Camera3d, terrain::terrain_height_mm};

const SOLDIER_SPACING_MM: f32 = 900.0;
const SOLDIER_CENTER_Y_MM: f32 = 900.0;

#[derive(Clone, Debug, PartialEq)]
pub struct RenderViewState {
    pub camera: Camera3d,
    selected_units: BTreeSet<String>,
    order_preview_units: BTreeSet<String>,
}

impl RenderViewState {
    #[must_use]
    pub fn fit(battlefield: FlatBattlefield) -> Self {
        Self {
            camera: Camera3d::fit(battlefield),
            selected_units: BTreeSet::new(),
            order_preview_units: BTreeSet::new(),
        }
    }

    #[must_use]
    pub fn with_selected(mut self, unit_id: impl Into<String>) -> Self {
        self.selected_units.insert(unit_id.into());
        self
    }

    #[must_use]
    pub fn with_order_preview(mut self, unit_id: impl Into<String>) -> Self {
        self.order_preview_units.insert(unit_id.into());
        self
    }

    #[must_use]
    pub fn is_selected(&self, unit_id: &str) -> bool {
        self.selected_units.contains(unit_id)
    }

    #[must_use]
    pub fn has_order_preview(&self, unit_id: &str) -> bool {
        self.order_preview_units.contains(unit_id)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct RenderUnitInstance {
    pub unit_id: String,
    pub side: BattleSide,
    pub position: BattlePoint,
    pub formation: Formation,
    pub soldiers: u16,
    pub frontage_slots: u16,
    pub morale: u16,
    pub fatigue: u16,
    pub routed: bool,
    pub selected: bool,
    pub order_preview: bool,
    pub terrain_elevation_mm: f32,
    /// Renderer world-space centers: ground X → world X, elevation → world Y,
    /// ground Y → world Z.
    pub soldier_centers_mm: Vec<[f32; 3]>,
}

impl RenderUnitInstance {
    #[must_use]
    pub fn interaction_anchor_mm(&self) -> [f32; 3] {
        [
            self.position.x_mm as f32,
            self.terrain_elevation_mm + SOLDIER_CENTER_Y_MM,
            self.position.y_mm as f32,
        ]
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct BattleRenderSnapshot {
    pub tick: u64,
    pub battlefield: FlatBattlefield,
    pub deployment_zones: [DeploymentZone; 2],
    pub forest_cells: Vec<TacticalTerrainCell>,
    pub river_cells: Vec<TacticalTerrainCell>,
    pub river_crossing_cells: Vec<TacticalTerrainCell>,
    pub camera: Camera3d,
    pub units: Vec<RenderUnitInstance>,
}

impl BattleRenderSnapshot {
    /// Captures authoritative battle state into renderer-owned world space.
    /// No pixel, clip-space, or platform-specific input data is stored here.
    #[must_use]
    pub fn capture(battle: &TacticalBattle, view: &RenderViewState) -> Self {
        let battlefield = battle.battlefield();
        let units = battle
            .units()
            .iter()
            .filter(|unit| !unit.is_destroyed())
            .map(|unit| {
                let terrain_elevation_mm = terrain_height_mm(battlefield, unit.position()) as f32;
                RenderUnitInstance {
                    unit_id: unit.id().to_owned(),
                    side: unit.side(),
                    position: unit.position(),
                    formation: unit.formation(),
                    soldiers: unit.soldiers(),
                    frontage_slots: unit.frontage_slots(),
                    morale: unit.morale(),
                    fatigue: unit.fatigue(),
                    routed: unit.is_routed(),
                    selected: view.is_selected(unit.id()),
                    order_preview: view.has_order_preview(unit.id()),
                    terrain_elevation_mm,
                    soldier_centers_mm: soldier_centers(
                        battlefield,
                        unit.position(),
                        unit.soldiers(),
                        unit.frontage_slots(),
                    ),
                }
            })
            .collect();
        Self {
            tick: battle.tick(),
            battlefield,
            deployment_zones: battle.deployment_zones(),
            forest_cells: battle.terrain().forest_cells().to_vec(),
            river_cells: battle.terrain().river_cells().to_vec(),
            river_crossing_cells: battle.terrain().river_crossing_cells().to_vec(),
            camera: view.camera,
            units,
        }
    }

    /// Projects core battle truth into the world-space render domain. This is
    /// intentionally a synonym for `capture`; screen projection remains on
    /// `Camera3d` and the GPU pipeline.
    #[must_use]
    pub fn project(battle: &TacticalBattle, view: &RenderViewState) -> Self {
        Self::capture(battle, view)
    }
}

fn soldier_centers(
    battlefield: FlatBattlefield,
    position: BattlePoint,
    soldiers: u16,
    frontage_slots: u16,
) -> Vec<[f32; 3]> {
    let count = usize::from(soldiers);
    let frontage = usize::from(frontage_slots.max(1).min(soldiers.max(1)));
    let ranks = count.div_ceil(frontage).max(1);
    let center_file = (frontage.saturating_sub(1)) as f32 / 2.0;
    let center_rank = (ranks.saturating_sub(1)) as f32 / 2.0;
    (0..count)
        .map(|index| {
            let file = index % frontage;
            let rank = index / frontage;
            let x = position.x_mm as f32 + (file as f32 - center_file) * SOLDIER_SPACING_MM;
            let z = position.y_mm as f32 + (rank as f32 - center_rank) * SOLDIER_SPACING_MM;
            let terrain_point = BattlePoint::new(
                x.round().clamp(0.0, battlefield.width_mm as f32) as u32,
                z.round().clamp(0.0, battlefield.depth_mm as f32) as u32,
            );
            [
                x,
                terrain_height_mm(battlefield, terrain_point) as f32 + SOLDIER_CENTER_Y_MM,
                z,
            ]
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use medieval_core::TacticalUnit;

    fn battle() -> TacticalBattle {
        TacticalBattle::new(
            FlatBattlefield::new(100_000, 100_000),
            vec![TacticalUnit::new(
                "attacker",
                BattleSide::Attacker,
                80,
                BattlePoint::new(50_000, 50_000),
                Formation::Line { files: 20 },
                1_000,
            )],
        )
        .unwrap()
    }

    #[test]
    fn capture_is_world_space_and_deterministic() {
        let battle = battle();
        let view = RenderViewState::fit(battle.battlefield()).with_selected("attacker");
        let first = BattleRenderSnapshot::capture(&battle, &view);
        let second = BattleRenderSnapshot::capture(&battle, &view);
        assert_eq!(first, second);
        assert_eq!(first, BattleRenderSnapshot::project(&battle, &view));
        assert_eq!(first.deployment_zones, battle.deployment_zones());
        assert_eq!(first.forest_cells, battle.terrain().forest_cells().to_vec());
        assert_eq!(first.river_cells, battle.terrain().river_cells().to_vec());
        assert_eq!(
            first.river_crossing_cells,
            battle.terrain().river_crossing_cells().to_vec()
        );
        assert_eq!(first.units[0].soldier_centers_mm.len(), 80);
        assert!(
            first.units[0]
                .soldier_centers_mm
                .iter()
                .flat_map(|center| center.iter())
                .all(|value| value.is_finite())
        );
        assert!(first.units[0].terrain_elevation_mm > 0.0);
        assert_eq!(
            first.units[0].interaction_anchor_mm(),
            [
                50_000.0,
                first.units[0].terrain_elevation_mm + SOLDIER_CENTER_Y_MM,
                50_000.0,
            ]
        );
        assert!(first.units[0].selected);
    }

    #[test]
    fn camera_changes_do_not_rewrite_world_geometry() {
        let battle = battle();
        let fit = RenderViewState::fit(battle.battlefield());
        let mut moved = fit.clone();
        moved.camera.pan_ground(battle.battlefield(), 5_000.0, 0.0);
        moved.camera.dolly(battle.battlefield(), 2.0);
        assert_eq!(
            BattleRenderSnapshot::capture(&battle, &fit).units[0].soldier_centers_mm,
            BattleRenderSnapshot::capture(&battle, &moved).units[0].soldier_centers_mm
        );
    }

    #[test]
    fn soldier_centers_follow_the_terrain_surface() {
        let battlefield = FlatBattlefield::new(100_000, 100_000);
        let centers = soldier_centers(battlefield, BattlePoint::new(50_000, 50_000), 1, 1);
        assert_eq!(centers.len(), 1);
        assert_eq!(
            centers[0][1],
            terrain_height_mm(battlefield, BattlePoint::new(50_000, 50_000)) as f32
                + SOLDIER_CENTER_Y_MM
        );
    }
}
