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

struct SkyOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_sky(@builtin(vertex_index) index: u32) -> SkyOutput {
    var point = vec2<f32>(-1.0, -1.0);
    if index == 1u {
        point = vec2<f32>(3.0, -1.0);
    } else if index == 2u {
        point = vec2<f32>(-1.0, 3.0);
    }
    var output: SkyOutput;
    output.position = vec4<f32>(point, 0.9999, 1.0);
    output.uv = point * 0.5 + vec2<f32>(0.5);
    return output;
}

@fragment
fn fs_sky(input: SkyOutput) -> @location(0) vec4<f32> {
    let horizon = smoothstep(0.0, 1.0, clamp(input.uv.y, 0.0, 1.0));
    let haze = vec3<f32>(0.50, 0.47, 0.36);
    let high_sky = vec3<f32>(0.30, 0.43, 0.51);
    let sun_glow = exp(-distance(input.uv, vec2<f32>(0.74, 0.73)) * 7.0) * 0.11;
    return vec4<f32>(mix(haze, high_sky, horizon) + vec3<f32>(sun_glow, sun_glow * 0.82, sun_glow * 0.45), 1.0);
}

@vertex
fn vs_main(input: VertexInput) -> VertexOutput {
    var local = cube_vertex(input.vertex_index);
    var normal = cube_normal(input.vertex_index);
    // The same small instanced world mesh renders tapered, layered conifer
    // crowns. This is presentation geometry, independent of forest legality.
    if input.center_material.w > 4.5 && input.center_material.w < 5.5 {
        let taper = mix(1.0, 0.10, (local.y + 1.0) * 0.5);
        local.x *= taper;
        local.z *= taper;
        if abs(normal.y) < 0.5 {
            normal = normalize(vec3<f32>(normal.x, 0.72, normal.z));
        }
    }
    let world_position = input.center_material.xyz + local * input.half_extent_routed.xyz;

    var output: VertexOutput;
    output.position = project_world(world_position);
    output.normal = normal;
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
    return vec4<f32>(color * (0.48 + diffuse * 0.52), 1.0);
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    if input.material > 17.5 {
        return vec4<f32>(0.04, 0.08, 0.04, 0.30);
    }
    var color = vec3<f32>(0.69, 0.18, 0.12);
    if input.material > 16.5 {
        color = vec3<f32>(0.26, 0.18, 0.10);
    } else if input.material > 15.5 {
        color = vec3<f32>(0.25, 0.34, 0.18);
    } else if input.material > 14.5 {
        color = vec3<f32>(0.95, 0.85, 0.35);
    } else if input.material > 13.5 {
        color = vec3<f32>(0.18, 0.45, 0.92);
    } else if input.material > 12.5 {
        color = vec3<f32>(0.86, 0.22, 0.15);
    } else if input.material > 11.5 {
        color = vec3<f32>(0.82, 0.68, 0.24);
    } else if input.material > 10.5 {
        color = vec3<f32>(0.35, 0.32, 0.28);
    } else if input.material > 9.5 {
        color = vec3<f32>(0.42, 0.30, 0.18);
    } else if input.material > 8.5 {
        color = vec3<f32>(0.25, 0.20, 0.15);
    } else if input.material > 7.5 {
        color = vec3<f32>(0.48, 0.45, 0.40);
    } else if input.material > 6.5 {
        color = vec3<f32>(0.46, 0.32, 0.16);
    } else if input.material > 5.5 {
        color = vec3<f32>(0.10, 0.34, 0.58);
    } else if input.material > 4.5 {
        color = mix(vec3<f32>(0.12, 0.29, 0.15), vec3<f32>(0.22, 0.37, 0.17), input.elevation);
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

struct TerrainInput {
    @location(7) position: vec4<f32>,
    @location(8) normal: vec4<f32>,
    @location(9) color: vec4<f32>,
};
struct TerrainOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) world_position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) color: vec3<f32>,
};
@vertex
fn vs_terrain(input: TerrainInput) -> TerrainOutput {
    var output: TerrainOutput;
    output.position = project_world(input.position.xyz);
    output.world_position = input.position.xyz;
    output.normal = input.normal.xyz;
    output.color = input.color.rgb;
    return output;
}
fn soil_noise(point: vec2<f32>) -> f32 {
    let cell = floor(point);
    let fraction = fract(point);
    let smooth_fraction = fraction * fraction * (vec2<f32>(3.0) - 2.0 * fraction);
    let hashes = vec4<f32>(
        dot(cell, vec2<f32>(127.1, 311.7)),
        dot(cell + vec2<f32>(1.0, 0.0), vec2<f32>(127.1, 311.7)),
        dot(cell + vec2<f32>(0.0, 1.0), vec2<f32>(127.1, 311.7)),
        dot(cell + vec2<f32>(1.0, 1.0), vec2<f32>(127.1, 311.7)),
    );
    let values = fract(sin(hashes) * 43758.5453);
    return mix(mix(values.x, values.y, smooth_fraction.x), mix(values.z, values.w, smooth_fraction.x), smooth_fraction.y);
}
@fragment
fn fs_terrain(input: TerrainOutput) -> @location(0) vec4<f32> {
    let broad = soil_noise(input.world_position.xz / 6500.0);
    let grain = soil_noise(input.world_position.xz / 650.0);
    let top = input.normal.y > 0.9;
    let variation = select(0.80 + grain * 0.25, 0.78 + broad * 0.30 + grain * 0.10, top);
    return shade(input.color * variation, input.normal);
}
