use std::cell::RefCell;

use bytemuck::{Pod, Zeroable};
use medieval_core::{
    BattlePoint, BattleSide, DeploymentZone, FlatBattlefield, TacticalTerrainCell,
};
use wgpu::util::DeviceExt;

use crate::{
    BattleRenderSnapshot, Camera3d,
    terrain::{
        TERRAIN_GRID_SIZE, terrain_cell_bounds_mm, terrain_cell_height_mm, terrain_height_mm,
    },
};

const SOLDIER_HALF_WIDTH_MM: f32 = 250.0;
const SOLDIER_HALF_HEIGHT_MM: f32 = 900.0;
const SOLDIER_HALF_DEPTH_MM: f32 = 250.0;
const GROUND_BASE_DEPTH_MM: f32 = 200.0;
const DEPLOYMENT_BOUNDARY_HALF_WIDTH_MM: f32 = 180.0;
const DEPLOYMENT_BOUNDARY_HEIGHT_MM: f32 = 40.0;
const FOREST_TREES_PER_CELL: u32 = 4;
const RIVER_SURFACE_HEIGHT_MM: f32 = 30.0;
const CROSSING_SURFACE_HEIGHT_MM: f32 = 90.0;
const CUBE_VERTEX_COUNT: u32 = 36;
const INITIAL_INSTANCE_CAPACITY: usize = 256;
const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth24Plus;
const SHADER_SOURCE: &str = include_str!("battlefield.wgsl");
const INSTANCE_ATTRIBUTES: [wgpu::VertexAttribute; 3] =
    wgpu::vertex_attr_array![0 => Float32x4, 1 => Float32x4, 2 => Float32x4];

#[repr(C)]
#[derive(Copy, Clone, Debug, PartialEq, Pod, Zeroable)]
struct GpuWorldInstance {
    center_material: [f32; 4],
    half_extent_routed: [f32; 4],
    visual: [f32; 4],
}

impl GpuWorldInstance {
    fn terrain_cell(battlefield: FlatBattlefield, cell_x: u32, cell_z: u32) -> Option<Self> {
        let (x0, x1, z0, z1) = terrain_cell_bounds_mm(battlefield, cell_x, cell_z)?;
        if x1 <= x0 || z1 <= z0 {
            return None;
        }
        let top = terrain_cell_height_mm(battlefield, cell_x, cell_z) as f32;
        let bottom = -GROUND_BASE_DEPTH_MM;
        let half_height = (top - bottom) / 2.0;
        Some(Self {
            center_material: [
                (x0 as f32 + x1 as f32) / 2.0,
                bottom + half_height,
                (z0 as f32 + z1 as f32) / 2.0,
                2.0,
            ],
            half_extent_routed: [
                (x1 - x0) as f32 / 2.0,
                half_height,
                (z1 - z0) as f32 / 2.0,
                0.0,
            ],
            visual: [0.0; 4],
        })
    }

    fn deployment_boundary_segment(
        zone: DeploymentZone,
        battlefield: FlatBattlefield,
        cell_z: u32,
    ) -> Option<Self> {
        let (_, _, z0, z1) = terrain_cell_bounds_mm(battlefield, 0, cell_z)?;
        if z1 <= z0 {
            return None;
        }
        let boundary_x = match zone.side {
            BattleSide::Attacker => zone.max_x_mm,
            BattleSide::Defender => zone.min_x_mm,
        };
        let center_z = z0 + (z1 - z0) / 2;
        let terrain_y =
            terrain_height_mm(battlefield, BattlePoint::new(boundary_x, center_z)) as f32;
        let half_height = DEPLOYMENT_BOUNDARY_HEIGHT_MM / 2.0;
        Some(Self {
            center_material: [
                boundary_x as f32,
                terrain_y + half_height,
                (z0 as f32 + z1 as f32) / 2.0,
                match zone.side {
                    BattleSide::Attacker => 3.0,
                    BattleSide::Defender => 4.0,
                },
            ],
            half_extent_routed: [
                DEPLOYMENT_BOUNDARY_HALF_WIDTH_MM,
                half_height,
                (z1 - z0) as f32 / 2.0,
                0.0,
            ],
            visual: [0.0; 4],
        })
    }

    fn forest_tree(
        cell: TacticalTerrainCell,
        battlefield: FlatBattlefield,
        tree_index: u32,
    ) -> Option<Self> {
        if tree_index >= FOREST_TREES_PER_CELL {
            return None;
        }
        let (x0, x1, z0, z1) = terrain_cell_bounds_mm(battlefield, cell.cell_x, cell.cell_z)?;
        if x1 <= x0 || z1 <= z0 {
            return None;
        }
        let slot_x = tree_index % 2;
        let slot_z = tree_index / 2;
        let tree_x = x0
            + u32::try_from(u64::from(x1 - x0) * u64::from(slot_x * 2 + 1) / 4)
                .expect("forest tree X offset fits in u32");
        let tree_z = z0
            + u32::try_from(u64::from(z1 - z0) * u64::from(slot_z * 2 + 1) / 4)
                .expect("forest tree Z offset fits in u32");
        let terrain_y = terrain_height_mm(battlefield, BattlePoint::new(tree_x, tree_z)) as f32;
        let minimum_span = (x1 - x0).min(z1 - z0) as f32;
        let half_width = (minimum_span * 0.06).clamp(120.0, 700.0);
        let half_height = (half_width * 3.0).clamp(700.0, 2_600.0);
        Some(Self {
            center_material: [tree_x as f32, terrain_y + half_height, tree_z as f32, 5.0],
            half_extent_routed: [half_width, half_height, half_width, 0.0],
            visual: [0.0; 4],
        })
    }

    fn river_cell(
        cell: TacticalTerrainCell,
        battlefield: FlatBattlefield,
        crossing: bool,
    ) -> Option<Self> {
        let (x0, x1, z0, z1) = terrain_cell_bounds_mm(battlefield, cell.cell_x, cell.cell_z)?;
        if x1 <= x0 || z1 <= z0 {
            return None;
        }
        let terrain_y = terrain_cell_height_mm(battlefield, cell.cell_x, cell.cell_z) as f32;
        let height = if crossing {
            CROSSING_SURFACE_HEIGHT_MM
        } else {
            RIVER_SURFACE_HEIGHT_MM
        };
        let half_height = height / 2.0;
        Some(Self {
            center_material: [
                (x0 as f32 + x1 as f32) / 2.0,
                terrain_y + half_height,
                (z0 as f32 + z1 as f32) / 2.0,
                if crossing { 7.0 } else { 6.0 },
            ],
            half_extent_routed: [
                (x1 - x0) as f32 / 2.0,
                half_height,
                (z1 - z0) as f32 / 2.0,
                0.0,
            ],
            visual: [0.0; 4],
        })
    }

    fn soldier(
        center: [f32; 3],
        side: BattleSide,
        routed: bool,
        selected: bool,
        order_preview: bool,
    ) -> Self {
        Self {
            center_material: [
                center[0],
                center[1],
                center[2],
                match side {
                    BattleSide::Attacker => 0.0,
                    BattleSide::Defender => 1.0,
                },
            ],
            half_extent_routed: [
                SOLDIER_HALF_WIDTH_MM,
                SOLDIER_HALF_HEIGHT_MM,
                SOLDIER_HALF_DEPTH_MM,
                flag(routed),
            ],
            visual: [flag(selected), flag(order_preview), 0.0, 0.0],
        }
    }
}

const fn flag(value: bool) -> f32 {
    if value { 1.0 } else { 0.0 }
}

#[repr(C)]
#[derive(Copy, Clone, Debug, PartialEq, Pod, Zeroable)]
struct CameraUniform {
    eye_near: [f32; 4],
    right_tan_half_fov: [f32; 4],
    up_aspect: [f32; 4],
    forward_far: [f32; 4],
}

impl CameraUniform {
    fn from_camera(
        camera: Camera3d,
        battlefield: FlatBattlefield,
        viewport_width: u32,
        viewport_height: u32,
    ) -> Self {
        let projection = camera.projection(battlefield);
        let aspect = viewport_width.max(1) as f32 / viewport_height.max(1) as f32;
        Self {
            eye_near: [
                projection.eye_mm[0],
                projection.eye_mm[1],
                projection.eye_mm[2],
                projection.near_mm,
            ],
            right_tan_half_fov: [
                projection.right[0],
                projection.right[1],
                projection.right[2],
                projection.tan_half_fov_y,
            ],
            up_aspect: [projection.up[0], projection.up[1], projection.up[2], aspect],
            forward_far: [
                projection.forward[0],
                projection.forward[1],
                projection.forward[2],
                projection.far_mm,
            ],
        }
    }
}

struct DepthTarget {
    width: u32,
    height: u32,
    _texture: wgpu::Texture,
    view: wgpu::TextureView,
}

pub struct GpuBattleRenderer {
    device: wgpu::Device,
    queue: Option<wgpu::Queue>,
    pipeline: wgpu::RenderPipeline,
    camera_buffer: wgpu::Buffer,
    camera_bind_group: wgpu::BindGroup,
    instance_buffer: wgpu::Buffer,
    instance_capacity: usize,
    instance_count: u32,
    camera: Camera3d,
    battlefield: FlatBattlefield,
    depth_target: RefCell<Option<DepthTarget>>,
}

impl GpuBattleRenderer {
    #[must_use]
    pub fn new(device: &wgpu::Device, target_format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Medieval 3D tactical battlefield shader"),
            source: wgpu::ShaderSource::Wgsl(SHADER_SOURCE.into()),
        });
        let camera_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Medieval 3D tactical camera layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Medieval 3D tactical pipeline layout"),
            bind_group_layouts: &[Some(&camera_layout)],
            immediate_size: 0,
        });
        let battlefield = FlatBattlefield::new(1, 1);
        let camera = Camera3d::fit(battlefield);
        let camera_uniform = CameraUniform::from_camera(camera, battlefield, 1, 1);
        let camera_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Medieval 3D tactical camera uniform"),
            contents: bytemuck::bytes_of(&camera_uniform),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let camera_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Medieval 3D tactical camera bind group"),
            layout: &camera_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: camera_buffer.as_entire_binding(),
            }],
        });
        let instance_buffer = create_instance_buffer(device, INITIAL_INSTANCE_CAPACITY);
        let pipeline = create_pipeline(device, target_format, &pipeline_layout, &shader);
        Self {
            device: device.clone(),
            queue: None,
            pipeline,
            camera_buffer,
            camera_bind_group,
            instance_buffer,
            instance_capacity: INITIAL_INSTANCE_CAPACITY,
            instance_count: 0,
            camera,
            battlefield,
            depth_target: RefCell::new(None),
        }
    }

    pub fn upload_snapshot(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        snapshot: &BattleRenderSnapshot,
    ) {
        let instances = gpu_instances(snapshot);
        if instances.len() > self.instance_capacity {
            self.instance_capacity = instances.len().next_power_of_two();
            self.instance_buffer = create_instance_buffer(device, self.instance_capacity);
        }
        if !instances.is_empty() {
            queue.write_buffer(&self.instance_buffer, 0, bytemuck::cast_slice(&instances));
        }
        self.instance_count =
            u32::try_from(instances.len()).expect("3D tactical instance count fits in u32");
        self.camera = snapshot.camera;
        self.battlefield = snapshot.battlefield;
        self.queue = Some(queue.clone());
    }

    pub fn render(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        clear_color: wgpu::Color,
    ) {
        let texture = target.texture();
        let width = texture.width().max(1);
        let height = texture.height().max(1);
        self.queue
            .as_ref()
            .expect("a tactical snapshot is uploaded before rendering")
            .write_buffer(
                &self.camera_buffer,
                0,
                bytemuck::bytes_of(&CameraUniform::from_camera(
                    self.camera,
                    self.battlefield,
                    width,
                    height,
                )),
            );

        let mut depth_target = self.depth_target.borrow_mut();
        if depth_target
            .as_ref()
            .is_none_or(|depth| depth.width != width || depth.height != height)
        {
            *depth_target = Some(create_depth_target(&self.device, width, height));
        }
        let depth_view = &depth_target
            .as_ref()
            .expect("depth target exists before the render pass")
            .view;
        let color_attachments = [Some(wgpu::RenderPassColorAttachment {
            view: target,
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(clear_color),
                store: wgpu::StoreOp::Store,
            },
        })];
        let depth_stencil_attachment = Some(wgpu::RenderPassDepthStencilAttachment {
            view: depth_view,
            depth_ops: Some(wgpu::Operations {
                load: wgpu::LoadOp::Clear(1.0),
                store: wgpu::StoreOp::Store,
            }),
            stencil_ops: None,
        });
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Medieval 3D tactical render pass"),
            color_attachments: &color_attachments,
            depth_stencil_attachment,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.camera_bind_group, &[]);
        pass.set_vertex_buffer(0, self.instance_buffer.slice(..));
        pass.draw(0..CUBE_VERTEX_COUNT, 0..self.instance_count);
    }

    #[must_use]
    pub const fn instance_count(&self) -> u32 {
        self.instance_count
    }
}

fn gpu_instances(snapshot: &BattleRenderSnapshot) -> Vec<GpuWorldInstance> {
    let soldier_count = snapshot
        .units
        .iter()
        .map(|unit| unit.soldier_centers_mm.len())
        .sum::<usize>();
    let terrain_capacity = (TERRAIN_GRID_SIZE * TERRAIN_GRID_SIZE) as usize;
    let deployment_capacity = (2 * TERRAIN_GRID_SIZE) as usize;
    let forest_capacity = snapshot.forest_cells.len() * FOREST_TREES_PER_CELL as usize;
    let river_capacity = snapshot.river_cells.len();
    let mut instances = Vec::with_capacity(
        terrain_capacity
            + deployment_capacity
            + forest_capacity
            + river_capacity
            + soldier_count,
    );
    for cell_z in 0..TERRAIN_GRID_SIZE {
        for cell_x in 0..TERRAIN_GRID_SIZE {
            if let Some(cell) = GpuWorldInstance::terrain_cell(snapshot.battlefield, cell_x, cell_z)
            {
                instances.push(cell);
            }
        }
    }
    for cell in &snapshot.river_cells {
        let crossing = snapshot.river_crossing_cells.contains(cell);
        if let Some(river) = GpuWorldInstance::river_cell(*cell, snapshot.battlefield, crossing) {
            instances.push(river);
        }
    }
    for cell in &snapshot.forest_cells {
        for tree_index in 0..FOREST_TREES_PER_CELL {
            if let Some(tree) =
                GpuWorldInstance::forest_tree(*cell, snapshot.battlefield, tree_index)
            {
                instances.push(tree);
            }
        }
    }
    for zone in snapshot.deployment_zones {
        for cell_z in 0..TERRAIN_GRID_SIZE {
            if let Some(marker) =
                GpuWorldInstance::deployment_boundary_segment(zone, snapshot.battlefield, cell_z)
            {
                instances.push(marker);
            }
        }
    }
    for unit in &snapshot.units {
        instances.extend(unit.soldier_centers_mm.iter().map(|center| {
            GpuWorldInstance::soldier(
                *center,
                unit.side,
                unit.routed,
                unit.selected,
                unit.order_preview,
            )
        }));
    }
    instances
}

fn create_pipeline(
    device: &wgpu::Device,
    target_format: wgpu::TextureFormat,
    layout: &wgpu::PipelineLayout,
    shader: &wgpu::ShaderModule,
) -> wgpu::RenderPipeline {
    let vertex_buffers = [Some(wgpu::VertexBufferLayout {
        array_stride: std::mem::size_of::<GpuWorldInstance>() as wgpu::BufferAddress,
        step_mode: wgpu::VertexStepMode::Instance,
        attributes: &INSTANCE_ATTRIBUTES,
    })];
    let color_targets = [Some(wgpu::ColorTargetState {
        format: target_format,
        blend: Some(wgpu::BlendState::ALPHA_BLENDING),
        write_mask: wgpu::ColorWrites::ALL,
    })];
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("Medieval 3D tactical world pipeline"),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some("vs_main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            buffers: &vertex_buffers,
        },
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: Some(wgpu::DepthStencilState {
            format: DEPTH_FORMAT,
            depth_write_enabled: Some(true),
            depth_compare: Some(wgpu::CompareFunction::LessEqual),
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        }),
        multisample: wgpu::MultisampleState::default(),
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some("fs_main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            targets: &color_targets,
        }),
        multiview_mask: None,
        cache: None,
    })
}

fn create_instance_buffer(device: &wgpu::Device, capacity: usize) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("Medieval 3D tactical instances"),
        size: (capacity.max(1) * std::mem::size_of::<GpuWorldInstance>()) as wgpu::BufferAddress,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

fn create_depth_target(device: &wgpu::Device, width: u32, height: u32) -> DepthTarget {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Medieval 3D tactical depth texture"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: DEPTH_FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    DepthTarget {
        width,
        height,
        _texture: texture,
        view,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{BattleRenderSnapshot, RenderViewState};
    use medieval_core::{BattlePoint, FlatBattlefield, Formation, TacticalBattle, TacticalUnit};

    #[test]
    fn gpu_batch_contains_terrain_cells_river_and_individual_soldiers() {
        let battle = TacticalBattle::new(
            FlatBattlefield::new(100_000, 100_000),
            vec![TacticalUnit::new(
                "attacker",
                BattleSide::Attacker,
                80,
                BattlePoint::new(30_000, 50_000),
                Formation::Line { files: 20 },
                1_000,
            )],
        )
        .unwrap();
        let snapshot =
            BattleRenderSnapshot::capture(&battle, &RenderViewState::fit(battle.battlefield()));
        assert_eq!(
            gpu_instances(&snapshot).len(),
            (TERRAIN_GRID_SIZE * TERRAIN_GRID_SIZE + 2 * TERRAIN_GRID_SIZE) as usize
                + snapshot.forest_cells.len() * FOREST_TREES_PER_CELL as usize
                + snapshot.river_cells.len()
                + 80
        );
    }

    #[test]
    fn deployment_boundaries_are_projected_from_core_zones() {
        let battlefield = FlatBattlefield::new(100_000, 100_000);
        let zones = medieval_core::standard_deployment_zones(battlefield);
        let attacker =
            GpuWorldInstance::deployment_boundary_segment(zones[0], battlefield, 0).unwrap();
        let defender =
            GpuWorldInstance::deployment_boundary_segment(zones[1], battlefield, 0).unwrap();
        assert_eq!(attacker.center_material[0], zones[0].max_x_mm as f32);
        assert_eq!(defender.center_material[0], zones[1].min_x_mm as f32);
        assert_eq!(attacker.center_material[3], 3.0);
        assert_eq!(defender.center_material[3], 4.0);
    }

    #[test]
    fn forest_instances_are_projected_from_core_cells() {
        let battlefield = FlatBattlefield::new(100_000, 100_000);
        let battle = TacticalBattle::new(
            battlefield,
            vec![TacticalUnit::new(
                "attacker",
                BattleSide::Attacker,
                1,
                BattlePoint::new(10_000, 10_000),
                Formation::Line { files: 1 },
                1_000,
            )],
        )
        .unwrap();
        let snapshot = BattleRenderSnapshot::capture(&battle, &RenderViewState::fit(battlefield));
        let cell = snapshot.forest_cells[0];
        let tree = GpuWorldInstance::forest_tree(cell, battlefield, 0).unwrap();
        let (x0, x1, z0, z1) =
            terrain_cell_bounds_mm(battlefield, cell.cell_x, cell.cell_z).unwrap();
        assert_eq!(tree.center_material[3], 5.0);
        assert!(tree.center_material[0] > x0 as f32 && tree.center_material[0] < x1 as f32);
        assert!(tree.center_material[2] > z0 as f32 && tree.center_material[2] < z1 as f32);
        assert!(tree.half_extent_routed[1] > tree.half_extent_routed[0]);
    }

    #[test]
    fn river_instances_distinguish_blocked_water_from_crossings() {
        let battlefield = FlatBattlefield::new(100_000, 100_000);
        let battle = TacticalBattle::new(
            battlefield,
            vec![TacticalUnit::new(
                "attacker",
                BattleSide::Attacker,
                1,
                BattlePoint::new(10_000, 10_000),
                Formation::Line { files: 1 },
                1_000,
            )],
        )
        .unwrap();
        let snapshot = BattleRenderSnapshot::capture(&battle, &RenderViewState::fit(battlefield));
        let blocked = snapshot
            .river_cells
            .iter()
            .copied()
            .find(|cell| !snapshot.river_crossing_cells.contains(cell))
            .unwrap();
        let crossing = snapshot.river_crossing_cells[0];
        assert_eq!(
            GpuWorldInstance::river_cell(blocked, battlefield, false)
                .unwrap()
                .center_material[3],
            6.0
        );
        assert_eq!(
            GpuWorldInstance::river_cell(crossing, battlefield, true)
                .unwrap()
                .center_material[3],
            7.0
        );
    }

    #[test]
    fn terrain_instances_raise_center_cells_above_edge_cells() {
        let battlefield = FlatBattlefield::new(100_000, 100_000);
        let edge = GpuWorldInstance::terrain_cell(battlefield, 0, 0).unwrap();
        let center = GpuWorldInstance::terrain_cell(battlefield, 4, 4).unwrap();
        let edge_top = edge.center_material[1] + edge.half_extent_routed[1];
        let center_top = center.center_material[1] + center.half_extent_routed[1];
        assert!(center_top > edge_top);
    }

    #[test]
    fn shader_consumes_world_geometry_and_perspective_camera_basis() {
        assert!(SHADER_SOURCE.contains("vertex_index"));
        assert!(SHADER_SOURCE.contains("world_position"));
        assert!(SHADER_SOURCE.contains("cube_normal"));
        assert!(SHADER_SOURCE.contains("view_z"));
        assert!(SHADER_SOURCE.contains("eye_near"));
        assert!(!SHADER_SOURCE.contains("center_zoom_elevation"));
    }
}
