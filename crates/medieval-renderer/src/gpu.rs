use std::cell::RefCell;

use bytemuck::{Pod, Zeroable};
use medieval_core::{BattleSide, FlatBattlefield};
use wgpu::util::DeviceExt;

use crate::BattleRenderSnapshot;

const SOLDIER_HALF_WIDTH_MM: f32 = 250.0;
const SOLDIER_HALF_HEIGHT_MM: f32 = 900.0;
const SOLDIER_HALF_DEPTH_MM: f32 = 250.0;
const GROUND_HALF_HEIGHT_MM: f32 = 100.0;
const OBLIQUE_ELEVATION_SCALE: f32 = 0.72;
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
    fn ground(battlefield: FlatBattlefield) -> Self {
        Self {
            center_material: [
                battlefield.width_mm as f32 / 2.0,
                -GROUND_HALF_HEIGHT_MM,
                battlefield.depth_mm as f32 / 2.0,
                2.0,
            ],
            half_extent_routed: [
                battlefield.width_mm as f32 / 2.0,
                GROUND_HALF_HEIGHT_MM,
                battlefield.depth_mm as f32 / 2.0,
                0.0,
            ],
            visual: [0.0; 4],
        }
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
    center_zoom_elevation: [f32; 4],
    battlefield: [f32; 4],
}

impl CameraUniform {
    fn from_snapshot(snapshot: &BattleRenderSnapshot) -> Self {
        Self {
            center_zoom_elevation: [
                snapshot.camera.center_x_mm,
                snapshot.camera.center_y_mm,
                snapshot.camera.sanitized_zoom(),
                OBLIQUE_ELEVATION_SCALE,
            ],
            battlefield: [
                snapshot.battlefield.width_mm as f32 / 2.0,
                snapshot.battlefield.depth_mm as f32 / 2.0,
                snapshot.battlefield.depth_mm as f32,
                0.0,
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
    pipeline: wgpu::RenderPipeline,
    camera_buffer: wgpu::Buffer,
    camera_bind_group: wgpu::BindGroup,
    instance_buffer: wgpu::Buffer,
    instance_capacity: usize,
    instance_count: u32,
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
        let camera = CameraUniform {
            center_zoom_elevation: [0.0, 0.0, 1.0, OBLIQUE_ELEVATION_SCALE],
            battlefield: [1.0, 1.0, 2.0, 0.0],
        };
        let camera_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Medieval 3D tactical camera uniform"),
            contents: bytemuck::bytes_of(&camera),
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
            pipeline,
            camera_buffer,
            camera_bind_group,
            instance_buffer,
            instance_capacity: INITIAL_INSTANCE_CAPACITY,
            instance_count: 0,
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
        queue.write_buffer(
            &self.camera_buffer,
            0,
            bytemuck::bytes_of(&CameraUniform::from_snapshot(snapshot)),
        );
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
    let mut instances = Vec::with_capacity(1 + soldier_count);
    instances.push(GpuWorldInstance::ground(snapshot.battlefield));
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
    fn gpu_batch_contains_ground_and_individual_soldiers() {
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
        assert_eq!(gpu_instances(&snapshot).len(), 81);
    }

    #[test]
    fn shader_consumes_world_geometry() {
        assert!(SHADER_SOURCE.contains("vertex_index"));
        assert!(SHADER_SOURCE.contains("world_position"));
        assert!(SHADER_SOURCE.contains("cube_normal"));
    }
}
