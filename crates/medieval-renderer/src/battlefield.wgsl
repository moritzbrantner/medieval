struct VertexInput {
    @location(0) vertex_position: vec2<f32>,
    @location(1) center_size: vec4<f32>,
    @location(2) visual: vec4<f32>,
};

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) visual: vec4<f32>,
};

@vertex
fn vs_main(input: VertexInput) -> VertexOutput {
    var output: VertexOutput;
    let center = input.center_size.xy;
    let half_extent = input.center_size.zw;
    output.position = vec4<f32>(center + input.vertex_position * half_extent, 0.0, 1.0);
    output.visual = input.visual;
    return output;
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    var color = vec3<f32>(0.69, 0.18, 0.12);
    if input.visual.x > 0.5 {
        color = vec3<f32>(0.12, 0.31, 0.67);
    }
    if input.visual.y > 0.5 {
        color = color * 0.45 + vec3<f32>(0.30, 0.30, 0.30);
    }
    if input.visual.z > 0.5 {
        color = vec3<f32>(0.95, 0.78, 0.18);
    }
    if input.visual.w > 0.5 {
        color = color * 0.70 + vec3<f32>(0.30, 0.30, 0.30);
    }
    return vec4<f32>(color, 1.0);
}
