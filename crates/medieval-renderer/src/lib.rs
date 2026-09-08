use std::collections::BTreeSet;

use bytemuck::{Pod, Zeroable};
use medieval_core::{BattlePoint, BattleSide, FlatBattlefield, Formation, TacticalBattle};
use wgpu::util::DeviceExt;

const SOLDIER_SPACING_MM: f32 = 900.0;
const MIN_CAMERA_ZOOM: f32 = 0.05;
const QUAD_VERTEX_COUNT: u32 = 6;
const INITIAL_INSTANCE_CAPACITY: usize = 16;
const SHADER_SOURCE: &str = include_str!("battlefield.wgsl");

const VERTEX_ATTRIBUTES: [wgpu::VertexAttribute; 1] = wgpu::vertex_attr_array![0 => Float32x2];
const INSTANCE_ATTRIBUTES: [wgpu::VertexAttribute; 2] =
    wgpu::vertex_attr_array![1 => Float32x4, 2 => Float32x4];

#[derive(Copy, Clone, Debug, PartialEq)]
pub struct Camera2d {
    pub center_x_mm: f32,
    pub center_y_mm: f32,
    pub zoom: f32,
}

impl Camera2d {
    #[must_use]
    pub fn fit(battlefield: FlatBattlefield) -> Self {
        Self {
            center_x_mm: battlefield.width_mm as f32 / 2.0,
            center_y_mm: battlefield.depth_mm as f32 / 2.0,
            zoom: 1.0,
        }
    }

    fn sanitized_zoom(self) -> f32 {
        if self.zoom.is_finite() && self.zoom >= MIN_CAMERA_ZOOM {
            self.zoom
        } else {
            1.0
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct RenderViewState {
    pub camera: Camera2d,
    selected_units: BTreeSet<String>,
    order_preview_units: BTreeSet<String>,
}

impl RenderViewState {
    #[must_use]
    pub fn fit(battlefield: FlatBattlefield) -> Self {
        Self {
            camera: Camera2d::fit(battlefield),
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
    clip_center: [f32; 2],
    clip_half_extent: [f32; 2],
}

#[derive(Clone, Debug, PartialEq)]
pub struct BattleRenderSnapshot {
    pub tick: u64,
    pub battlefield: FlatBattlefield,
    pub camera: Camera2d,
    pub units: Vec<RenderUnitInstance>,
}

impl BattleRenderSnapshot {
    #[must_use]
    pub fn project(battle: &TacticalBattle, view: &RenderViewState) -> Self {
        let battlefield = battle.battlefield();
        let units = battle
            .units()
            .iter()
            .filter(|unit| !unit.is_destroyed())
            .map(|unit| {
                let frontage_slots = unit.frontage_slots();
                let ranks = u32::from(unit.soldiers())
                    .div_ceil(u32::from(frontage_slots.max(1)))
                    .max(1);
                let width_mm = f32::from(frontage_slots.max(1)) * SOLDIER_SPACING_MM;
                let depth_mm = ranks as f32 * SOLDIER_SPACING_MM;
                let (clip_center, clip_half_extent) = project_rect(
                    unit.position(),
                    width_mm,
                    depth_mm,
                    battlefield,
                    view.camera,
                );

                RenderUnitInstance {
                    unit_id: unit.id().to_owned(),
                    side: unit.side(),
                    position: unit.position(),
                    formation: unit.formation(),
                    soldiers: unit.soldiers(),
                    frontage_slots,
                    morale: unit.morale(),
                    fatigue: unit.fatigue(),
                    routed: unit.is_routed(),
                    selected: view.is_selected(unit.id()),
                    order_preview: view.has_order_preview(unit.id()),
                    clip_center,
                    clip_half_extent,
                }
            })
            .collect();

        Self {
            tick: battle.tick(),
            battlefield,
            camera: view.camera,
            units,
        }
    }

    fn gpu_instances(&self) -> Vec<GpuUnitInstance> {
        self.units
            .iter()
            .map(|unit| GpuUnitInstance {
                center_size: [
                    unit.clip_center[0],
                    unit.clip_center[1],
                    unit.clip_half_extent[0],
                    unit.clip_half_extent[1],
                ],
                visual: [
                    match unit.side {
                        BattleSide::Attacker => 0.0,
                        BattleSide::Defender => 1.0,
                    },
                    f32::from(unit.routed),
                    f32::from(unit.selected),
                    f32::from(unit.order_preview),
                ],
            })
            .collect()
    }
}

fn project_rect(
    position: BattlePoint,
    width_mm: f32,
    depth_mm: f32,
    battlefield: FlatBattlefield,
    camera: Camera2d,
) -> ([f32; 2], [f32; 2]) {
    let half_field_width = battlefield.width_mm as f32 / 2.0;
    let half_field_depth = battlefield.depth_mm as f32 / 2.0;
    let zoom = camera.sanitized_zoom();
    let center_x = (position.x_mm as f32 - camera.center_x_mm) / half_field_width * zoom;
    let center_y = (camera.center_y_mm - position.y_mm as f32) / half_field_depth * zoom;
    let half_width = width_mm / battlefield.width_mm as f32 * zoom;
    let half_depth = depth_mm / battlefield.depth_mm as f32 * zoom;
    ([center_x, center_y], [half_width, half_depth])
}

#[repr(C)]
#[derive(Copy, Clone, Debug, PartialEq, Pod, Zeroable)]
struct QuadVertex {
    position: [f32; 2],
}

const QUAD_VERTICES: [QuadVertex; QUAD_VERTEX_COUNT as usize] = [
    QuadVertex {
        position: [-1.0, -1.0],
    },
    QuadVertex {
        position: [1.0, -1.0],
    },
    QuadVertex {
        position: [1.0, 1.0],
    },
    QuadVertex {
        position: [-1.0, -1.0],
    },
    QuadVertex {
        position: [1.0, 1.0],
    },
    QuadVertex {
        position: [-1.0, 1.0],
    },
];

#[repr(C)]
#[derive(Copy, Clone, Debug, PartialEq, Pod, Zeroable)]
struct GpuUnitInstance {
    center_size: [f32; 4],
    visual: [f32; 4],
}

pub struct GpuBattleRenderer {
    pipeline: wgpu::RenderPipeline,
    vertex_buffer: wgpu::Buffer,
    instance_buffer: wgpu::Buffer,
    instance_capacity: usize,
    instance_count: u32,
}

impl GpuBattleRenderer {
    #[must_use]
    pub fn new(device: &wgpu::Device, target_format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Medieval tactical unit shader"),
            source: wgpu::ShaderSource::Wgsl(SHADER_SOURCE.into()),
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Medieval tactical pipeline layout"),
            bind_group_layouts: &[],
            immediate_size: 0,
        });
        let vertex_buffers = [
            Some(wgpu::VertexBufferLayout {
                array_stride: std::mem::size_of::<QuadVertex>() as wgpu::BufferAddress,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &VERTEX_ATTRIBUTES,
            }),
            Some(wgpu::VertexBufferLayout {
                array_stride: std::mem::size_of::<GpuUnitInstance>() as wgpu::BufferAddress,
                step_mode: wgpu::VertexStepMode::Instance,
                attributes: &INSTANCE_ATTRIBUTES,
            }),
        ];
        let color_targets = [Some(wgpu::ColorTargetState {
            format: target_format,
            blend: Some(wgpu::BlendState::ALPHA_BLENDING),
            write_mask: wgpu::ColorWrites::ALL,
        })];
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Medieval tactical unit pipeline"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &vertex_buffers,
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &color_targets,
            }),
            multiview_mask: None,
            cache: None,
        });
        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Medieval tactical quad vertices"),
            contents: bytemuck::cast_slice(&QUAD_VERTICES),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let instance_buffer = create_instance_buffer(device, INITIAL_INSTANCE_CAPACITY);

        Self {
            pipeline,
            vertex_buffer,
            instance_buffer,
            instance_capacity: INITIAL_INSTANCE_CAPACITY,
            instance_count: 0,
        }
    }

    pub fn upload_snapshot(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        snapshot: &BattleRenderSnapshot,
    ) {
        let instances = snapshot.gpu_instances();
        if instances.len() > self.instance_capacity {
            self.instance_capacity = instances.len().next_power_of_two();
            self.instance_buffer = create_instance_buffer(device, self.instance_capacity);
        }
        if !instances.is_empty() {
            queue.write_buffer(&self.instance_buffer, 0, bytemuck::cast_slice(&instances));
        }
        self.instance_count = u32::try_from(instances.len()).expect("unit instance count fits in u32");
    }

    pub fn render(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        clear_color: wgpu::Color,
    ) {
        let color_attachments = [Some(wgpu::RenderPassColorAttachment {
            view: target,
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(clear_color),
                store: wgpu::StoreOp::Store,
            },
        })];
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Medieval tactical render pass"),
            color_attachments: &color_attachments,
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(&self.pipeline);
        pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        pass.set_vertex_buffer(1, self.instance_buffer.slice(..));
        pass.draw(0..QUAD_VERTEX_COUNT, 0..self.instance_count);
    }

    #[must_use]
    pub const fn instance_count(&self) -> u32 {
        self.instance_count
    }
}

fn create_instance_buffer(device: &wgpu::Device, capacity: usize) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("Medieval tactical unit instances"),
        size: (capacity.max(1) * std::mem::size_of::<GpuUnitInstance>()) as wgpu::BufferAddress,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use medieval_core::{TACTICAL_TICKS_PER_SECOND, TacticalUnit};

    fn unit(
        id: &str,
        side: BattleSide,
        soldiers: u16,
        x_mm: u32,
        formation: Formation,
    ) -> TacticalUnit {
        TacticalUnit::new(
            id,
            side,
            soldiers,
            BattlePoint::new(x_mm, 50_000),
            formation,
            1_000,
        )
    }

    fn sample_battle() -> TacticalBattle {
        TacticalBattle::new(
            FlatBattlefield::new(100_000, 100_000),
            vec![
                unit(
                    "defender",
                    BattleSide::Defender,
                    80,
                    70_000,
                    Formation::Column { files: 16 },
                ),
                unit(
                    "attacker",
                    BattleSide::Attacker,
                    80,
                    30_000,
                    Formation::Line { files: 20 },
                ),
            ],
        )
        .unwrap()
    }

    #[test]
    fn identical_battle_and_view_produce_identical_instance_streams() {
        let battle = sample_battle();
        let view = RenderViewState::fit(battle.battlefield())
            .with_selected("attacker")
            .with_order_preview("attacker");

        let first = BattleRenderSnapshot::project(&battle, &view);
        let second = BattleRenderSnapshot::project(&battle, &view);

        assert_eq!(first, second);
        assert_eq!(first.gpu_instances(), second.gpu_instances());
        assert_eq!(
            first
                .units
                .iter()
                .map(|unit| unit.unit_id.as_str())
                .collect::<Vec<_>>(),
            vec!["attacker", "defender"]
        );
    }

    #[test]
    fn projection_carries_authoritative_metadata_without_mutating_battle() {
        let battle = sample_battle();
        let before = battle.clone();
        let view = RenderViewState::fit(battle.battlefield()).with_selected("attacker");

        let snapshot = BattleRenderSnapshot::project(&battle, &view);
        let attacker = snapshot
            .units
            .iter()
            .find(|unit| unit.unit_id == "attacker")
            .unwrap();

        assert_eq!(battle, before);
        assert_eq!(attacker.side, BattleSide::Attacker);
        assert_eq!(attacker.formation, Formation::Line { files: 20 });
        assert_eq!(attacker.frontage_slots, 20);
        assert_eq!(attacker.morale, 1_000);
        assert_eq!(attacker.fatigue, 0);
        assert!(attacker.selected);
        assert!(!attacker.routed);
    }

    #[test]
    fn camera_and_selection_are_projection_only() {
        let battle = sample_battle();
        let before = battle.clone();
        let fit = RenderViewState::fit(battle.battlefield());
        let mut moved = fit.clone().with_selected("defender");
        moved.camera.center_x_mm += 5_000.0;
        moved.camera.zoom = 2.0;

        let fit_snapshot = BattleRenderSnapshot::project(&battle, &fit);
        let moved_snapshot = BattleRenderSnapshot::project(&battle, &moved);

        assert_eq!(battle, before);
        assert_ne!(fit_snapshot.units[0].clip_center, moved_snapshot.units[0].clip_center);
        assert!(!fit_snapshot.units[1].selected);
        assert!(moved_snapshot.units[1].selected);
    }

    #[test]
    fn destroyed_units_leave_the_active_instance_batch() {
        let mut battle = TacticalBattle::new(
            FlatBattlefield::new(20_000, 20_000),
            vec![
                unit(
                    "attacker",
                    BattleSide::Attacker,
                    80,
                    9_500,
                    Formation::Line { files: 40 },
                ),
                unit(
                    "defender",
                    BattleSide::Defender,
                    1,
                    10_500,
                    Formation::Column { files: 1 },
                ),
            ],
        )
        .unwrap();
        battle
            .issue_engagement_order("attacker", "defender")
            .unwrap();
        battle.advance_ticks(TACTICAL_TICKS_PER_SECOND);
        assert!(battle.units().iter().any(|unit| {
            unit.id() == "defender" && unit.is_destroyed()
        }));

        let snapshot = BattleRenderSnapshot::project(
            &battle,
            &RenderViewState::fit(battle.battlefield()),
        );

        assert_eq!(snapshot.units.len(), 1);
        assert_eq!(snapshot.units[0].unit_id, "attacker");
        assert_eq!(snapshot.gpu_instances().len(), 1);
    }

    #[test]
    fn routed_state_is_reflected_in_render_metadata() {
        let mut battle = TacticalBattle::new(
            FlatBattlefield::new(30_000, 30_000),
            vec![
                unit(
                    "attacker",
                    BattleSide::Attacker,
                    80,
                    14_500,
                    Formation::Line { files: 60 },
                ),
                unit(
                    "defender",
                    BattleSide::Defender,
                    80,
                    15_500,
                    Formation::Column { files: 4 },
                ),
            ],
        )
        .unwrap();
        battle
            .issue_engagement_order("attacker", "defender")
            .unwrap();
        for _ in 0..60 {
            battle.advance_ticks(TACTICAL_TICKS_PER_SECOND);
            if battle
                .units()
                .iter()
                .any(|unit| unit.id() == "defender" && unit.is_routed())
            {
                break;
            }
        }

        let defender = battle
            .units()
            .iter()
            .find(|unit| unit.id() == "defender")
            .unwrap();
        assert!(defender.is_routed() || defender.is_destroyed());
        let snapshot = BattleRenderSnapshot::project(
            &battle,
            &RenderViewState::fit(battle.battlefield()),
        );
        if !defender.is_destroyed() {
            let rendered = snapshot
                .units
                .iter()
                .find(|unit| unit.unit_id == "defender")
                .unwrap();
            assert!(rendered.routed);
        }
    }

    #[test]
    fn projected_rectangles_are_finite_and_scale_with_zoom() {
        let battle = sample_battle();
        let fit = RenderViewState::fit(battle.battlefield());
        let mut zoomed = fit.clone();
        zoomed.camera.zoom = 2.0;
        let fit_snapshot = BattleRenderSnapshot::project(&battle, &fit);
        let zoomed_snapshot = BattleRenderSnapshot::project(&battle, &zoomed);

        for unit in &fit_snapshot.units {
            assert!(unit.clip_center.into_iter().all(f32::is_finite));
            assert!(unit.clip_half_extent.into_iter().all(f32::is_finite));
            assert!(unit.clip_half_extent.into_iter().all(|value| value > 0.0));
        }
        assert!(
            zoomed_snapshot.units[0].clip_half_extent[0]
                > fit_snapshot.units[0].clip_half_extent[0]
        );
    }

    #[test]
    fn shader_contract_contains_instanced_vertex_and_fragment_entries() {
        assert!(SHADER_SOURCE.contains("@vertex"));
        assert!(SHADER_SOURCE.contains("fn vs_main"));
        assert!(SHADER_SOURCE.contains("@fragment"));
        assert!(SHADER_SOURCE.contains("fn fs_main"));
        assert_eq!(QUAD_VERTICES.len(), QUAD_VERTEX_COUNT as usize);
    }
}
