struct CameraUniform {
    center_zoom_elevation: vec4<f32>,
    battlefield: vec4<f32>,
};

@group(0) @binding(0)
var<uniform> camera: CameraUniform;

struct VertexInput {
    @builtin(vertex_index) vertex_index: u32,
    @location(0) center_material: vec4<f32>,
    @location(1) half_extent_routed: vec4<f32>,
    @location(2) visual: vec4<f32>,
};

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) normal: vec3<f32>,
    @location(1) material: f32,
    @location(2) routed: f32,
    @location(3) selected: f32,
    @location(4) order_preview: f32,
};

fn cube_vertex(index: u32) -> vec3<f32> {
    let triangle = array<vec2<f32>, 6>(
        vec2<f32>(-1.0, -1.0), vec2<f32>(1.0, -1.0), vec2<f32>(1.0, 1.0),
        vec2<f32>(-1.0, -1.0), vec2<f32>(1.0, 1.0), vec2<f32>(-1.0, 1.0),
    );
    let uv = triangle[index % 6u];
    switch index / 6u {
        case 0u: { return vec3<f32>(uv.x, uv.y, 1.0); }
        case 1u: { return vec3<f32>(-uv.x, uv.y, -1.0); }
        case 2u: { return vec3<f32>(-1.0, uv.y, -uv.x); }
        case 3u: { return vec3<f32>(1.0, uv.y, uv.x); }
        case 4u: { return vec3<f32>(uv.x, 1.0, -uv.y); }
        default: { return vec3<f32>(uv.x, -1.0, uv.y); }
    }
}

fn cube_normal(index: u32) -> vec3<f32> {
    switch index / 6u {
        case 0u: { return vec3<f32>(0.0, 0.0, 1.0); }
        case 1u: { return vec3<f32>(0.0, 0.0, -1.0); }
        case 2u: { return vec3<f32>(-1.0, 0.0, 0.0); }
        case 3u: { return vec3<f32>(1.0, 0.0, 0.0); }
        case 4u: { return vec3<f32>(0.0, 1.0, 0.0); }
        default: { return vec3<f32>(0.0, -1.0, 0.0); }
    }
}

@vertex
fn vs_main(input: VertexInput) -> VertexOutput {
    let world_position = input.center_material.xyz
        + cube_vertex(input.vertex_index) * input.half_extent_routed.xyz;
    let half_width = max(camera.battlefield.x, 1.0);
    let half_depth = max(camera.battlefield.y, 1.0);
    let full_depth = max(camera.battlefield.z, 1.0);
    let zoom = camera.center_zoom_elevation.z;
    let elevation_lift = world_position.y * camera.center_zoom_elevation.w;
    let clip_x = (world_position.x - camera.center_zoom_elevation.x) / half_width * zoom;
    let clip_y = (camera.center_zoom_elevation.y - world_position.z + elevation_lift)
        / half_depth
        * zoom;
    let depth = clamp((world_position.z - world_position.y * 0.35) / full_depth, 0.0, 1.0);

    var output: VertexOutput;
    output.position = vec4<f32>(clip_x, clip_y, depth, 1.0);
    output.normal = cube_normal(input.vertex_index);
    output.material = input.center_material.w;
    output.routed = input.half_extent_routed.w;
    output.selected = input.visual.x;
    output.order_preview = input.visual.y;
    return output;
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    var color = vec3<f32>(0.69, 0.18, 0.12);
    if input.material > 1.5 {
        color = vec3<f32>(0.22, 0.29, 0.15);
    } else if input.material > 0.5 {
        color = vec3<f32>(0.12, 0.31, 0.67);
    }
    if input.routed > 0.5 {
        color = color * 0.42 + vec3<f32>(0.28, 0.28, 0.28);
    }
    if input.selected > 0.5 {
        color = color * 0.45 + vec3<f32>(0.95, 0.78, 0.18) * 0.55;
    }
    if input.order_preview > 0.5 {
        color = color * 0.72 + vec3<f32>(0.24, 0.24, 0.24);
    }
    let light_direction = normalize(vec3<f32>(0.35, 0.82, 0.45));
    let diffuse = max(dot(normalize(input.normal), light_direction), 0.0);
    return vec4<f32>(color * (0.36 + diffuse * 0.64), 1.0);
}
