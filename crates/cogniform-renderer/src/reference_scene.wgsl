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
    emissive: vec4<f32>,
    base_color_uv_row_0: vec4<f32>,
    base_color_uv_row_1: vec4<f32>,
    normal_uv_row_0: vec4<f32>,
    normal_uv_row_1: vec4<f32>,
    metallic_roughness_uv_row_0: vec4<f32>,
    metallic_roughness_uv_row_1: vec4<f32>,
    emissive_uv_row_0: vec4<f32>,
    emissive_uv_row_1: vec4<f32>,
    optical: vec4<f32>,
    specular: vec4<f32>,
    specular_uv_row_0: vec4<f32>,
    specular_uv_row_1: vec4<f32>,
    specular_color_uv_row_0: vec4<f32>,
    specular_color_uv_row_1: vec4<f32>,
    clearcoat: vec4<f32>,
    clearcoat_uv_row_0: vec4<f32>,
    clearcoat_uv_row_1: vec4<f32>,
    clearcoat_roughness_uv_row_0: vec4<f32>,
    clearcoat_roughness_uv_row_1: vec4<f32>,
    clearcoat_normal_uv_row_0: vec4<f32>,
    clearcoat_normal_uv_row_1: vec4<f32>,
    sheen: vec4<f32>,
};

@group(0) @binding(0)
var<uniform> draw: DrawUniform;

@group(0) @binding(1)
var base_color_texture: texture_2d<f32>;

@group(0) @binding(2)
var base_color_sampler: sampler;

@group(0) @binding(3)
var normal_texture: texture_2d<f32>;

@group(0) @binding(4)
var metallic_roughness_texture: texture_2d<f32>;

@group(0) @binding(5)
var emissive_texture: texture_2d<f32>;

@group(0) @binding(6)
var normal_sampler: sampler;

@group(0) @binding(7)
var metallic_roughness_sampler: sampler;

@group(0) @binding(8)
var emissive_sampler: sampler;

@group(0) @binding(9)
var specular_texture: texture_2d<f32>;

@group(0) @binding(10)
var specular_sampler: sampler;

@group(0) @binding(11)
var specular_color_texture: texture_2d<f32>;

@group(0) @binding(12)
var specular_color_sampler: sampler;

@group(0) @binding(13)
var clearcoat_texture: texture_2d<f32>;

@group(0) @binding(14)
var clearcoat_sampler: sampler;

@group(0) @binding(15)
var clearcoat_roughness_texture: texture_2d<f32>;

@group(0) @binding(16)
var clearcoat_roughness_sampler: sampler;

@group(0) @binding(17)
var clearcoat_normal_texture: texture_2d<f32>;

@group(0) @binding(18)
var clearcoat_normal_sampler: sampler;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) world_normal: vec3<f32>,
    @location(1) world_position: vec3<f32>,
    @location(2) texcoord_0: vec2<f32>,
    @location(3) world_tangent: vec4<f32>,
    @location(4) color_0: vec4<f32>,
    @location(5) texcoord_1: vec2<f32>,
};

struct FragmentOutput {
    @location(0) color: vec4<f32>,
    @location(1) entity_id: u32,
    @location(2) normal: vec4<f32>,
};

const PI: f32 = 3.141592653589793;
const MINIMUM_ROUGHNESS: f32 = 0.05;
const SHEEN_MINIMUM_ROUGHNESS: f32 = 1e-6;

fn transform_uv(uv: vec2<f32>, row_0: vec4<f32>, row_1: vec4<f32>) -> vec2<f32> {
    let homogeneous = vec3(uv, 1.0);
    return vec2(dot(row_0.xyz, homogeneous), dot(row_1.xyz, homogeneous));
}

fn fresnel_schlick_range(
    view_half: f32,
    reflectance_at_normal: vec3<f32>,
    reflectance_at_grazing: vec3<f32>,
) -> vec3<f32> {
    let grazing = pow(1.0 - clamp(view_half, 0.0, 1.0), 5.0);
    return reflectance_at_normal
        + (reflectance_at_grazing - reflectance_at_normal) * grazing;
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
    metallic: f32,
    roughness: f32,
    dielectric_f0: f32,
    specular_color_factor: vec3<f32>,
    specular_factor: f32,
) -> vec3<f32> {
    let normal_light = clamp(dot(world_normal, surface_to_light), 0.0, 1.0);
    if normal_light <= 0.0 {
        return vec3(0.0);
    }

    let default_specular = specular_factor == 1.0
        && all(specular_color_factor == vec3(1.0));
    let configured_dielectric_normal_reflectance = min(
        vec3(dielectric_f0) * specular_color_factor,
        vec3(1.0),
    ) * specular_factor;
    let dielectric_normal_reflectance = select(
        configured_dielectric_normal_reflectance,
        vec3(dielectric_f0),
        default_specular,
    );
    let normal_reflectance = mix(
        dielectric_normal_reflectance,
        base_color,
        vec3(metallic),
    );
    let configured_grazing_reflectance = mix(
        vec3(specular_factor),
        vec3(1.0),
        vec3(metallic),
    );
    let grazing_reflectance = select(
        configured_grazing_reflectance,
        vec3(1.0),
        default_specular,
    );
    var fresnel = normal_reflectance;
    var dielectric_fresnel = dielectric_normal_reflectance;
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
                fresnel = fresnel_schlick_range(
                    view_half,
                    normal_reflectance,
                    grazing_reflectance,
                );
                dielectric_fresnel = fresnel_schlick_range(
                    view_half,
                    dielectric_normal_reflectance,
                    vec3(specular_factor),
                );
                let distribution = distribution_ggx(normal_half, roughness);
                let geometry = geometry_schlick_ggx(normal_view, roughness)
                    * geometry_schlick_ggx(normal_light, roughness);
                specular = distribution * geometry * fresnel
                    / max(4.0 * normal_view * normal_light, 1e-6);
            }
        }
    }

    let dielectric_energy = max(
        max(dielectric_fresnel.r, dielectric_fresnel.g),
        dielectric_fresnel.b,
    );
    let specular_diffuse_weight = vec3(1.0 - dielectric_energy) * (1.0 - metallic);
    let compatibility_diffuse_weight = (vec3(1.0) - fresnel) * (1.0 - metallic);
    let diffuse_weight = select(
        specular_diffuse_weight,
        compatibility_diffuse_weight,
        default_specular,
    );
    let diffuse = diffuse_weight * base_color / PI;
    return (diffuse + specular) * normal_light;
}

fn lambda_sheen_numeric_helper(direction: f32, alpha: f32) -> f32 {
    let one_minus_alpha_squared = (1.0 - alpha) * (1.0 - alpha);
    let a = mix(21.5473, 25.3245, one_minus_alpha_squared);
    let b = mix(3.82987, 3.32435, one_minus_alpha_squared);
    let c = mix(0.19823, 0.16801, one_minus_alpha_squared);
    let d = mix(-1.97760, -1.27393, one_minus_alpha_squared);
    let e = mix(-4.32054, -4.85967, one_minus_alpha_squared);
    return a / (1.0 + b * pow(direction, c)) + d * direction + e;
}

fn lambda_sheen(normal_direction: f32, alpha: f32) -> f32 {
    if normal_direction < 0.5 {
        return exp(lambda_sheen_numeric_helper(normal_direction, alpha));
    }
    return exp(
        2.0 * lambda_sheen_numeric_helper(0.5, alpha)
            - lambda_sheen_numeric_helper(1.0 - normal_direction, alpha),
    );
}

fn sheen_direct_response(
    world_normal: vec3<f32>,
    surface_to_light: vec3<f32>,
    surface_to_view: vec3<f32>,
    has_view: bool,
    roughness: f32,
) -> f32 {
    let normal_light = clamp(dot(world_normal, surface_to_light), 0.0, 1.0);
    if normal_light <= 0.0 || !has_view {
        return 0.0;
    }
    let normal_view = clamp(dot(world_normal, surface_to_view), 0.0, 1.0);
    let half_vector = surface_to_view + surface_to_light;
    let half_length_squared = dot(half_vector, half_vector);
    if normal_view <= 0.0 || half_length_squared <= 0.0 {
        return 0.0;
    }
    let inverse_half_length = inverseSqrt(half_length_squared);
    if inverse_half_length <= 0.0 {
        return 0.0;
    }
    let surface_to_half = half_vector * inverse_half_length;
    let normal_half = clamp(dot(world_normal, surface_to_half), 0.0, 1.0);
    let bounded_roughness = max(roughness, SHEEN_MINIMUM_ROUGHNESS);
    let alpha = bounded_roughness * bounded_roughness;
    let inverse_alpha = 1.0 / alpha;
    let sine_half_squared = max(1.0 - normal_half * normal_half, 0.0);
    let distribution = (2.0 + inverse_alpha)
        * pow(sine_half_squared, inverse_alpha * 0.5)
        / (2.0 * PI);
    let visibility = clamp(
        1.0 / (
            (1.0 + lambda_sheen(normal_view, alpha) + lambda_sheen(normal_light, alpha))
                * (4.0 * normal_view * normal_light)
        ),
        0.0,
        1.0,
    );
    let bounded_brdf = min(distribution * visibility, 1.0 / PI);
    return bounded_brdf * normal_light;
}

fn sheen_layered_response(
    base_response: vec3<f32>,
    world_normal: vec3<f32>,
    surface_to_light: vec3<f32>,
    surface_to_view: vec3<f32>,
    has_view: bool,
    color: vec3<f32>,
    roughness: f32,
) -> vec3<f32> {
    let maximum_color = max(max(color.r, color.g), color.b);
    if maximum_color == 0.0 {
        return base_response;
    }
    let sheen_response = sheen_direct_response(
        world_normal,
        surface_to_light,
        surface_to_view,
        has_view,
        roughness,
    );
    return base_response * (1.0 - maximum_color) + color * sheen_response;
}

fn clearcoat_fresnel_weight(
    world_normal: vec3<f32>,
    surface_to_view: vec3<f32>,
    has_view: bool,
    clearcoat_factor: f32,
) -> f32 {
    if clearcoat_factor == 0.0 {
        return 0.0;
    }
    var normal_view = 1.0;
    if has_view {
        normal_view = clamp(abs(dot(world_normal, surface_to_view)), 0.0, 1.0);
    }
    let fresnel = 0.04 + 0.96 * pow(1.0 - normal_view, 5.0);
    return clearcoat_factor * fresnel;
}

fn clearcoat_direct_response(
    world_normal: vec3<f32>,
    surface_to_light: vec3<f32>,
    surface_to_view: vec3<f32>,
    has_view: bool,
    clearcoat_roughness: f32,
) -> vec3<f32> {
    let normal_light = clamp(dot(world_normal, surface_to_light), 0.0, 1.0);
    if normal_light <= 0.0 || !has_view {
        return vec3(0.0);
    }
    let normal_view = clamp(dot(world_normal, surface_to_view), 0.0, 1.0);
    let half_vector = surface_to_view + surface_to_light;
    let half_length_squared = dot(half_vector, half_vector);
    if normal_view <= 0.0 || half_length_squared <= 0.0 {
        return vec3(0.0);
    }
    let inverse_half_length = inverseSqrt(half_length_squared);
    if inverse_half_length <= 0.0 {
        return vec3(0.0);
    }
    let surface_to_half = half_vector * inverse_half_length;
    let normal_half = clamp(dot(world_normal, surface_to_half), 0.0, 1.0);
    let distribution = distribution_ggx(normal_half, clearcoat_roughness);
    let geometry = geometry_schlick_ggx(normal_view, clearcoat_roughness)
        * geometry_schlick_ggx(normal_light, clearcoat_roughness);
    let response = distribution * geometry
        / max(4.0 * normal_view * normal_light, 1e-6);
    return vec3(response * normal_light);
}

@vertex
fn vs_main(
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) texcoord_0: vec2<f32>,
    @location(3) tangent: vec4<f32>,
    @location(4) color_0: vec4<f32>,
    @location(5) texcoord_1: vec2<f32>,
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
    output.texcoord_1 = texcoord_1;
    output.color_0 = color_0;
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
fn fs_main(
    input: VertexOutput,
    @builtin(front_facing) front_facing: bool,
) -> FragmentOutput {
    var output: FragmentOutput;
    let material_flags = u32(draw.material.w);
    let vertex_color = select(vec4(1.0), input.color_0, (material_flags & 32u) != 0u);
    let base_color_source_uv = select(
        input.texcoord_0,
        input.texcoord_1,
        (material_flags & 64u) != 0u,
    );
    let base_color_uv = transform_uv(
        base_color_source_uv,
        draw.base_color_uv_row_0,
        draw.base_color_uv_row_1,
    );
    let base_color = textureSample(base_color_texture, base_color_sampler, base_color_uv)
        * draw.color
        * vertex_color;
    if (material_flags & 4u) != 0u && base_color.a < draw.emissive.w {
        discard;
    }
    let source_geometric_world_normal = normalize(input.world_normal);
    var source_shaded_world_normal = source_geometric_world_normal;
    if (material_flags & 1u) != 0u {
        let tangent_rejected = input.world_tangent.xyz
            - source_geometric_world_normal
                * dot(source_geometric_world_normal, input.world_tangent.xyz);
        let tangent_length_squared = dot(tangent_rejected, tangent_rejected);
        if tangent_length_squared > 1e-12 {
            let world_tangent = tangent_rejected * inverseSqrt(tangent_length_squared);
            let handedness = select(-1.0, 1.0, input.world_tangent.w >= 0.0);
            let world_bitangent = cross(source_geometric_world_normal, world_tangent) * handedness;
            let normal_uv = transform_uv(
                select(
                    input.texcoord_0,
                    input.texcoord_1,
                    (material_flags & 512u) != 0u,
                ),
                draw.normal_uv_row_0,
                draw.normal_uv_row_1,
            );
            let sampled = textureSample(normal_texture, normal_sampler, normal_uv).rgb;
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
                    + source_geometric_world_normal * unit_tangent_normal.z;
                let candidate_length_squared = dot(candidate, candidate);
                if candidate_length_squared > 1e-12 {
                    source_shaded_world_normal = candidate * inverseSqrt(candidate_length_squared);
                }
            }
        }
    }
    var source_clearcoat_world_normal = source_geometric_world_normal;
    if draw.clearcoat.x != 0.0 && draw.clearcoat_normal_uv_row_1.w != 0.0 {
        let tangent_rejected = input.world_tangent.xyz
            - source_geometric_world_normal
                * dot(source_geometric_world_normal, input.world_tangent.xyz);
        let tangent_length_squared = dot(tangent_rejected, tangent_rejected);
        if tangent_length_squared > 1e-12 {
            let world_tangent = tangent_rejected * inverseSqrt(tangent_length_squared);
            let handedness = select(-1.0, 1.0, input.world_tangent.w >= 0.0);
            let world_bitangent = cross(source_geometric_world_normal, world_tangent) * handedness;
            let clearcoat_normal_uv = transform_uv(
                select(
                    input.texcoord_0,
                    input.texcoord_1,
                    (material_flags & 16384u) != 0u,
                ),
                draw.clearcoat_normal_uv_row_0,
                draw.clearcoat_normal_uv_row_1,
            );
            let sampled = textureSample(
                clearcoat_normal_texture,
                clearcoat_normal_sampler,
                clearcoat_normal_uv,
            ).rgb;
            let tangent_normal = vec3(
                (sampled.r * 2.0 - 1.0) * draw.clearcoat_normal_uv_row_0.w,
                (sampled.g * 2.0 - 1.0) * draw.clearcoat_normal_uv_row_0.w,
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
                    + source_geometric_world_normal * unit_tangent_normal.z;
                let candidate_length_squared = dot(candidate, candidate);
                if candidate_length_squared > 1e-12 {
                    source_clearcoat_world_normal = candidate
                        * inverseSqrt(candidate_length_squared);
                }
            }
        }
    }
    let double_sided = (material_flags & 8u) != 0u;
    let face_sign = select(-1.0, 1.0, front_facing || !double_sided);
    let geometric_world_normal = source_geometric_world_normal * face_sign;
    let shaded_world_normal = source_shaded_world_normal * face_sign;
    let clearcoat_world_normal = source_clearcoat_world_normal * face_sign;
    let sampled_material = textureSample(
        metallic_roughness_texture,
        metallic_roughness_sampler,
        transform_uv(
            select(
                input.texcoord_0,
                input.texcoord_1,
                (material_flags & 256u) != 0u,
            ),
            draw.metallic_roughness_uv_row_0,
            draw.metallic_roughness_uv_row_1,
        ),
    ).gb;
    let roughness = clamp(draw.material.y * sampled_material.x, 0.0, 1.0);
    let metallic = clamp(draw.material.x * sampled_material.y, 0.0, 1.0);
    let specular_factor = draw.specular.w * textureSample(
        specular_texture,
        specular_sampler,
        transform_uv(
            select(
                input.texcoord_0,
                input.texcoord_1,
                (material_flags & 1024u) != 0u,
            ),
            draw.specular_uv_row_0,
            draw.specular_uv_row_1,
        ),
    ).a;
    let specular_color_factor = draw.specular.xyz * textureSample(
        specular_color_texture,
        specular_color_sampler,
        transform_uv(
            select(
                input.texcoord_0,
                input.texcoord_1,
                (material_flags & 2048u) != 0u,
            ),
            draw.specular_color_uv_row_0,
            draw.specular_color_uv_row_1,
        ),
    ).rgb;
    let unlit = (material_flags & 16u) != 0u;
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
    var clearcoat_factor = 0.0;
    var clearcoat_roughness = draw.clearcoat.y;
    if !unlit && draw.clearcoat.x != 0.0 {
        clearcoat_factor = draw.clearcoat.x * textureSample(
            clearcoat_texture,
            clearcoat_sampler,
            transform_uv(
                select(
                    input.texcoord_0,
                    input.texcoord_1,
                    (material_flags & 4096u) != 0u,
                ),
                draw.clearcoat_uv_row_0,
                draw.clearcoat_uv_row_1,
            ),
        ).r;
        clearcoat_roughness = clamp(draw.clearcoat.y * textureSample(
            clearcoat_roughness_texture,
            clearcoat_roughness_sampler,
            transform_uv(
                select(
                    input.texcoord_0,
                    input.texcoord_1,
                    (material_flags & 8192u) != 0u,
                ),
                draw.clearcoat_roughness_uv_row_0,
                draw.clearcoat_roughness_uv_row_1,
            ),
        ).g, 0.0, 1.0);
    }
    var clearcoat_weight = 0.0;
    if !unlit && clearcoat_factor != 0.0 {
        clearcoat_weight = clearcoat_fresnel_weight(
            clearcoat_world_normal,
            surface_to_view,
            has_view,
            clearcoat_factor,
        );
    }
    var shaded_color = base_color.rgb;
    if !unlit && (draw.directional_light_count.x > 0u || draw.point_light_count.x > 0u) {
        shaded_color = vec3(0.0);
        for (var index = 0u; index < draw.directional_light_count.x; index = index + 1u) {
            let light = draw.directional_lights[index];
            let base_response = direct_material_response(
                shaded_world_normal,
                light.surface_to_light.xyz,
                surface_to_view,
                has_view,
                base_color.rgb,
                metallic,
                roughness,
                draw.optical.x,
                specular_color_factor,
                specular_factor,
            );
            let layered_base = sheen_layered_response(
                base_response,
                shaded_world_normal,
                light.surface_to_light.xyz,
                surface_to_view,
                has_view,
                draw.sheen.xyz,
                draw.sheen.w,
            );
            var response = layered_base;
            if clearcoat_factor != 0.0 {
                let coat_response = clearcoat_direct_response(
                    clearcoat_world_normal,
                    light.surface_to_light.xyz,
                    surface_to_view,
                    has_view,
                    clearcoat_roughness,
                );
                response = mix(layered_base, coat_response, clearcoat_weight);
            }
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
                    let base_response = direct_material_response(
                        shaded_world_normal,
                        surface_to_light,
                        surface_to_view,
                        has_view,
                        base_color.rgb,
                        metallic,
                        roughness,
                        draw.optical.x,
                        specular_color_factor,
                        specular_factor,
                    );
                    let layered_base = sheen_layered_response(
                        base_response,
                        shaded_world_normal,
                        surface_to_light,
                        surface_to_view,
                        has_view,
                        draw.sheen.xyz,
                        draw.sheen.w,
                    );
                    var response = layered_base;
                    if clearcoat_factor != 0.0 {
                        let coat_response = clearcoat_direct_response(
                            clearcoat_world_normal,
                            surface_to_light,
                            surface_to_view,
                            has_view,
                            clearcoat_roughness,
                        );
                        response = mix(layered_base, coat_response, clearcoat_weight);
                    }
                    let contribution = min(
                        response * light.color_intensity.rgb * attenuated_intensity,
                        vec3(1.0),
                    );
                    shaded_color = min(shaded_color + contribution, vec3(1.0));
                }
            }
        }
    } else if !unlit && clearcoat_factor != 0.0 {
        shaded_color = shaded_color * (1.0 - clearcoat_weight);
    }
    if !unlit {
        var emissive = textureSample(
            emissive_texture,
            emissive_sampler,
            transform_uv(
                select(
                    input.texcoord_0,
                    input.texcoord_1,
                    (material_flags & 128u) != 0u,
                ),
                draw.emissive_uv_row_0,
                draw.emissive_uv_row_1,
            ),
        ).rgb
            * draw.emissive.rgb
            * draw.camera_position.w;
        if clearcoat_factor != 0.0 {
            emissive = emissive * (1.0 - clearcoat_weight);
        }
        shaded_color = min(shaded_color + emissive, vec3(1.0));
    }
    let output_alpha = select(base_color.a, 1.0, (material_flags & 2u) != 0u);
    output.color = vec4(shaded_color, output_alpha);
    output.entity_id = draw.entity_id.x;
    output.normal = vec4(geometric_world_normal * 0.5 + vec3(0.5), 1.0);
    return output;
}
