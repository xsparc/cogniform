# Content-addressed GLB assets

Status: immutable asset records, the approved GLB subset, world references,
and bounded renderer uploads are implemented by CF007. CF015 composes those
steps into the local typed service without making them implicit. CF019 adds an
independent immutable bounded file for retaining one exact source across a
restart; import and upload remain explicit. CF020 adds bounded optional vertex
normals. CF027 retains the approved numeric metallic-roughness factors through
the same immutable upload and renderer-residency path. CF028 retains one
bounded primary f32 coordinate set. CF029 admits one shared embedded PNG
base-color texture through that path without adding scene-graph traversal.
CF030 adds explicit content-hash-wide eviction across the CPU store and
renderer without changing logical scene state. CF055 adds one bounded
source-tangent normal-texture role while preserving geometric-normal
observation semantics. CF056 adds one bounded linear packed
metallic-roughness role whose green and blue channels multiply the existing
direct-light factors. CF057 retains bounded core emissive RGB and adds it to
the imported surface. CF058 adds one bounded sRGB emissive-texture role that
multiplies that numeric factor without adding light authority. CF059 adds
deterministic imported OPAQUE and MASK coverage without blending or sorting.
CF060 adds the core single-sided default and bounded explicit double-sided
rendering without draw sorting or asset-keyed pipeline growth. CF061 admits
the ratified `KHR_materials_unlit` marker through strict declarations and
base-color-only shading without new resources or pipelines. CF062 retains
bounded core sampler filters and S/T wrapping independently for all four
existing roles through one fixed renderer-owned table. CF063 adds one bounded
primary linear vertex-color set as a base-color multiplier.

## Ownership and lifecycle

An asset is identified by the SHA-256 digest of its exact source bytes. The
canonical `ContentHash` representation is 64 lowercase hexadecimal characters.
Callers provide both the expected digest and the bytes to `AssetStore::enqueue`;
a mismatch fails before the store consumes record or queue capacity.

Processing is deliberately split into explicit steps:

```text
exact GLB bytes + expected hash
  -> verify and reserve bounded source capacity
  -> Queued record
  -> caller invokes AssetStore::process_next
  -> Ready, ProxyReady, or Rejected record
  -> immutable AssetUploadJob for (content hash, mesh index, optional role textures)
  -> atomically reserve renderer mesh and content-hash-and-role upload/residency capacity
  -> caller invokes HeadlessRenderer::process_next_asset_upload
  -> immutable GPU-resident mesh and optional role textures
```

Neither admission nor world mutation decodes an asset. Frame submission never
decodes source bytes or processes an upload. An owner should schedule
`AssetStore::process_next` on a CPU service worker and call
`process_next_asset_upload` only on the renderer domain. This library baseline
does not create workers or make those calls implicitly.

`LocalService` is now the standard in-process owner. Its
`enqueue_asset_source`, `process_next_asset_import`, `asset_record`,
`enqueue_asset_upload`, and `process_next_asset_upload` methods preserve the
same split lifecycle. `evict_asset` explicitly releases every CPU and renderer
record for one content hash, while `asset_status` returns only aggregate store
and renderer counters. Those counters include optional monotonic elapsed
microseconds for the oldest pending import and upload. An empty queue reports
no age; already-known or already-queued work retains the original age;
capacity rejection does not alter it; processing or eviction removes it with
the matching entry. No source bytes, content hash, mesh key, system-clock
timestamp, or automatic telemetry is exposed. The engine forwards immutable
upload jobs and never
exposes mutable renderer state or backend handles. The lower-level store and
renderer APIs remain available for embedders that own those domains directly.

Records are retained as `Queued`, `Ready`, `ProxyReady`, or `Rejected`. The
original source is retained only while queued. Ready records retain expanded
triangle positions, unit normals, primary coordinates, one typed immutable
source tangent and unit primary color per vertex, one typed immutable numeric
material per mesh, and
at most four role-separated immutable RGBA8 textures. A PNG referenced by
multiple roles shares its decoded CPU allocation.
Proxy records have no texture. `AssetStore::evict` removes one hash's queued
source or terminal CPU record, decoded meshes, and decoded role textures.
`HeadlessRenderer::evict_asset` removes every pending or resident mesh for the
hash and its optional unique role-texture reservations or residency. Unrelated work
keeps its FIFO order.

The authoritative world stores only `AssetMeshComponent`, containing the
content hash and zero-based mesh index. That component participates in logical
snapshots, hashing, replay, and render extraction. It contains no source bytes,
CPU mesh ownership, ECS handle, or GPU handle.

Fresh and restored local services start with empty source/import state and GPU
residency. Replay still restores the logical content hash and mesh index. A
frame that has no resident mesh and no explicit primitive fallback returns
`AssetUnavailable`; the caller must supply exact matching bytes and explicitly
drive import and upload. Rehydration does not mutate the world or append replay.

`cogniform-storage::AssetFileStore` can create and later load one separate
exact-hash source file under a caller-selected bound. It does not retain the
source inside `AssetStore`, associate it with a recovery point, discover a path
from a hash, decode the GLB, or schedule import/upload. See the
[asset-file guide](../persistence/asset-files.md).

## Explicit eviction

Eviction is content-hash-wide and caller-driven. `LocalService::evict_asset`
returns separate exact CPU-store and renderer outcomes so a caller can account
for removed records, jobs, meshes, textures, and released bytes. Repeating the
operation after the hash is absent is an idempotent no-op.

Eviction does not scan or mutate the authoritative world, increment its
revision, append replay, reserve a frame, cancel a submitted readback, or
delete an `AssetFileStore` source. A later draw therefore uses an explicitly
authored primitive fallback when present or returns `AssetUnavailable` without
consuming a frame identity. Supplying the same exact-hash bytes and explicitly
driving import and upload restores the ordinary render path.

A backend may retain physical GPU allocations until already submitted work is
safe to retire; the logical reservation and residency counters are released
immediately. The baseline has no per-mesh, LRU, reference-counted, background,
or automatic eviction and does not automatically rehydrate an evicted hash.
Callers must avoid adversarial evict/reimport retry loops.

## Approved GLB subset

The importer accepts only the following baseline:

- GLB version 2 with an exact declared file length;
- exactly two four-byte-aligned chunks: JSON first, then BIN;
- one embedded buffer with no URI;
- one or more meshes, with exactly one primitive per mesh;
- triangle-list mode, either explicit mode `4` or the glTF default;
- exactly `POSITION`, with optional `NORMAL`, `TANGENT`, `TEXCOORD_0`, and
  `COLOR_0`, within a fixed maximum of sixteen primitive attribute semantics;
- finite non-normalized f32 `VEC3` positions;
- optional non-normalized f32 `VEC3` normals with the same source count as
  positions; each direction must be finite and non-zero;
- optional non-normalized finite f32 `VEC2` `TEXCOORD_0` with the same source
  count as positions; values outside `[0, 1]` are retained unchanged;
- optional non-normalized f32 `VEC4` `TANGENT` with the same source count as
  positions; XYZ must be finite and non-zero, W must be exactly `-1` or `1`,
  and all expanded vertices in one triangle must use the same W sign;
- optional same-count `COLOR_0` as `VEC3` or `VEC4`. Components may be
  non-normalized finite f32 or normalized unsigned byte/unsigned short. Finite
  f32 values are clamped to `[0, 1]`, integers expand into that range, and
  VEC3 synthesizes alpha one. Every declared color set must be canonical,
  consecutive from zero, valid, and same-count before a valid `COLOR_1` or
  later set may receive unsupported/proxy classification;
- at most four root textures and four referenced root images across one shared
  base-color index, one shared metallic-roughness index, one shared normal
  index, and one shared emissive index. Every referencing material
  must use omitted or zero `texCoord`, and each referencing primitive must
  provide `TEXCOORD_0`;
- `normalTexture` additionally requires explicit source `NORMAL` and
  `TANGENT`; its optional `scale` must be finite and defaults to one;
- `pbrMetallicRoughness.metallicRoughnessTexture` uses the same primary
  coordinate contract, linear texels, green perceptual roughness, and blue
  metallic; red and alpha are retained but have no material effect;
- `emissiveTexture` uses the same primary-coordinate contract, sRGB-decoded
  RGB multiplied by the numeric linear `emissiveFactor`, and ignored alpha;
  omission uses a white fallback;
- at most four strict root sampler objects. Each optional `magFilter`,
  `minFilter`, `wrapS`, and `wrapT` must be one core integer enum; explicit
  null and every other field or type are invalid. Every texture must reference
  an in-range image and optional in-range sampler. Omitted filters default to
  linear and omitted wraps default to repeat. Valid unused sampler records are
  unsupported/proxy candidates only after all records and references validate;
- each image must have no URI, use an in-BIN buffer view, declare
  `image/png`, and decode as a static non-interlaced 8-bit RGB or RGBA image;
- optional non-normalized scalar u16 or u32 indices;
- optional non-empty unique-string `extensionsUsed` and
  `extensionsRequired`, with required a subset of used. The sole recognized
  name is `KHR_materials_unlit`; every actual supported or unknown extension
  member must be declared in used;
- tightly packed or valid component-aligned buffer-view strides up to 252
  bytes; and
- an optional material with unit-interval
  `pbrMetallicRoughness.baseColorFactor`, `metallicFactor`, and
  `roughnessFactor`, plus optional `emissiveFactor` containing exactly three
  finite unit-interval linear RGB values; and
- optional string `alphaMode`, defaulting to `OPAQUE`, with `OPAQUE` and `MASK`
  supported. Optional `alphaCutoff` requires an explicit mode, must be finite
  and non-negative, and defaults to `0.5` for `MASK`; values above one remain
  valid. For an explicitly selected material,
  omitted PBR factors use the glTF defaults of one and omitted emissive uses
  `[0, 0, 0]`. A primitive without a material retains the
  existing neutral Cogniform fallback `(0.8, 0.8, 0.8, 1.0)`, metallic `0`,
  and roughness `0.8`; and
- optional boolean `doubleSided`, defaulting to false. Explicit false and true
  are retained; null and every non-boolean form are invalid, including in an
  unused material record; and
- optional exact empty material `extensions.KHR_materials_unlit`, retained as
  typed `AssetShadingModel::Unlit` for selected and unused materials. Omission
  retains `MetallicRoughness`; null, scalar, array, undeclared, or otherwise
  malformed markers are invalid.

Indexed geometry is expanded into a triangle vertex stream, using the same
source index for position, normal, tangent, primary coordinate, and primary
color. The complete source coordinate, tangent, and color accessors are
validated before expanded allocation, including values not selected by the
index stream. Accepted source
normals and tangent XYZ are normalized deterministically. When `NORMAL` is
absent, each expanded triangle receives one unit cross-product normal following
its winding; degenerate triangles reject. Every output vertex is interleaved
position, normal, primary coordinate, tangent, and color and consumes exactly
64 decoded and GPU bytes. The prior 48-byte position/normal/coordinate/tangent
prefix is unchanged. Missing coordinates are exact zero; missing tangents use
`[1, 0, 0, 1]` and never enable normal sampling; missing colors are exact
white.
Every referenced range and index is checked before use, and the complete
expanded byte requirement is checked before allocation.
A primitive without indices must contain a multiple of three positions; an
indexed primitive must contain a multiple of three indices.

The strict schema rejects unknown fields after recognized unsupported feature
declarations are classified. External buffers or images, data URIs, additional
GLB chunks, sparse accessors, unsupported normal or primary-coordinate
encodings, additional coordinate sets, wider rendered color sets, morph
targets, more than four images/textures/samplers, unused image or texture
records, valid unused sampler records, JPEG and wider PNG forms, texture transforms,
occlusion texture roles, `BLEND` alpha coverage, nodes, scenes, cameras, animations,
skins, and all other or wider extensions
are not supported. There is no compressed geometry, mipmap, anisotropy, or
scene-graph traversal path.

## Failure and proxy policy

Import diagnostics contain a stable code, a static schema/import location, and
an optional collection index. They never contain source text, filenames,
parser payloads, or unbounded backend messages.

`UnsupportedAssetPolicy::Reject` is the default. With the explicit
`ProxyCuboid` policy, only these otherwise-valid classifications may become a
magenta unit-cube `ProxyReady` record:

- unsupported extension;
- unsupported feature;
- unsupported accessor; or
- unsupported primitive mode.

Invalid GLB framing or lengths, malformed or type-invalid JSON, invalid buffer
ranges or indices, non-finite positions, zero or non-finite normals or
tangents, invalid tangent handedness, normal/tangent count mismatches,
non-finite primary coordinates, primary-coordinate count mismatches, malformed
or non-finite primary colors, invalid color normalization/count/ranges or
malformed/skipped color sets, missing positions, multiple primitives, or
excess primitive attribute semantics,
or out-of-range emissive factors, malformed alpha mode/cutoff values,
malformed `doubleSided` or sampler values and indices,
malformed, duplicate, empty, or inconsistent extension declarations, malformed
or undeclared unlit markers,
malformed emissive texture roles or missing
coordinates, malformed or truncated PNG data, invalid
image ranges, degenerate
fallback triangles, and collection or byte-limit failures always produce
`Rejected`. A proxy therefore never masks malformed or over-limit input. A
syntactically valid but unsupported normal, tangent accessor, missing
normal-texture basis, primary-coordinate, image format, texture role, or well-
formed wider alpha mode, unknown extension, or non-empty unlit payload may
proxy only under explicit policy and only after malformed peer data is
excluded. Proxy vertices always contain exact zero
primary coordinates, the disabled fallback tangent, white color, opaque coverage, zero
emission, no imported texture, and the single-sided material default. The
generated proxy cube topology is unchanged; its faces therefore follow the
same hardware back-cull rule as another imported false material.

At draw time, a resident mesh uses its imported base-color factor, optional
base-color, metallic-roughness, tangent-space normal, and emissive textures,
metallic, roughness, normal scale, and emissive RGB unless the world entity has an explicit material,
which overrides the imported material as a whole and uses renderer-owned
white base-color/emissive, factor-one metallic-roughness, and neutral-normal
fallbacks. Each role selects its own immutable sampler descriptor. Repeat,
mirrored-repeat, and clamp apply independently in S and T. Magnification uses
the authored nearest/linear choice. With one retained image level, source
`NEAREST`, `NEAREST_MIPMAP_NEAREST`, and `NEAREST_MIPMAP_LINEAR` use nearest;
source `LINEAR`, `LINEAR_MIPMAP_NEAREST`, and `LINEAR_MIPMAP_LINEAR` use
linear. Base color and emissive sample as sRGB and the data roles as linear;
emissive and normal alpha plus metallic-roughness red/alpha are ignored.
Sampled base RGBA multiplies the factor and interpolated primary vertex RGBA
before material response. An explicit scene material disables the imported
vertex color along with the rest of the imported material. Imported
default or explicit `OPAQUE` ignores that alpha and emits one. Imported `MASK`
discards only products below its cutoff before any render attachment output;
equality survives and cutoff above one discards all bounded alpha. Surviving
fragments emit alpha one. An explicit scene material disables imported
coverage and preserves its own alpha semantics. An omitted or false imported
`doubleSided` value uses hardware back-face culling, so a culled fragment
writes no color, depth, entity ID, normal, or derived visibility. Explicit
true keeps both faces and reverses the completed geometric and tangent-mapped
shaded normals on a back face before observation and lighting. Built-ins,
authored primitive fallbacks, and explicit scene materials remain unculled
without that imported face correction. A selected imported unlit material
uses only the multiplied base RGB regardless of no, directional, point, or
combined lights. Its retained normal, metallic-roughness, and emissive values
and textures remain validated and CPU/GPU-accounted but are visually inert.
An explicit scene material disables imported unlit and uses ordinary direct
lighting. The
metallic-roughness green and blue channels multiply the numeric roughness and
metallic factors only for the direct-light response. The
source-tangent basis perturbs ordinary direct lighting only; depth, identity, and the
normal observation retain the geometric direction. Emissive texture RGB
multiplies the numeric factor before it is added after the ordinary no-light or
direct-light metallic-roughness response and clamped to one while alpha stays
unchanged; it neither creates a light nor illuminates another entity. If the referenced mesh is not resident,
an explicit primitive component is used as the author-chosen fallback. Without
that component, preparation fails with `AssetUnavailable`.

`AssetMaterial` carries validated numeric metadata including core emissive
RGB, typed alpha coverage/cutoff, the retained double-sided value, typed
shading model, immutable texture-role and sampler facts, and finite
normal scale. `AssetUploadJob` exposes that material and
separate optional base-color, metallic-roughness, normal, and emissive `AssetTexture`
values; the compatible
`base_color` accessor remains. A source image shared by multiple roles counts once
in CPU asset residency, while GPU bytes count once per content-hash-and-role
resource because role semantics differ. All roles are reserved atomically.
Vertex bytes use the exact 64-byte expanded accounting independently.

## Default bounds

`AssetLimits::default` applies these limits before or during CPU processing:

| Resource | Default |
|---|---:|
| Source bytes per asset | 16 MiB |
| Aggregate queued source bytes | 32 MiB |
| JSON chunk bytes | 1 MiB |
| BIN chunk bytes | 16 MiB |
| Retained asset records | 256 |
| Pending imports | 64 |
| Meshes per asset | 64 |
| Buffer views, accessors, or materials per asset | 256 each |
| Primitives per mesh | 1 |
| Expanded vertices per mesh | 262,144 |
| Source indices per mesh | 786,432 |
| Texture width or height | 2,048 |
| Texture pixels | 4,194,304 |
| Retained decoded texture bytes | 16 MiB |
| PNG decoder working bytes | 4 MiB |
| Decoded bytes per asset | 16 MiB |
| Aggregate resident CPU mesh and texture bytes | 64 MiB |

`RendererConfig::new` independently applies these GPU-side reservations:

| Resource | Default |
|---|---:|
| Pending upload jobs | 64 |
| Aggregate pending upload bytes | 32 MiB |
| Expanded vertices per mesh | 262,144 |
| Vertex bytes per mesh | 16 MiB |
| Resident meshes, including reservations | 256 |
| Resident vertex bytes, including reservations | 64 MiB |
| Texture width or height | 2,048 |
| Texture bytes per role | 16 MiB |
| Aggregate pending texture bytes | 32 MiB |
| Resident textures, including reservations | 256 |
| Resident texture bytes, including reservations | 64 MiB |

Embedders may choose different non-zero values. Renderer configuration is
rejected if a maximum-size mesh cannot fit in both the pending-byte and
resident-byte limits.

## Validation

The default offline suite verifies exact hash admission, every truncated prefix
of the checked fixture, malformed extension declarations, proxy eligibility,
material and emissive retention/defaults/range/type/texture failures, normal and tangent
normalization/count/value/range/handedness failures, primary-coordinate exact and indexed
retention, zero defaults, full-source validation, embedded RGB/RGBA expansion,
PNG truncation and malformed/reference/format/resource-limit failures,
unsupported encodings and texture roles, winding fallback, exact 64-byte and
shared/distinct role-texture accounting, atomic reservation, procedure replay,
world extraction, and
renderer upload reservation:

```text
cargo test -p cogniform-assets -p cogniform-procedural --locked --offline
cargo test --workspace --all-features --locked --offline
```

The checked text fixture under `tests/assets/triangle.glb.hex` can be exercised
on an approved DX12 or Vulkan adapter:

```text
cargo test -p cogniform-renderer --test asset_fixture --locked --offline -- --ignored --exact approved_glb_fixture_renders_with_identity_color_depth_and_winding_normal
cargo test -p cogniform-renderer --test asset_fixture --locked --offline -- --ignored --exact imported_normals_are_inverse_transformed_and_observable
cargo test -p cogniform-renderer --test asset_fixture --locked --offline -- --ignored --exact imported_material_factors_drive_direct_light_and_scene_override
cargo test -p cogniform-renderer --test asset_fixture --locked --offline -- --ignored --exact primary_texcoords_are_retained_without_changing_rendered_observations
cargo test --release -p cogniform-renderer --test asset_fixture --locked --offline -- --ignored --exact embedded_base_color_texture_preserves_orientation_factor_override_and_residency
cargo test --release -p cogniform-renderer --test asset_fixture --locked --offline -- --ignored --exact normal_texture_changes_direct_lighting_not_geometric_normal_observation
cargo test --release -p cogniform-renderer --test asset_fixture --locked --offline -- --ignored --exact metallic_roughness_texture_multiplies_factors_for_direct_lights_only
cargo test --release -p cogniform-renderer --test asset_fixture --all-features --locked --offline -- --ignored --exact emissive_factor_adds_after_unlit_or_direct_response_and_preserves_other_outputs
cargo test --release -p cogniform-renderer --test asset_fixture --all-features --locked --offline -- --ignored --exact emissive_texture_decodes_srgb_ignores_alpha_and_uses_white_fallback
cargo test --release -p cogniform-renderer --test asset_fixture --all-features --locked --offline -- --ignored --exact emissive_texture_adds_after_direct_light_and_scene_override_disables_it
cargo test --release -p cogniform-renderer --test asset_fixture --all-features --locked --offline -- --ignored --exact four_texture_roles_upload_evict_and_rehydrate_exactly
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
cargo test --release -p cogniform-renderer --test asset_fixture vertex_colors_interpolate_and_preserve_non_color_observations --all-features --locked --offline -- --ignored --exact --nocapture
cargo test --release -p cogniform-renderer --test asset_fixture vertex_color_multiplies_factor_texture_and_scene_override --all-features --locked --offline -- --ignored --exact --nocapture
cargo test --release -p cogniform-renderer --test asset_fixture vertex_color_alpha_default_material_and_double_sided_back_face_are_exact --all-features --locked --offline -- --ignored --exact --nocapture
cargo test --release -p cogniform-engine --test service_assets --locked --offline -- --ignored --exact local_service_imports_renders_and_explicitly_rehydrates_one_glb_asset
cargo test --release -p cogniform-engine --test service_assets --locked --offline -- --ignored --exact exact_hash_rehydration_restores_a_textured_asset_only_after_explicit_work
cargo test --release -p cogniform-storage --test asset_file --locked --offline -- --ignored --exact persisted_recovery_and_asset_sources_restore_renderable_state
```

The controlled tests create no window, perform no network call, and upload no
artifact. They verify exact entity identity plus tolerant imported color,
depth, position-only winding normals, imported normals under non-uniform scale,
and distinct imported/overridden direct material response with exact unlit
base RGBA. The primary-coordinate comparison proves every color, depth,
identity, normal, and background sample remains exact with coordinates that
include values outside the unit interval. The texture check pins top-to-bottom
orientation, sRGB sampling, RGB-factor multiplication, OPAQUE alpha-one output,
direct-light response,
scene override, and one shared 16-byte GPU texture without changing depth,
identity, normal, or background. The normal-texture check pins linear sampling,
finite scale, source-tangent shading, alpha irrelevance, and direct-light color
change while depth, identity, background, and geometric-normal observations
remain unchanged. The metallic-roughness check proves linear green/blue factor multiplication
for directional and point lights, red/alpha irrelevance, exact unlit and
scene-override behavior and unchanged non-color observations. The emissive
check proves bounded addition after unlit, directional, and point response,
clamping, alpha preservation, scene-override suppression, unchanged non-color
and background output, and unchanged revision/hash/replay. The emissive-texture
checks prove hardware sRGB decoding, RGB-factor multiplication, alpha
irrelevance, white/zero neutrality, both direct-light paths, override
suppression, and unchanged non-color output. The four-role test proves exact
distinct-image CPU bytes plus GPU upload, eviction, and
rehydration counts. The alpha checks prove factor-only, texture-only, and
multiplied coverage, exact cutoff equality, cutoff-above-one discard, OPAQUE
alpha-one output, explicit scene override, background preservation, every
render attachment, and unchanged revision/hash/replay. The double-sided checks use positive-scale
180-degree Y rotations to prove stable per-draw cull/uncull/cull selection,
face-oriented geometric and tangent-mapped normals, directional normal-map
lighting, point-light compatibility,
MASK discard/equality, explicit scene-material precedence, exact identity and
derived visibility, and unchanged eviction/revision/hash/replay. They do not
claim mirrored-transform support. The unlit checks prove exact sampled base
color across no, directional, point, and combined lights, visually inert but
retained fallback texture roles, explicit scene override, OPAQUE/MASK and
double-sided composition, face-oriented geometric normals, exact four-role
eviction/rehydration, and unchanged revision/hash/replay. The sampler checks
prove independent repeat/mirror/clamp selection on both axes, nearest/linear
magnification, exact nearest- and linear-family one-mip minification, whole-frame
equality for omitted, empty, and fully explicit defaults, and both independent
and one-record-shared bindings across four roles for one shared image while
preserving the same lifecycle and causality assertions. The vertex-color
checks prove interpolation, an exact white baseline, factor and
sRGB texture multiplication, independent emission, complete scene override,
OPAQUE/MASK alpha behavior, material-free fallback, double-sided back-face
orientation, stable non-color observations, exact eviction/rehydration, and
unchanged revision/hash/replay. The service/storage checks prove that restored
plain and textured asset references require explicit exact-hash CPU/GPU
rehydration without another logical mutation. The CF019 case
persists recovery and asset source in separate files, drops the source service,
restores the logical reference, observes its exact typed absence, and then
loads/imports/uploads the expected bytes without changing revision, hash, or
replay.

See [ADR 0062](../adr/0062-bounded-core-gltf-samplers.md) for the strict
sampler boundary, one-mip fallback, fixed 36-entry table, and compatibility
decision.

See [ADR 0063](../adr/0063-bounded-core-gltf-vertex-colors.md) for the strict
primary color formats, 64-byte prefix-compatible vertex ABI, multiplication
order, and scene-override decision.
