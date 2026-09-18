struct CameraUniform {
    eye_near: vec4<f32>,
    right_tan_half_fov: vec4<f32>,
    up_aspect: vec4<f32>,
    forward_far: vec4<f32>,
};

struct CharacterPalette {
    colors: array<vec4<f32>, 14>,
};

@group(0) @binding(0)
var<uniform> camera: CameraUniform;

@group(0) @binding(1)
var<uniform> character_palette: CharacterPalette;

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
    @location(5) elevation: f32,
};

struct CharacterVertexInput {
    @location(3) position_role: vec4<f32>,
    @location(4) normal_padding: vec4<f32>,
    @location(5) origin_side: vec4<f32>,
    @location(6) visual: vec4<f32>,
};

struct CharacterVertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) normal: vec3<f32>,
    @location(1) material_role: f32,
    @location(2) side: f32,
    @location(3) routed: f32,
    @location(4) selected: f32,
    @location(5) order_preview: f32,
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

fn project_world(world_position: vec3<f32>) -> vec4<f32> {
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
    return vec4<f32>(
        view_x / tan_half_x,
        view_y / tan_half_y,
        depth_a * view_z - depth_b,
        view_z,
    );
}

@vertex
fn vs_main(input: VertexInput) -> VertexOutput {
    let world_position = input.center_material.xyz
        + cube_vertex(input.vertex_index) * input.half_extent_routed.xyz;

    var output: VertexOutput;
    output.position = project_world(world_position);
    output.normal = cube_normal(input.vertex_index);
    output.material = input.center_material.w;
    output.routed = input.half_extent_routed.w;
    output.selected = input.visual.x;
    output.order_preview = input.visual.y;
    output.elevation = input.visual.z;
    return output;
}

@vertex
fn vs_character(input: CharacterVertexInput) -> CharacterVertexOutput {
    let world_position = input.origin_side.xyz + input.position_role.xyz;

    var output: CharacterVertexOutput;
    output.position = project_world(world_position);
    output.normal = input.normal_padding.xyz;
    output.material_role = input.position_role.w;
    output.side = input.origin_side.w;
    output.routed = input.visual.x;
    output.selected = input.visual.y;
    output.order_preview = input.visual.z;
    return output;
}

fn shade(color: vec3<f32>, normal: vec3<f32>) -> vec4<f32> {
    let light_direction = normalize(vec3<f32>(0.35, 0.82, 0.45));
    let diffuse = max(dot(normalize(normal), light_direction), 0.0);
    return vec4<f32>(color * (0.38 + diffuse * 0.62), 1.0);
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    var color = vec3<f32>(0.69, 0.18, 0.12);
    if input.material > 6.5 {
        color = vec3<f32>(0.46, 0.32, 0.16);
    } else if input.material > 5.5 {
        color = vec3<f32>(0.10, 0.34, 0.58);
    } else if input.material > 4.5 {
        color = vec3<f32>(0.10, 0.24, 0.08);
    } else if input.material > 3.5 {
        color = vec3<f32>(0.18, 0.45, 0.92);
    } else if input.material > 2.5 {
        color = vec3<f32>(0.88, 0.24, 0.16);
    } else if input.material > 1.5 {
        let lowland = vec3<f32>(0.14, 0.21, 0.09);
        let highland = vec3<f32>(0.40, 0.45, 0.20);
        color = mix(lowland, highland, clamp(input.elevation, 0.0, 1.0));
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
    return shade(color, input.normal);
}

@fragment
fn fs_character(input: CharacterVertexOutput) -> @location(0) vec4<f32> {
    let role = u32(clamp(input.material_role + 0.5, 0.0, 6.0));
    let side_offset = select(0u, 7u, input.side > 0.5);
    var color = character_palette.colors[side_offset + role].rgb;
    if input.routed > 0.5 {
        color = color * 0.42 + vec3<f32>(0.28, 0.28, 0.28);
    }
    if input.selected > 0.5 {
        color = color * 0.45 + vec3<f32>(0.95, 0.78, 0.18) * 0.55;
    }
    if input.order_preview > 0.5 {
        color = color * 0.72 + vec3<f32>(0.24, 0.24, 0.24);
    }
    return shade(color, input.normal);
}
