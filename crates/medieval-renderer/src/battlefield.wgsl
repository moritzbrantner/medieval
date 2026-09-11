struct CameraUniform {
    eye_near: vec4<f32>,
    right_tan_half_fov: vec4<f32>,
    up_aspect: vec4<f32>,
    forward_far: vec4<f32>,
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
    let relative = world_position - camera.eye_near.xyz;
    let view_x = dot(relative, camera.right_tan_half_fov.xyz);
    let view_y = dot(relative, camera.up_aspect.xyz);
    let view_z = dot(relative, camera.forward_far.xyz);
    let base_tan_half_fov = max(camera.right_tan_half_fov.w, 0.0001);
    let aspect = max(camera.up_aspect.w, 0.0001);
    let tan_half_x = base_tan_half_fov * max(aspect, 1.0);
    let tan_half_y = base_tan_half_fov / min(aspect, 1.0);
    let near = camera.eye_near.w;
    let far = camera.forward_far.w;
    let depth_a = far / (far - near);
    let depth_b = near * far / (far - near);

    var output: VertexOutput;
    output.position = vec4<f32>(
        view_x / tan_half_x,
        view_y / tan_half_y,
        depth_a * view_z - depth_b,
        view_z,
    );
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
    var alpha = 1.0;
    if input.material > 3.5 {
        color = vec3<f32>(0.18, 0.45, 0.92);
        alpha = 0.72;
    } else if input.material > 2.5 {
        color = vec3<f32>(0.88, 0.24, 0.16);
        alpha = 0.72;
    } else if input.material > 1.5 {
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
    return vec4<f32>(color * (0.36 + diffuse * 0.64), alpha);
}
