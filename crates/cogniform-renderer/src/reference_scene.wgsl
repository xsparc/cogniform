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
    camera_position: vec4<f32>,
    material: vec4<f32>,
};

@group(0) @binding(0)
var<uniform> draw: DrawUniform;

@group(0) @binding(1)
var base_color_texture: texture_2d<f32>;

@group(0) @binding(2)
var base_color_sampler: sampler;

@group(0) @binding(3)
var normal_texture: texture_2d<f32>;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) world_normal: vec3<f32>,
    @location(1) world_position: vec3<f32>,
    @location(2) texcoord_0: vec2<f32>,
    @location(3) world_tangent: vec4<f32>,
};

struct FragmentOutput {
    @location(0) color: vec4<f32>,
    @location(1) entity_id: u32,
    @location(2) normal: vec4<f32>,
};

const PI: f32 = 3.141592653589793;
const MINIMUM_ROUGHNESS: f32 = 0.05;

fn fresnel_schlick(view_half: f32, reflectance_at_normal: vec3<f32>) -> vec3<f32> {
    let grazing = pow(1.0 - clamp(view_half, 0.0, 1.0), 5.0);
    return reflectance_at_normal + (vec3(1.0) - reflectance_at_normal) * grazing;
}

fn distribution_ggx(normal_half: f32, roughness: f32) -> f32 {
    let bounded_roughness = max(roughness, MINIMUM_ROUGHNESS);
    let alpha = bounded_roughness * bounded_roughness;
    let alpha_squared = alpha * alpha;
    let denominator_term = normal_half * normal_half * (alpha_squared - 1.0) + 1.0;
    return alpha_squared / max(PI * denominator_term * denominator_term, 1e-12);
}

fn geometry_schlick_ggx(normal_direction: f32, roughness: f32) -> f32 {
    let remapped = roughness + 1.0;
    let k = remapped * remapped / 8.0;
    return normal_direction / max(normal_direction * (1.0 - k) + k, 1e-6);
}

fn direct_material_response(
    world_normal: vec3<f32>,
    surface_to_light: vec3<f32>,
    surface_to_view: vec3<f32>,
    has_view: bool,
    base_color: vec3<f32>,
) -> vec3<f32> {
    let normal_light = clamp(dot(world_normal, surface_to_light), 0.0, 1.0);
    if normal_light <= 0.0 {
        return vec3(0.0);
    }

    let metallic = draw.material.x;
    let roughness = draw.material.y;
    let normal_reflectance = mix(vec3(0.04), base_color, vec3(metallic));
    var fresnel = normal_reflectance;
    var specular = vec3(0.0);

    if has_view {
        let normal_view = clamp(dot(world_normal, surface_to_view), 0.0, 1.0);
        let half_vector = surface_to_view + surface_to_light;
        let half_length_squared = dot(half_vector, half_vector);
        if normal_view > 0.0 && half_length_squared > 0.0 {
            let inverse_half_length = inverseSqrt(half_length_squared);
            if inverse_half_length > 0.0 {
                let surface_to_half = half_vector * inverse_half_length;
                let normal_half = clamp(dot(world_normal, surface_to_half), 0.0, 1.0);
                let view_half = clamp(dot(surface_to_view, surface_to_half), 0.0, 1.0);
                fresnel = fresnel_schlick(view_half, normal_reflectance);
                let distribution = distribution_ggx(normal_half, roughness);
                let geometry = geometry_schlick_ggx(normal_view, roughness)
                    * geometry_schlick_ggx(normal_light, roughness);
                specular = distribution * geometry * fresnel
                    / max(4.0 * normal_view * normal_light, 1e-6);
            }
        }
    }

    let diffuse_weight = (vec3(1.0) - fresnel) * (1.0 - metallic);
    let diffuse = diffuse_weight * base_color / PI;
    return (diffuse + specular) * normal_light;
}

@vertex
fn vs_main(
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) texcoord_0: vec2<f32>,
    @location(3) tangent: vec4<f32>,
) -> VertexOutput {
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
    output.texcoord_0 = texcoord_0;
    let tangent_model_scale = max(scale_x, max(scale_y, scale_z));
    let tangent_matrix = mat3x3<f32>(
        model_linear[0] / tangent_model_scale,
        model_linear[1] / tangent_model_scale,
        model_linear[2] / tangent_model_scale,
    );
    let determinant_sign = select(
        -1.0,
        1.0,
        dot(tangent_matrix[0], cross(tangent_matrix[1], tangent_matrix[2])) >= 0.0,
    );
    output.world_tangent = vec4(
        tangent_matrix * tangent.xyz,
        tangent.w * determinant_sign,
    );
    return output;
}

@fragment
fn fs_main(input: VertexOutput) -> FragmentOutput {
    var output: FragmentOutput;
    let geometric_world_normal = normalize(input.world_normal);
    var shaded_world_normal = geometric_world_normal;
    if draw.material.w > 0.5 {
        let tangent_rejected = input.world_tangent.xyz
            - geometric_world_normal * dot(geometric_world_normal, input.world_tangent.xyz);
        let tangent_length_squared = dot(tangent_rejected, tangent_rejected);
        if tangent_length_squared > 1e-12 {
            let world_tangent = tangent_rejected * inverseSqrt(tangent_length_squared);
            let handedness = select(-1.0, 1.0, input.world_tangent.w >= 0.0);
            let world_bitangent = cross(geometric_world_normal, world_tangent) * handedness;
            let sampled = textureSample(normal_texture, base_color_sampler, input.texcoord_0).rgb;
            let tangent_normal = vec3(
                (sampled.r * 2.0 - 1.0) * draw.material.z,
                (sampled.g * 2.0 - 1.0) * draw.material.z,
                sampled.b * 2.0 - 1.0,
            );
            let tangent_normal_scale = max(
                abs(tangent_normal.x),
                max(abs(tangent_normal.y), abs(tangent_normal.z)),
            );
            if tangent_normal_scale > 0.0 {
                let scaled_tangent_normal = tangent_normal / tangent_normal_scale;
                let tangent_normal_length_squared = dot(
                    scaled_tangent_normal,
                    scaled_tangent_normal,
                );
                let unit_tangent_normal = scaled_tangent_normal
                    * inverseSqrt(tangent_normal_length_squared);
                let candidate = world_tangent * unit_tangent_normal.x
                    + world_bitangent * unit_tangent_normal.y
                    + geometric_world_normal * unit_tangent_normal.z;
                let candidate_length_squared = dot(candidate, candidate);
                if candidate_length_squared > 1e-12 {
                    shaded_world_normal = candidate * inverseSqrt(candidate_length_squared);
                }
            }
        }
    }
    let base_color = textureSample(base_color_texture, base_color_sampler, input.texcoord_0)
        * draw.color;
    var shaded_color = base_color.rgb;
    if draw.directional_light_count.x > 0u || draw.point_light_count.x > 0u {
        shaded_color = vec3(0.0);
        let to_view = draw.camera_position.xyz - input.world_position;
        let view_distance_squared = dot(to_view, to_view);
        var surface_to_view = vec3(0.0);
        var has_view = false;
        if view_distance_squared > 0.0 {
            let inverse_view_distance = inverseSqrt(view_distance_squared);
            if inverse_view_distance > 0.0 {
                surface_to_view = to_view * inverse_view_distance;
                has_view = true;
            }
        }
        for (var index = 0u; index < draw.directional_light_count.x; index = index + 1u) {
            let light = draw.directional_lights[index];
            let response = direct_material_response(
                shaded_world_normal,
                light.surface_to_light.xyz,
                surface_to_view,
                has_view,
                base_color.rgb,
            );
            let contribution = min(
                response * min(
                    light.color_intensity.rgb * light.color_intensity.a,
                    vec3(1.0),
                ),
                vec3(1.0),
            );
            shaded_color = min(shaded_color + contribution, vec3(1.0));
        }
        for (var index = 0u; index < draw.point_light_count.x; index = index + 1u) {
            let light = draw.point_lights[index];
            let to_light = light.position.xyz - input.world_position;
            let distance_squared = dot(to_light, to_light);
            if distance_squared > 0.0 {
                let inverse_distance = inverseSqrt(distance_squared);
                if inverse_distance > 0.0 {
                    let surface_to_light = to_light * inverse_distance;
                    let attenuated_intensity = min(
                        light.color_intensity.a / max(distance_squared, 1e-6),
                        1.0,
                    );
                    let response = direct_material_response(
                        shaded_world_normal,
                        surface_to_light,
                        surface_to_view,
                        has_view,
                        base_color.rgb,
                    );
                    let contribution = min(
                        response * light.color_intensity.rgb * attenuated_intensity,
                        vec3(1.0),
                    );
                    shaded_color = min(shaded_color + contribution, vec3(1.0));
                }
            }
        }
    }
    output.color = vec4(shaded_color, base_color.a);
    output.entity_id = draw.entity_id.x;
    output.normal = vec4(geometric_world_normal * 0.5 + vec3(0.5), 1.0);
    return output;
}
