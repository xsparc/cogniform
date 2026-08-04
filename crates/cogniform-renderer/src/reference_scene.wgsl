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
    @location(0) world_position: vec3<f32>,
};

struct FragmentOutput {
    @location(0) color: vec4<f32>,
    @location(1) entity_id: u32,
    @location(2) normal: vec4<f32>,
};

@vertex
fn vs_main(@location(0) position: vec3<f32>) -> VertexOutput {
    var output: VertexOutput;
    let world_position = draw.model * vec4(position, 1.0);
    output.position = draw.view_projection * world_position;
    output.world_position = world_position.xyz;
    return output;
}

@fragment
fn fs_main(input: VertexOutput, @builtin(front_facing) front_facing: bool) -> FragmentOutput {
    var output: FragmentOutput;
    var geometric_normal = normalize(cross(dpdx(input.world_position), dpdy(input.world_position)));
    // Fragment derivatives follow framebuffer coordinates, whose Y direction
    // reverses the cross-product sign for front-facing triangles. Correct the
    // sign so the emitted world-space normal follows source triangle winding.
    if front_facing {
        geometric_normal = -geometric_normal;
    }
    output.color = draw.color;
    output.entity_id = draw.entity_id.x;
    output.normal = vec4(geometric_normal * 0.5 + vec3(0.5), 1.0);
    return output;
}
