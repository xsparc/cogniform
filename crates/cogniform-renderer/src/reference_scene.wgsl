const REFERENCE_ENTITY_ID: u32 = 7u;
const REFERENCE_COLOR: vec4<f32> = vec4<f32>(0.2, 0.6, 0.9, 1.0);

// Twelve triangles forming one unit cube. The order is fixed and independent
// of CPU collections or backend handles.
const CUBE_VERTICES: array<vec3<f32>, 36> = array<vec3<f32>, 36>(
    // Near face.
    vec3(-0.5, -0.5, -0.5), vec3( 0.5, -0.5, -0.5), vec3( 0.5,  0.5, -0.5),
    vec3(-0.5, -0.5, -0.5), vec3( 0.5,  0.5, -0.5), vec3(-0.5,  0.5, -0.5),
    // Far face.
    vec3( 0.5, -0.5,  0.5), vec3(-0.5, -0.5,  0.5), vec3(-0.5,  0.5,  0.5),
    vec3( 0.5, -0.5,  0.5), vec3(-0.5,  0.5,  0.5), vec3( 0.5,  0.5,  0.5),
    // Left face.
    vec3(-0.5, -0.5,  0.5), vec3(-0.5, -0.5, -0.5), vec3(-0.5,  0.5, -0.5),
    vec3(-0.5, -0.5,  0.5), vec3(-0.5,  0.5, -0.5), vec3(-0.5,  0.5,  0.5),
    // Right face.
    vec3( 0.5, -0.5, -0.5), vec3( 0.5, -0.5,  0.5), vec3( 0.5,  0.5,  0.5),
    vec3( 0.5, -0.5, -0.5), vec3( 0.5,  0.5,  0.5), vec3( 0.5,  0.5, -0.5),
    // Top face.
    vec3(-0.5,  0.5, -0.5), vec3( 0.5,  0.5, -0.5), vec3( 0.5,  0.5,  0.5),
    vec3(-0.5,  0.5, -0.5), vec3( 0.5,  0.5,  0.5), vec3(-0.5,  0.5,  0.5),
    // Bottom face.
    vec3(-0.5, -0.5,  0.5), vec3( 0.5, -0.5,  0.5), vec3( 0.5, -0.5, -0.5),
    vec3(-0.5, -0.5,  0.5), vec3( 0.5, -0.5, -0.5), vec3(-0.5, -0.5, -0.5),
);

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
};

struct FragmentOutput {
    @location(0) color: vec4<f32>,
    @location(1) entity_id: u32,
};

@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    let world = CUBE_VERTICES[vertex_index];
    // Fixed orthographic camera with a small view shear so multiple cube faces
    // remain visible. WebGPU clip-space depth is mapped to [0, 1].
    let view_projection = mat4x4<f32>(
        vec4(0.70, 0.00, 0.00, 0.00),
        vec4(0.00, 0.70, 0.00, 0.00),
        vec4(0.20, 0.15, 0.40, 0.00),
        vec4(0.00, 0.00, 0.50, 1.00),
    );
    var output: VertexOutput;
    output.position = view_projection * vec4(world, 1.0);
    return output;
}

@fragment
fn fs_main() -> FragmentOutput {
    var output: FragmentOutput;
    output.color = REFERENCE_COLOR;
    output.entity_id = REFERENCE_ENTITY_ID;
    return output;
}
