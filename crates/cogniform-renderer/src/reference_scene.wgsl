struct DirectionalLight {
    surface_to_light: vec4<f32>,
    color_intensity: vec4<f32>,
};

struct PointLight {
    position: vec4<f32>,
    color_intensity: vec4<f32>,
};

struct DrawUniform {
    model: mat4x4<f32>,
    view_projection: mat4x4<f32>,
    color: vec4<f32>,
    entity_id: vec4<u32>,
    directional_light_count: vec4<u32>,
    directional_lights: array<DirectionalLight, 4>,
    point_light_count: vec4<u32>,
    point_lights: array<PointLight, 4>,
};

@group(0) @binding(0)
var<uniform> draw: DrawUniform;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) world_normal: vec3<f32>,
    @location(1) world_position: vec3<f32>,
};

struct FragmentOutput {
    @location(0) color: vec4<f32>,
    @location(1) entity_id: u32,
    @location(2) normal: vec4<f32>,
};

@vertex
fn vs_main(@location(0) position: vec3<f32>, @location(1) normal: vec3<f32>) -> VertexOutput {
    var output: VertexOutput;
    let world_position = draw.model * vec4(position, 1.0);
    let model_linear = mat3x3<f32>(draw.model[0].xyz, draw.model[1].xyz, draw.model[2].xyz);
    let scale_x = max(max(abs(model_linear[0].x), abs(model_linear[0].y)), abs(model_linear[0].z));
    let scale_y = max(max(abs(model_linear[1].x), abs(model_linear[1].y)), abs(model_linear[1].z));
    let scale_z = max(max(abs(model_linear[2].x), abs(model_linear[2].y)), abs(model_linear[2].z));
    let common_scale = min(scale_x, min(scale_y, scale_z));
    let normalized_x = model_linear[0] / scale_x;
    let normalized_y = model_linear[1] / scale_y;
    let normalized_z = model_linear[2] / scale_z;
    // These cofactor columns equal inverse-transpose columns up to one shared
    // positive factor. Fragment normalization removes that factor, while the
    // column pre-scaling avoids avoidable overflow for non-uniform models.
    let cofactor_x = cross(normalized_y, normalized_z) * (common_scale / scale_x);
    let cofactor_y = cross(normalized_z, normalized_x) * (common_scale / scale_y);
    let cofactor_z = cross(normalized_x, normalized_y) * (common_scale / scale_z);
    let normal_matrix = mat3x3<f32>(
        cofactor_x,
        cofactor_y,
        cofactor_z,
    );
    output.position = draw.view_projection * world_position;
    output.world_normal = normal_matrix * normal;
    output.world_position = world_position.xyz;
    return output;
}

@fragment
fn fs_main(input: VertexOutput) -> FragmentOutput {
    var output: FragmentOutput;
    let world_normal = normalize(input.world_normal);
    var light_factor = vec3(1.0);
    if draw.directional_light_count.x > 0u || draw.point_light_count.x > 0u {
        light_factor = vec3(0.0);
        for (var index = 0u; index < draw.directional_light_count.x; index = index + 1u) {
            let light = draw.directional_lights[index];
            let diffuse = max(dot(world_normal, light.surface_to_light.xyz), 0.0);
            let contribution = min(
                light.color_intensity.rgb * light.color_intensity.a * diffuse,
                vec3(1.0),
            );
            light_factor = min(light_factor + contribution, vec3(1.0));
        }
        for (var index = 0u; index < draw.point_light_count.x; index = index + 1u) {
            let light = draw.point_lights[index];
            let to_light = light.position.xyz - input.world_position;
            let distance_squared = dot(to_light, to_light);
            if distance_squared > 0.0 {
                let inverse_distance = inverseSqrt(distance_squared);
                if inverse_distance > 0.0 {
                    let surface_to_light = to_light * inverse_distance;
                    let diffuse = max(dot(world_normal, surface_to_light), 0.0);
                    let attenuated_intensity = min(
                        light.color_intensity.a / max(distance_squared, 1e-6),
                        1.0,
                    );
                    let contribution = min(
                        light.color_intensity.rgb * attenuated_intensity * diffuse,
                        vec3(1.0),
                    );
                    light_factor = min(light_factor + contribution, vec3(1.0));
                }
            }
        }
    }
    output.color = vec4(draw.color.rgb * light_factor, draw.color.a);
    output.entity_id = draw.entity_id.x;
    output.normal = vec4(world_normal * 0.5 + vec3(0.5), 1.0);
    return output;
}
