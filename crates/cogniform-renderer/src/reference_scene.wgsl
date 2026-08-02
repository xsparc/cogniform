struct DrawUniform {
    model: mat4x4<f32>,
    view_projection: mat4x4<f32>,
    color: vec4<f32>,
    entity_id: vec4<u32>,
};

@group(0) @binding(0)
var<uniform> draw: DrawUniform;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
};

struct FragmentOutput {
    @location(0) color: vec4<f32>,
    @location(1) entity_id: u32,
};

@vertex
fn vs_main(@location(0) position: vec3<f32>) -> VertexOutput {
    var output: VertexOutput;
    output.position = draw.view_projection * draw.model * vec4(position, 1.0);
    return output;
}

@fragment
fn fs_main() -> FragmentOutput {
    var output: FragmentOutput;
    output.color = draw.color;
    output.entity_id = draw.entity_id.x;
    return output;
}
