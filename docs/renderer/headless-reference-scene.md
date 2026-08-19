# Headless reference renderer

CF004 establishes the first real render-domain implementation. It creates a
`wgpu` instance, adapter, device, queue, pipeline, textures, and readback
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
Linear `Rgba8Unorm` asset normal and metallic-roughness textures require the
same sampled, copy-destination, and filterable usages.

No optional or experimental GPU feature is enabled. The adapter summary records
the adapter name, backend, device class, and WebGPU-compliance flag for
diagnostics, but backend handles never leave `cogniform-renderer`.

## Reference scene and outputs

The built-in scene is a unit cube, renderer-local entity ID `7`, fixed linear
RGBA color `[51, 153, 230, 255]`, and a fixed orthographic camera with a small
view shear. The background entity ID is `0`, cleared depth is `1.0`, and
normal alpha `0` marks background. The centered cube contains 12
non-degenerate outward counter-clockwise triangles, 36 expanded vertices, and
exact axis-aligned source normals, zero primary coordinates, and disabled
fallback tangents in one fixed 1,728-byte payload. The reference
projection selects its near negative-Z face at the center, so that probe must
report an outward negative-Z normal.

Extracted built-in geometry supports cuboids, planes, and spheres. A plane is a
centered unit square at local Z = 0, expanded as two counter-clockwise XY
triangles with a positive-Z unit normal. Its positive XYZ dimensions scale the
full model: X and Y control visible size and Z participates in normal
transformation without creating thickness. One fixed 288-byte plane vertex
payload is allocated at renderer initialization; frames do not tessellate or
upload it. The baseline pipeline does not cull the back side and does not flip
its source normal.

A sphere is centered, unit diameter, and uses a positive-Z polar axis. Its
fixed 16 longitude sectors and 8 latitude bands form 224 non-degenerate
outward counter-clockwise triangles, expanded to 672 vertices with unit radial
normals, zero primary coordinates, and disabled fallback tangents in the same
48-byte layout. The exact 32,256-byte payload is generated
once at renderer initialization. XYZ dimensions are bounding diameters, so
non-uniform values produce an ellipsoid and the existing inverse-transpose
normal path preserves the smooth direction. Sphere topology supplies exact
zero primary coordinates, and frames perform no built-in tessellation or
upload.

Imported vertices use one 48-byte position, normal, primary-coordinate, and
source-tangent layout. Its prior 32-byte prefix remains unchanged. Optional
non-normalized finite f32 `TEXCOORD_0` reaches shader location 2 and optional
finite normalized `TANGENT` plus exact handedness reaches location 3; missing
asset values, built-ins, and proxy vertices use exact zero coordinates and a
disabled `[1, 0, 0, 1]` tangent. A mesh may sample one approved embedded PNG
for each base-color, metallic-roughness, and normal role. The renderer decodes
base RGB as sRGB and the other roles as linear data, ignores normal alpha and
metallic-roughness red/alpha, and preserves glTF top-to-bottom rows. One
renderer-owned repeat/linear one-mip sampler applies the omitted-sampler
policy. White, factor-one metallic-roughness, and neutral-normal fallbacks bind
on every draw.
External images, generated tangents, custom samplers, transforms, additional
coordinate sets, mipmaps, and other material texture roles remain unsupported.

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
by metallic, and perceptual roughness is floored to `0.05` only in the GGX
distribution to avoid a singular highlight. Each contribution and the shared
sum are clamped in linear RGB; material alpha is preserved. If neither kind is
active, the shader bypasses that response and preserves exact base RGBA. A
resident GLB mesh supplies its imported base color, metallic, roughness, and
core emissive RGB when the entity has no scene material. Emissive RGB is added
after either response and clamped to one while preserving alpha; it affects
only that surface. An explicit `MaterialComponent` overrides all imported
values together and selects zero imported emission. Built-in and material-free asset fallbacks retain
the existing color with neutral dielectric `metallic = 0`, `roughness = 0.8`.
An imported normal texture constructs a source-tangent basis after the model
transform, applies finite normal scale to sampled XY, and perturbs only this
direct-light response. Unlit output and the normal observation retain the
geometric transformed direction. An imported metallic-roughness texture
multiplies perceptual roughness by green and metallic by blue before both
directional and point response; red and alpha are ignored. A scene material
override disables all three imported texture roles.

The selected camera's extracted world translation supplies the view direction.
A zero or derived-overflow view vector suppresses specular without creating a
non-finite value. A fifth definition of either kind, a degenerate active
directional positive-Z axis, or an active point or selected camera translation
outside finite GPU f32 returns a typed error before submission.

The existing bind group carries one fixed 496-byte per-draw uniform. The prior
480-byte prefix remains model, view-projection, color, compact ID, directional
count and four directional slots, point count and four point slots, camera
position, and metallic/roughness plus normal scale/enabled state. One appended
zero-padded `vec4` contains core emissive RGB. Bindings 1, 3, and 4 select
the sampled base-color, normal, and metallic-roughness views; binding 2 is the
fixed sampler. This adds
no light buffer, alternate pipeline, runtime
configuration, or observation payload. Point range/cutoff/radius, spot lights,
emissive textures/strength, cross-surface emission, ambient or image-based
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
cargo test -p cogniform-renderer --test headless_reference --locked --offline -- --ignored --exact extracted_sphere_produces_curved_depth_identity_and_radial_normals
cargo test -p cogniform-renderer --test headless_reference --locked --offline -- --ignored --exact directional_light_modulates_front_and_back_facing_direct_color
cargo test -p cogniform-renderer --test headless_reference --locked --offline -- --ignored --exact point_light_applies_bounded_distance_and_facing_direct_shading
cargo test -p cogniform-renderer --test headless_reference --locked --offline -- --ignored --exact metallic_and_roughness_drive_distinct_bounded_direct_response
cargo test -p cogniform-renderer --test asset_fixture --locked --offline -- --ignored --exact approved_glb_fixture_renders_with_identity_color_depth_and_winding_normal
cargo test -p cogniform-renderer --test asset_fixture --locked --offline -- --ignored --exact imported_normals_are_inverse_transformed_and_observable
cargo test -p cogniform-renderer --test asset_fixture --locked --offline -- --ignored --exact imported_material_factors_drive_direct_light_and_scene_override
cargo test -p cogniform-renderer --test asset_fixture --locked --offline -- --ignored --exact primary_texcoords_are_retained_without_changing_rendered_observations
cargo test --release -p cogniform-renderer --test asset_fixture --locked --offline -- --ignored --exact embedded_base_color_texture_preserves_orientation_factor_override_and_residency
cargo test --release -p cogniform-renderer --test asset_fixture --locked --offline -- --ignored --exact metallic_roughness_texture_multiplies_factors_for_direct_lights_only
cargo test --release -p cogniform-renderer --test asset_fixture --all-features --locked --offline -- --ignored --exact emissive_factor_adds_after_unlit_or_direct_response_and_preserves_other_outputs
cargo test --release -p cogniform-renderer --test asset_fixture --locked --offline -- --ignored --exact three_texture_roles_upload_evict_and_rehydrate_exactly
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
orientation, sRGB/factor/alpha response, scene override, and shared residency.
The emissive-factor contract additionally proves bounded addition after unlit,
directional, and point response, RGB clamping, alpha preservation, explicit
override suppression, unchanged non-color/background outputs, and unchanged
world revision, logical hash, and idempotent replay.
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
See [ADR 0056](../adr/0056-bounded-metallic-roughness-textures.md) for the
packed-channel, linear-sampling, factor, role-residency, and override rules.
See [ADR 0057](../adr/0057-bounded-glb-emissive-factors.md) for the core
emissive-factor, uniform-append, clamp, and authority rules.
See [the extraction and observation guide](incremental-extraction-and-observations.md)
for the CF005 world-to-render and asynchronous feedback path.
