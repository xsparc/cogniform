# Headless reference renderer

CF004 establishes the first real render-domain implementation. It creates a
`wgpu` instance, adapter, device, queue, fixed pipelines, textures, and readback
buffers without creating a display handle, window, or presentation surface.
The public API exposes only project-owned configuration, diagnostics, adapter
metadata, and owned output values.

## Supported baseline

The initial compiled backend set is:

| Platform | Backend | CF004 role |
|---|---|---|
| Windows | Direct3D 12 or Vulkan | Local hardware or software validation |
| Linux | Vulkan | Controlled native or self-hosted validation |

Metal, OpenGL/GLES, browser WebGPU, and visible windowing are not compiled in
this slice. Unsupported platforms return a structured backend error. A selected
adapter must support three color attachments, twelve color-attachment bytes per
sample, the configured target and readback limits, and render/copy usages for
linear `Rgba8Unorm` color, `R32Uint` identity, `Rgba8Unorm` normal, and
`Depth32Float` depth. It must also support copy-destination, sampled binding,
and filterable sampling for `Rgba8UnormSrgb` asset textures.
Linear `Rgba8Unorm` asset normal, metallic-roughness, clearcoat-intensity,
clearcoat-roughness, and clearcoat-normal textures require the
same sampled, copy-destination, and filterable usages.
The fixed imported-material layout also requires at least twelve sampled
textures, twelve samplers per shader stage, twenty-five bindings per bind group, six
vertex attributes, and a 72-byte vertex-buffer stride.
Narrow adapters fail structured capability preflight before pipeline creation.

No optional or experimental GPU feature is enabled. The adapter summary records
the adapter name, backend, device class, and WebGPU-compliance flag for
diagnostics, but backend handles never leave `cogniform-renderer`.

## Reference scene and outputs

The built-in scene is a unit cube, renderer-local entity ID `7`, fixed linear
RGBA color `[51, 153, 230, 255]`, and a fixed orthographic camera with a small
view shear. The background entity ID is `0`, cleared depth is `1.0`, and
normal alpha `0` marks background. The centered cube contains 12
non-degenerate outward counter-clockwise triangles, 36 expanded vertices, and
exact axis-aligned source normals, zero primary and secondary coordinates,
disabled fallback tangents, and white colors in one fixed 2,592-byte payload. The reference
projection selects its near negative-Z face at the center, so that probe must
report an outward negative-Z normal.

Extracted built-in geometry supports cuboids, planes, and spheres. A plane is a
centered unit square at local Z = 0, expanded as two counter-clockwise XY
triangles with a positive-Z unit normal. Its positive XYZ dimensions scale the
full model: X and Y control visible size and Z participates in normal
transformation without creating thickness. One fixed 432-byte plane vertex
payload is allocated at renderer initialization; frames do not tessellate or
upload it. The unculled pipeline used by built-ins does not cull the back side and does not flip
its source normal.

A sphere is centered, unit diameter, and uses a positive-Z polar axis. Its
fixed 16 longitude sectors and 8 latitude bands form 224 non-degenerate
outward counter-clockwise triangles, expanded to 672 vertices with unit radial
normals, zero primary and secondary coordinates, disabled fallback tangents,
and white colors in the same 72-byte layout. The exact 48,384-byte payload is generated
once at renderer initialization. XYZ dimensions are bounding diameters, so
non-uniform values produce an ellipsoid and the existing inverse-transpose
normal path preserves the smooth direction. Sphere topology supplies exact
zero primary and secondary coordinates, and frames perform no built-in tessellation or
upload.

Imported vertices use one 72-byte position, normal, primary-coordinate,
tangent, primary-color, and appended secondary-coordinate layout. Its prior
64-byte prefix remains unchanged. Optional finite decoded `TEXCOORD_0` reaches shader location 2 and optional
finite normalized `TANGENT` plus exact handedness reaches location 3; missing
non-normal-mapped asset values, built-ins, and proxy vertices use exact zero
coordinates and a disabled `[1, 0, 0, 1]` tangent. A normal-textured primitive
with missing source tangents receives bounded validated default MikkTSpace
values before upload, using the normal role's selected transformed coordinates
while retaining both source sets unchanged. Optional f32 or normalized unsigned-byte/
unsigned-short `COLOR_0` VEC3/VEC4 reaches location 4 as linear unit RGBA;
optional consecutive `TEXCOORD_1` reaches location 5. Omitted secondary values,
built-ins, and proxies use exact zero.
Core normalized integer coordinates and admitted required
`KHR_mesh_quantization` attributes are expanded to these same f32 locations
before upload, so no packed source format, binding, stride, or pipeline reaches
the renderer;
missing asset values, built-ins, and proxies use white. A mesh may sample one approved embedded PNG
for each base-color, metallic-roughness, normal, emissive, specular-strength,
and specular-color role. The renderer decodes base, emissive, and specular-
color RGB as sRGB and the data roles as linear, ignores emissive/normal/color-
specular alpha, strength-specular RGB, and metallic-roughness red/alpha, and preserves glTF
top-to-bottom rows. Each role independently indexes one renderer-owned table
of exactly 36 initialization-created samplers: three U wraps by three V wraps
by two magnification filters by two effective one-mip minification filters.
Omitted sampling remains linear/repeat. Nearest-family mip filters use nearest
and linear-family mip filters use linear without generating another image
level. White base-color/emissive/specular-color, factor-one metallic-
roughness/specular-strength, and neutral-normal fallbacks bind on every draw.
Each active role independently selects primary or secondary coordinates, then
applies its retained finite `KHR_texture_transform` offset/rotation/scale
affine rows before sampling. An extension selector overrides the core selector.
External images, generated coordinates, non-core sampler features,
rendered coordinate sets above one, generated/stored mipmaps, and other material
texture roles, wider rendered colors, and morph colors remain unsupported.

`HeadlessRenderer::evict_asset` removes every pending upload and resident mesh
for one content hash, plus each unique pending or resident role texture at most
once.
The returned outcome reports exact removed counts and released bytes; unrelated
uploads retain FIFO order and repeated absent eviction is a no-op. Submitted
frames remain readable because a backend may defer physical resource
destruction until the work is safe to retire. Eviction does not modify the
extracted scene: the next preparation uses an authored primitive fallback or
returns `AssetUnavailable` until the exact asset is explicitly uploaded again.

Lighting is one fixed direct metallic-roughness baseline. A directional light
emits along its transformed local negative-Z axis; transformed positive Z is
normalized as the surface-to-light direction. A point light uses its extracted world translation
and attenuation `min(intensity / max(distance_squared, 1e-6), 1)`. Exact
source/fragment coincidence contributes zero rather than normalizing a zero
vector. A finite source whose derived f32 squared distance overflows likewise
contributes zero before direction multiplication. Each kind processes up to
four definitions in stable entity-ID order. Zero-intensity definitions count
toward their kind's capacity but are omitted from the active arrays.

Active directional and point lights evaluate GGX distribution, Schlick-GGX
Smith visibility, Schlick Fresnel, and an energy-conserving Lambert diffuse
split. Normal-incidence reflectance blends dielectric `0.04` toward base color
by metallic by default. A selected imported metallic-roughness material may
replace that dielectric base with finite unit F0 derived from ratified
`KHR_materials_ior`: exact one for authored IOR zero and f64
`((ior - 1) / (ior + 1))^2` for values at least one. Omission, explicit scene
materials, built-ins, missing-asset fallbacks, and proxies retain exact `0.04`,
and metallic one remains independent of IOR. A selected imported material may
also apply ratified numeric `KHR_materials_specular` strength and color. Its
dielectric normal reflectance is
`min(IOR_F0 * specularColorFactor, 1) * specularFactor`; its grazing
reflectance is strength; RGB Schlick drives the lobe; and the maximum
dielectric Fresnel channel controls scalar diffuse energy. Exact default
factors retain the CF072 path, strength zero gives pure dielectric diffuse, and
metallic one remains factor-independent. Linear specular-strength alpha and
sRGB-decoded specular-color RGB multiply those factors independently;
strength RGB and color alpha are ignored.
Ratified `KHR_materials_clearcoat` factor and roughness add a fixed-
IOR `1.5` white GGX layer for selected imported metallic-roughness materials.
Linear clearcoat red and roughness green multiply the numeric values; their
other channels are ignored. A finite-scaled tangent-space coat-normal texture
may replace only the coat normal, while omission preserves the geometric coat
normal even with a mapped base normal. The absolute view-normal cosine drives
Schlick Fresnel, attenuates the complete existing response before adding its direct
lobe for each active light, and attenuates the no-light compatibility response
and surface emission exactly once. Exact factor zero preserves the complete
prior shader path and skips coat texture work. The coat is visually absent for
unlit, explicit scene materials, built-ins, fallbacks, and proxies.
Ratified `KHR_materials_sheen` color and roughness add one bounded colored
direct-light lobe for selected imported metallic-roughness materials. The
selected base shading normal drives the Khronos Charlie distribution and
fitted visibility under a `1e-6` roughness floor. Cogniform caps the scalar
BRDF pointwise at `1 / PI`, attenuates the complete base response by
`1 - max(sheenColor)`, and then adds the colored lobe. This conservative
deviation from the non-normative sample shader guarantees a unit directional-
albedo bound without a lookup texture. Exact zero color preserves the prior
path. Clearcoat is applied above base plus sheen. No-light compatibility,
emission, unlit, explicit scene materials, built-ins, fallbacks, and proxies
remain sheen-free. sRGB sheen-color RGB and linear sheen-roughness alpha
multiply the retained numeric factors; color alpha and roughness RGB are
ignored. Role-correct white fallbacks preserve factor-only behavior.
Ratified `KHR_materials_anisotropy` strength and rotation reshape only the
selected imported base specular lobe. Source normal plus tangent, or the
existing bounded base normal-texture tangent generation, supplies the basis.
Nonzero strength rotates that basis and evaluates the ratified anisotropic GGX
distribution and correlated visibility for directional and point lights.
An optional linear texture maps red/green into a normalized tangent-space
direction, composes it with retained rotation, multiplies strength by blue,
and ignores alpha. A degenerate mapped direction disables anisotropy. Exact
zero effective strength takes the accepted isotropic branch. Diffuse,
IOR/specular, sheen, clearcoat, emission, no-light output, unlit behavior,
scene overrides, fallbacks, and observations remain unchanged.
Perceptual roughness is floored to `0.05` only in the GGX
distribution to avoid a singular highlight. Each contribution and the shared
sum are clamped in linear RGB; material alpha is preserved. If neither kind is
active, the shader bypasses that response and preserves exact base RGBA. A
resident GLB mesh supplies its imported base color, metallic, roughness, and
core emissive RGB when the entity has no scene material. Its interpolated
primary vertex RGBA multiplies the base-color factor and optional sampled base
RGBA before either lit or unlit response and before OPAQUE/MASK handling.
Emissive texture RGB
is sRGB-decoded, multiplied by the numeric emissive factor and optional finite
non-negative ratified emissive strength, added after either
response, and clamped to one while preserving material alpha; texture alpha is
ignored and emission affects only that surface. An explicit `MaterialComponent` overrides all imported
values together and selects zero imported emission. Built-in and material-free asset fallbacks retain
the existing color with neutral dielectric `metallic = 0`, `roughness = 0.8`.
An exact declared `KHR_materials_unlit` material instead selects sampled base
color multiplied by its numeric factor regardless of active directional or
point lights. Its normal, metallic-roughness, and emissive fallback data remain
resident and bounded but have no color effect. A scene material disables this
imported unlit selection and restores the ordinary direct response.
An imported normal texture constructs a retained tangent basis after the model
transform, applies finite normal scale to sampled XY, and perturbs only this
direct-light response. No-active-light output, imported unlit output, and the
normal observation retain the
geometric transformed direction. An imported metallic-roughness texture
multiplies perceptual roughness by green and metallic by blue before both
directional and point response; red and alpha are ignored. A scene material
override disables all twelve imported texture roles.

Imported GLB alpha coverage is evaluated independently of lighting. OPAQUE
ignores multiplied factor/texture alpha and emits one. MASK discards products
below its finite non-negative cutoff before color, depth, identity, or normal
output; equality survives and surviving alpha is one. A cutoff above one
therefore discards all bounded alpha. An explicit scene material disables this
imported coverage and retains the prior scene alpha path. BLEND, draw sorting,
pipeline blending, alpha-to-coverage, and MSAA are not part of this baseline.

Imported GLB face handling is derived from the selected material. Omitted or
false `doubleSided` selects a fixed CCW back-cull pipeline; true selects the
unculled pipeline and reverses both the completed geometric normal and the
completed tangent-mapped shaded normal on back faces before observation and
lighting. Built-ins, unresolved authored primitive fallbacks, and explicit
scene materials use the unculled pipeline without imported face correction.
The renderer creates exactly these two shared-layout pipelines once, selects
one per draw in stable entity order, and has no asset-keyed pipeline cache.

The selected camera's extracted world translation supplies the view direction.
A zero or derived-overflow view vector suppresses specular without creating a
non-finite value. A fifth definition of either kind, a degenerate active
directional positive-Z axis, or an active point or selected camera translation
outside finite GPU f32 returns a typed error before submission.

The existing bind group carries one fixed 960-byte per-draw uniform. The prior
928-byte prefix remains exact; its 912-byte, 848-byte, 832-byte, 736-byte, 720-byte, 656-byte, 640-byte, 624-byte,
496-byte, and first 480-byte
prefixes remain model,
view-projection, color, compact ID, directional
count and four directional slots, point count and four point slots, camera
position, and metallic/roughness plus normal scale/material flags. The prior
padding lane in the camera-position `vec4` carries imported emissive strength
with an exact one default. One appended
`vec4` contains core emissive RGB and uses its prior padding lane for the mask
cutoff. Eight appended `vec4` rows carry base-color, normal,
metallic-roughness, and emissive affine transforms. Material flag bit 4 selects
imported unlit shading, bit 5 selects imported vertex color, and bits 6 through
9 select secondary coordinates for base color, emissive, metallic-roughness,
and normal respectively. Bits 10 and 11 select secondary coordinates for
specular strength and color. Bits 12 through 14 select secondary coordinates
for clearcoat intensity, roughness, and normal. Bits 15 and 16 select
secondary coordinates for sheen color and roughness. The integer range through
131,071 remains exact in
the existing f32 flag lane. One optical `vec4` contains dielectric F0 and three
exact-zero padding lanes. One factor `vec4` contains specular color RGB and
strength; four rows carry strength then color affine transforms. One final
`vec4` contains clearcoat factor and roughness followed by two exact-zero
padding lanes. Six final rows carry clearcoat intensity, roughness, and normal
affine transforms; the coat-normal padding lanes carry finite scale and role
presence. One `vec4` contains sheen color RGB and roughness. Four final rows
carry sheen-color then sheen-roughness affine transforms. One factor `vec4`
contains anisotropy strength, rotation cosine, rotation sine, and exact-zero
padding. Two final rows carry the anisotropy affine transform; selector bit 17
chooses secondary coordinates and the first row's padding lane records role
presence.
Bindings 1, 3, 4, and 5 select
the sampled base-color, normal, metallic-roughness, and emissive views;
binding 2 selects base-color sampling and bindings 6, 7, and 8 select normal,
metallic-roughness, and emissive sampling. Bindings 9 and 11 select specular-
strength and specular-color views; bindings 10 and 12 select their samplers.
Bindings 13, 15, and 17 select clearcoat intensity, roughness, and normal
views; bindings 14, 16, and 18 select their samplers.
Bindings 19 and 21 select sheen color and roughness views; bindings 20 and 22
select their samplers. Binding 23 selects the linear anisotropy view and
binding 24 selects its sampler.
Inactive roles bind the
linear/repeat table entry. This adds
no light buffer, runtime-selected pipeline creation, runtime
configuration, or observation payload. Point range/cutoff/radius, spot lights,
cross-surface emission, ambient or image-based
lighting, shadows, additional material texture roles,
HDR, tone mapping, and configurable gamma conversion are unsupported.

- entity IDs must match exactly;
- color channels use an absolute tolerance of 2 units in RGBA8;
- center depth uses an absolute tolerance of 0.02;
- cuboid normals are flat outward unit vectors, plane normals retain positive
  Z, and position-only geometry follows source triangle winding; sphere radial
  normals and approved imported vertex normals are inverse-transformed into
  world space and interpolated before fragment normalization; all paths are
  quantized through signed RGB8 and renormalized after readback, and controlled
  comparisons use a 0.99 minimum dot product;
- cross-adapter bitwise image equality is not claimed.

The `u32` attachment is compact render identity, not `StableEntityId`. CF005
adds a bounded renderer-owned mapping and retains its compact-to-stable snapshot
with every pending frame, so machine-facing observations expose exact stable
identity even when compact values are later recycled.

## API lifecycle

```rust
use cogniform_renderer::{HeadlessRenderer, RendererConfig};

# async fn example() -> Result<(), Box<dyn std::error::Error>> {
let mut renderer = HeadlessRenderer::new(RendererConfig::new(64, 64)).await?;
let pending = renderer.submit_reference_scene()?;

// Explicit synchronization point. Submission itself does not wait.
let frame = pending.read()?;
assert_eq!(frame.entity_id_at(32, 32), Some(7));
assert!(frame.normal_at(32, 32).is_some());
# Ok(())
# }
```

Configuration rejects zero dimensions, dimensions above 4096, more than
4,194,304 pixels, arithmetic overflow, a zero readback timeout, and a timeout
above 60 seconds before GPU allocation. Texture copies use 256-byte padded
rows; only initialized pixel bytes enter the tightly packed output vectors.

`PendingFrame::read` is intended for conformance tests, the bounded observation
worker, and diagnostics. It may block its caller for the configured timeout.
Submission leases one preallocated color/depth/normal/identity readback set and
fails immediately when the fixed pool is exhausted. The engine worker performs
the blocking read outside renderer submission and releases the lease after
completion.

## Validation

Run the focused contract explicitly on Windows or Linux with a compatible,
approved adapter:

```text
cargo test -p cogniform-renderer --test headless_reference --locked --offline -- --ignored --exact reference_cube_produces_exact_ids_and_tolerant_color_depth_normals
cargo test -p cogniform-renderer --test headless_reference --locked --offline -- --ignored --exact extracted_plane_produces_color_depth_identity_and_plus_z_normal
cargo test --release -p cogniform-renderer --test headless_reference --all-features --locked --offline -- --ignored --exact back_facing_built_in_plane_preserves_unculled_unflipped_behavior
cargo test -p cogniform-renderer --test headless_reference --locked --offline -- --ignored --exact extracted_sphere_produces_curved_depth_identity_and_radial_normals
cargo test -p cogniform-renderer --test headless_reference --locked --offline -- --ignored --exact directional_light_modulates_front_and_back_facing_direct_color
cargo test -p cogniform-renderer --test headless_reference --locked --offline -- --ignored --exact point_light_applies_bounded_distance_and_facing_direct_shading
cargo test -p cogniform-renderer --test headless_reference --locked --offline -- --ignored --exact metallic_and_roughness_drive_distinct_bounded_direct_response
cargo test -p cogniform-renderer --test asset_fixture --locked --offline -- --ignored --exact approved_glb_fixture_renders_with_identity_color_depth_and_winding_normal
cargo test -p cogniform-renderer --test asset_fixture --locked --offline -- --ignored --exact imported_normals_are_inverse_transformed_and_observable
cargo test -p cogniform-renderer --test asset_fixture --locked --offline -- --ignored --exact imported_material_factors_drive_direct_light_and_scene_override
cargo test -p cogniform-renderer --test asset_fixture --locked --offline -- --ignored --exact primary_texcoords_are_retained_without_changing_rendered_observations
cargo test --release -p cogniform-renderer --test asset_fixture --locked --offline -- --ignored --exact embedded_base_color_texture_preserves_orientation_factor_override_and_residency
cargo test --release -p cogniform-renderer --test asset_fixture --locked --offline -- --ignored --exact generated_tangent_normal_texture_matches_explicit_render_output
cargo test --release -p cogniform-renderer --test asset_fixture --locked --offline -- --ignored --exact metallic_roughness_texture_multiplies_factors_for_direct_lights_only
cargo test --release -p cogniform-renderer --test asset_fixture --all-features --locked --offline -- --ignored --exact emissive_factor_adds_after_unlit_or_direct_response_and_preserves_other_outputs
cargo test --release -p cogniform-renderer --test asset_fixture --all-features --locked --offline -- --ignored --exact emissive_texture_decodes_srgb_ignores_alpha_and_uses_white_fallback
cargo test --release -p cogniform-renderer --test asset_fixture --all-features --locked --offline -- --ignored --exact emissive_texture_adds_after_direct_light_and_scene_override_disables_it
cargo test --release -p cogniform-renderer --test asset_fixture --all-features --locked --offline -- --ignored --exact emissive_strength_scales_factor_and_texture_before_unit_clamp
cargo test --release -p cogniform-renderer --test asset_fixture --all-features --locked --offline -- --ignored --exact material_ior_changes_only_imported_dielectric_direct_response
cargo test --release -p cogniform-renderer --test asset_fixture --all-features --locked --offline -- --ignored --exact material_specular_factors_compose_without_changing_renderer_topology
cargo test --release -p cogniform-renderer --test asset_fixture specular_textures_multiply_alpha_and_srgb_rgb_with_neutral_fallbacks --all-features --locked --offline -- --ignored --exact --nocapture
cargo test --release -p cogniform-renderer --test asset_fixture specular_texture_coordinates_transforms_and_samplers_are_independent --all-features --locked --offline -- --ignored --exact --nocapture
cargo test --release -p cogniform-renderer --test asset_fixture clearcoat_factors_layer_the_complete_imported_material_without_new_resources --all-features --locked --offline -- --ignored --exact --nocapture
cargo test --release -p cogniform-renderer --test asset_fixture clearcoat_textures_use_linear_r_g_channels_and_an_independent_scaled_normal --all-features --locked --offline -- --ignored --exact --nocapture
cargo test --release -p cogniform-renderer --test asset_fixture sheen_factors_bound_direct_light_and_preserve_compatibility_paths --all-features --locked --offline -- --ignored --exact --nocapture
cargo test --release -p cogniform-renderer --test asset_fixture sheen_textures_multiply_srgb_rgb_and_linear_alpha_with_neutral_fallbacks --all-features --locked --offline -- --ignored --exact --nocapture
cargo test --release -p cogniform-renderer --test asset_fixture sheen_texture_coordinates_transforms_and_samplers_are_independent --all-features --locked --offline -- --ignored --exact --nocapture
cargo test --release -p cogniform-renderer --test asset_fixture anisotropy_factors_rotate_direct_specular_without_new_renderer_resources --all-features --locked --offline -- --ignored --exact --nocapture
cargo test --release -p cogniform-renderer --test asset_fixture anisotropy_texture_controls_linear_direction_and_strength_while_ignoring_alpha --all-features --locked --offline -- --ignored --exact --nocapture
cargo test --release -p cogniform-renderer --test asset_fixture --all-features --locked --offline -- --ignored --exact four_texture_roles_upload_evict_and_rehydrate_exactly
cargo test --release -p cogniform-renderer --test asset_fixture --all-features --locked --offline -- --ignored --exact six_texture_roles_upload_evict_and_rehydrate_exactly
cargo test --release -p cogniform-renderer --test asset_fixture --all-features --locked --offline -- --ignored --exact twelve_texture_roles_upload_evict_and_rehydrate_exactly
cargo test --release -p cogniform-renderer --test asset_fixture --all-features --locked --offline -- --ignored --exact alpha_mask_factor_boundaries_control_every_fragment_output
cargo test --release -p cogniform-renderer --test asset_fixture --all-features --locked --offline -- --ignored --exact alpha_texture_product_opaque_mode_and_scene_override_are_exact
cargo test --release -p cogniform-renderer --test asset_fixture --all-features --locked --offline -- --ignored --exact double_sided_draws_switch_pipelines_without_reordering_or_causality_changes
cargo test --release -p cogniform-renderer --test asset_fixture --all-features --locked --offline -- --ignored --exact double_sided_back_face_composes_with_normal_maps_and_scene_override
cargo test --release -p cogniform-renderer --test asset_fixture --all-features --locked --offline -- --ignored --exact double_sided_back_face_preserves_mask_discard_and_equality
cargo test --release -p cogniform-renderer --test asset_fixture --all-features --locked --offline -- --ignored --exact unlit_base_texture_is_exact_across_lights_and_scene_override_restores_lighting
cargo test --release -p cogniform-renderer --test asset_fixture --all-features --locked --offline -- --ignored --exact unlit_double_sided_back_face_preserves_opaque_and_mask_coverage
cargo test --release -p cogniform-renderer --test asset_fixture core_sampler_wrap_and_magnification_modes_are_pixel_observable --all-features --locked --offline -- --ignored --exact --nocapture
cargo test --release -p cogniform-renderer --test asset_fixture mipmapped_minification_modes_use_the_documented_one_mip_fallback --all-features --locked --offline -- --ignored --exact --nocapture
cargo test --release -p cogniform-renderer --test asset_fixture four_texture_roles_bind_independent_samplers_for_one_shared_image --all-features --locked --offline -- --ignored --exact --nocapture
cargo test --release -p cogniform-renderer --test asset_fixture texture_transforms_apply_independently_to_all_four_roles --all-features --locked --offline -- --ignored --exact --nocapture
cargo test --release -p cogniform-renderer --test asset_fixture secondary_texture_coordinate_selectors_and_transforms_are_independent_per_role --all-features --locked --offline -- --ignored --exact --nocapture
cargo test --release -p cogniform-renderer --test asset_fixture vertex_colors_interpolate_and_preserve_non_color_observations --all-features --locked --offline -- --ignored --exact --nocapture
cargo test --release -p cogniform-renderer --test asset_fixture vertex_color_multiplies_factor_texture_and_scene_override --all-features --locked --offline -- --ignored --exact --nocapture
cargo test --release -p cogniform-renderer --test asset_fixture vertex_color_alpha_default_material_and_double_sided_back_face_are_exact --all-features --locked --offline -- --ignored --exact --nocapture
cargo test --release -p cogniform-renderer --test asset_fixture --locked --offline -- --ignored --exact content_hash_eviction_cancels_partial_uploads_and_preserves_submitted_work
```

The integration tests render at 64 by 64 pixels, verify exact object and
background IDs, probe outward cuboid, plane, sphere, and direct
directional/point material color, depth, position-only winding normals, curved
sphere depth and radial normals, and smooth normals under non-uniform scale using the declared
tolerances, verify scene and imported dielectric/metallic/roughness response,
exact unlit compatibility, scene-material override precedence, front- and
back-facing response, and near/far Point attenuation without changing
identity/depth/normals. They also compare every output sample between matching
GLBs with missing versus retained primary coordinates, then pin texture
orientation, sRGB/RGB-factor response, OPAQUE alpha-one output, scene override,
and shared residency.
The sampler probes distinguish repeat, mirrored-repeat, and clamp independently
on both axes; nearest and linear magnification; byte-identical nearest-family
and linear-family one-mip minification; whole-frame equality for omitted,
empty, and fully explicit defaults; and both independent and one-record-shared
role selection from one shared image. Their shared helper also exercises exact texture
eviction/rehydration plus unchanged revision, logical hash, and replay.
The texture-transform comparison uses one shared 4-by-4 image and independent
translation, rotation, and scale combinations for all four roles, then proves
whole-frame equality with four one-texel references.
The secondary-coordinate probe uses opposite core and extension selectors for
each role, proves extension precedence plus independent transformed selection,
and preserves depth, identity, and geometric-normal observations.
The vertex-color probes distinguish linearly interpolated primary colors,
factor and sRGB texture multiplication, independent emission, imported
OPAQUE/MASK coverage, material-free fallback, double-sided back-face normals,
and complete explicit scene-material override while preserving non-color
observations, eviction/rehydration, revision, logical hash, and replay.
The emissive-factor contract additionally proves bounded addition after unlit,
directional, and point response, RGB clamping, alpha preservation, explicit
override suppression, unchanged non-color/background outputs, and unchanged
world revision, logical hash, and idempotent replay.
The emissive-texture contract additionally proves sRGB RGB-factor
multiplication, alpha irrelevance, white and zero-factor neutrality,
directional/point composition, scene-override suppression, exact four-role GPU
accounting, eviction/rehydration, and unchanged non-color observations.
The emissive-strength contract proves the exact one default, zero neutrality,
factor-only and textured scaling before the unit clamp, saturation, explicit
scene-material suppression, and unchanged non-color observations.
The specular-texture contracts prove role-correct white fallbacks, linear
strength-alpha and sRGB color-RGB multiplication, ignored peer channels,
directional/point composition, complete scene override, independent patterned
coordinate-set overrides/transforms/authored samplers, exact six-role GPU
upload/eviction/rehydration, and unchanged depth, identity, and geometric-
normal observations.
The clearcoat contract proves exact omitted/zero identity, fixed-IOR rough and
smooth coat response over directional, point, and combined lights, retained
base IOR/specular and base-normal composition, no-light and emission
attenuation, complete scene override, unchanged non-color observations, and
no renderer resource growth.
The sheen contract proves exact omitted/zero identity, color and roughness
response under directional and point lights, combined-light composition below
clearcoat, exact no-light and emission compatibility, complete scene override,
unchanged non-color observations, sRGB color-RGB and linear roughness-alpha
multiplication, ignored peer channels, role-correct white fallbacks,
independent patterned coordinate-set overrides/transforms/authored samplers,
and exact eleven-role GPU upload/eviction/rehydration. Independent CPU vectors
reproduce the uncapped grazing-energy regression and pin the conservative
analytic and midpoint-grid bound.
The anisotropy contract proves exact omitted/zero identity, directional and
point response, rotation, roughness, tangent handedness, combined IOR/specular/
sheen/clearcoat/emission composition, no-light compatibility, complete scene
override, unchanged non-color observations, and no renderer resource growth.
Independent CPU vectors pin the ratified nonzero distribution and visibility
while preserving the accepted isotropic branch at exact zero.
The anisotropy-texture contract proves linear red/green direction, blue
strength, ignored alpha, directional/point response, scene override, no-light
behavior, unchanged non-color observations, and exact twelve-role GPU upload,
eviction, and rehydration.
The alpha-coverage contract distinguishes factor, texture, and product alpha;
pins equality, cutoff-above-one, OPAQUE, and scene-override behavior; verifies
discard across every attachment; and preserves revision, logical hash, and
idempotent replay.
The double-sided contract uses positive-scale 180-degree Y rotations to prove
stable cull/uncull/cull pipeline changes in one mixed frame, face-oriented
geometric and tangent-mapped normals with directional normal-map response and
point-light compatibility,
MASK discard/equality, exact identity and derived visibility, and explicit
scene-material precedence. The separate built-in-plane regression proves that
the same back orientation remains unculled and unflipped outside imported
material authority.
The imported-unlit contract proves byte-identical sampled base color across no,
directional, point, and combined lights; visually inert but retained four-role
fallback resources; explicit scene-material restoration of direct lighting;
OPAQUE/MASK and double-sided composition; face-oriented geometric normals; and
unchanged eviction, rehydration, revision, hash, and replay.
The eviction contract partially uploads a two-mesh textured asset, submits a
frame, releases the remaining reservation and all logical residency exactly,
keeps the submitted readback valid, verifies the authored cuboid fallback, and
then restores identical imported output through explicit exact-hash upload.
They validate output lengths and bounds behavior and require the selected
backend to be DX12 or Vulkan. Normal workspace CI compiles these tests
but leaves it ignored;
the architecture reserves adapter conformance for controlled local or
self-hosted hardware. The test does not contact a service, create a window,
upload an artifact, or require a paid runner.

See [ADR 0005](../adr/0005-bounded-headless-wgpu-baseline.md) for the dependency,
backend, identity, and synchronization decisions.
See [ADR 0020](../adr/0020-bounded-imported-vertex-normals.md) for the
position/normal vertex contract and inverse-transpose decision.
See [ADR 0021](../adr/0021-centered-built-in-plane-rendering.md) for the plane
geometry, dimension, and fallback convention.
See [ADR 0060](../adr/0060-bounded-gltf-double-sided-materials.md) for strict
material decoding, fixed pipeline selection, and back-face normal semantics.
See [ADR 0061](../adr/0061-bounded-gltf-unlit-materials.md) for strict extension
declarations, typed unlit selection, and base-color-only response.
See [ADR 0022](../adr/0022-fixed-built-in-uv-sphere-rendering.md) for the fixed
sphere topology, dimension, normal, and allocation convention.
See [ADR 0023](../adr/0023-bounded-directional-diffuse-lighting.md) for the
direction, capacity, uniform, shading, and compatibility rules.
See [ADR 0024](../adr/0024-bounded-point-diffuse-lighting.md) for point
position, attenuation, capacity, zero-distance, and appended-layout rules.
See [ADR 0025](../adr/0025-outward-built-in-cuboid-winding.md) for the cuboid
topology, exterior normal, compatibility, and canonical-lighting correction.
See [ADR 0026](../adr/0026-bounded-direct-metallic-roughness-response.md) for
the direct material response, camera/material uniform, and unlit-compatibility
rules.
See [ADR 0027](../adr/0027-imported-glb-metallic-roughness-materials.md) for
the imported numeric material, default, override, and byte-accounting rules.
See [ADR 0028](../adr/0028-bounded-primary-texture-coordinates.md) for the
primary-coordinate validation, zero-default, layout, and visual-compatibility
rules.
See [ADR 0029](../adr/0029-bounded-embedded-png-base-color-textures.md) for the
decode, residency, sampling, fallback, and material-override rules.
See [ADR 0030](../adr/0030-explicit-content-hash-asset-eviction.md) for the
content-hash eviction boundary, accounting, and submitted-work rule.
See [ADR 0055](../adr/0055-bounded-source-tangent-normal-textures.md) for the
normal-texture tangent-space, scale, role-residency, and geometric-normal rules.
See [ADR 0065](../adr/0065-bounded-generated-mikktspace-tangents.md) for
missing-tangent generation, its fixed CPU work guards, and the unchanged
renderer ABI.
See [ADR 0056](../adr/0056-bounded-metallic-roughness-textures.md) for the
packed-channel, linear-sampling, factor, role-residency, and override rules.
See [ADR 0057](../adr/0057-bounded-glb-emissive-factors.md) for the core
emissive-factor, uniform-append, clamp, and authority rules.
See [ADR 0058](../adr/0058-bounded-glb-emissive-textures.md) for emissive
texture decode, role-residency, sRGB sampling, fallback, and override rules.
See [ADR 0071](../adr/0071-bounded-gltf-emissive-strength.md) for strict
ratified extension admission, scalar retention, padding-lane reuse, and clamp
rules.
See [ADR 0072](../adr/0072-bounded-gltf-material-ior.md) for strict IOR
admission, deterministic F0 derivation, the appended optical row, and direct-
light compatibility.
See [ADR 0073](../adr/0073-bounded-gltf-material-specular-factors.md) for
strict numeric and deferred-texture admission, IOR/Fresnel composition, the
appended factor row, and unchanged resource topology.
See [ADR 0074](../adr/0074-bounded-gltf-material-specular-textures.md) for
strength-alpha/color-RGB sampling, six-role accounting, appended affine rows,
fixed bind-group growth, and capability preflight.
See [ADR 0075](../adr/0075-bounded-gltf-material-clearcoat-factors.md) for
strict numeric and deferred-texture admission, fixed-IOR geometric-normal
layering, the appended factor row, and unchanged resource topology.
See [ADR 0076](../adr/0076-bounded-gltf-material-clearcoat-textures.md) for
linear channel semantics, independent scaled coat normals, generated-tangent
agreement, nine-role accounting, appended affine rows, and capability
preflight.
See [ADR 0077](../adr/0077-bounded-gltf-material-sheen-factors.md) for strict
factor and deferred-texture admission, bounded Charlie layering, the appended
factor row, and unchanged resource topology.
See [ADR 0078](../adr/0078-bounded-gltf-material-sheen-textures.md) for
color-space/channel semantics, independent sampling authority, eleven-role
accounting, appended affine rows, and capability preflight.
See [ADR 0079](../adr/0079-bounded-gltf-material-anisotropy-factors.md) for
strict factor/rotation admission, tangent-space authority, deferred texture
validation, anisotropic GGX, and the exact zero-strength compatibility branch.
See [ADR 0080](../adr/0080-bounded-gltf-material-anisotropy-texture.md) for
linear channel semantics, twelve-role accounting, appended affine rows,
fixed bind-group growth, and capability preflight.
See [ADR 0062](../adr/0062-bounded-core-gltf-samplers.md) for strict sampler
decode, fixed-table indexing, independent role bindings, and the one-mip
fallback.
See [ADR 0063](../adr/0063-bounded-core-gltf-vertex-colors.md) for strict
primary color decoding, the 64-byte vertex ABI, multiplication order, and
override boundary.
See [ADR 0066](../adr/0066-bounded-gltf-texture-transforms.md) for strict
transform decoding, independent role sampling, generated-tangent coordinates,
and the 624-byte prefix-compatible uniform.
See [ADR 0069](../adr/0069-bounded-secondary-texture-coordinates.md) for the
appended coordinate attribute, four selector bits, and unchanged renderer
resource topology.
See [the extraction and observation guide](incremental-extraction-and-observations.md)
for the CF005 world-to-render and asynchronous feedback path.
