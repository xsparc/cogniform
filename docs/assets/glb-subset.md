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
observation semantics.

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
source tangent per vertex, one typed immutable numeric material per mesh, and
at most two role-separated immutable RGBA8 textures. A PNG referenced by both
roles shares its decoded CPU allocation.
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
- exactly `POSITION`, with optional `NORMAL`, `TANGENT`, and `TEXCOORD_0`;
- finite non-normalized f32 `VEC3` positions;
- optional non-normalized f32 `VEC3` normals with the same source count as
  positions; each direction must be finite and non-zero;
- optional non-normalized finite f32 `VEC2` `TEXCOORD_0` with the same source
  count as positions; values outside `[0, 1]` are retained unchanged;
- optional non-normalized f32 `VEC4` `TANGENT` with the same source count as
  positions; XYZ must be finite and non-zero, W must be exactly `-1` or `1`,
  and all expanded vertices in one triangle must use the same W sign;
- at most two root textures and two referenced root images across one shared
  base-color index and one shared normal index. Every referencing material
  must use omitted or zero `texCoord`, and each referencing primitive must
  provide `TEXCOORD_0`;
- `normalTexture` additionally requires explicit source `NORMAL` and
  `TANGENT`; its optional `scale` must be finite and defaults to one;
- every texture must reference an in-range image with no sampler, every table
  entry must be referenced, and the root samplers collection must be empty;
- each image must have no URI, use an in-BIN buffer view, declare
  `image/png`, and decode as a static non-interlaced 8-bit RGB or RGBA image;
- optional non-normalized scalar u16 or u32 indices;
- tightly packed or valid component-aligned buffer-view strides up to 252
  bytes; and
- an optional material with unit-interval
  `pbrMetallicRoughness.baseColorFactor`, `metallicFactor`, and
  `roughnessFactor`. For an explicitly selected material, omitted factors use
  the glTF defaults of one. A primitive without a material retains the
  existing neutral Cogniform fallback `(0.8, 0.8, 0.8, 1.0)`, metallic `0`,
  and roughness `0.8`.

Indexed geometry is expanded into a triangle vertex stream, using the same
source index for position, normal, tangent, and primary coordinate. The
complete source coordinate and tangent accessors are validated before expanded
allocation, including values not selected by the index stream. Accepted source
normals and tangent XYZ are normalized deterministically. When `NORMAL` is
absent, each expanded triangle receives one unit cross-product normal following
its winding; degenerate triangles reject. Every output vertex is interleaved
position, normal, primary coordinate, and tangent and consumes exactly 48
decoded and GPU bytes. The prior 32-byte position/normal/coordinate prefix is
unchanged. Missing coordinates are exact zero; missing tangents use
`[1, 0, 0, 1]` and never enable normal sampling.
Every referenced range and index is checked before use, and the complete
expanded byte requirement is checked before allocation.
A primitive without indices must contain a multiple of three positions; an
indexed primitive must contain a multiple of three indices.

The strict schema rejects unknown fields after recognized unsupported feature
declarations are classified. External buffers or images, data URIs, additional
GLB chunks, sparse accessors, unsupported normal or primary-coordinate
encodings, additional coordinate sets, colors, morph targets, multiple
primitives, more than two images/textures, unused image or texture records,
explicit samplers, JPEG and wider PNG forms, texture transforms,
metallic-roughness/occlusion/emissive texture
roles, alpha modes, nodes, scenes, cameras, animations, skins, and extensions
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
or truncated PNG data, invalid image ranges, degenerate
fallback triangles, and collection or byte-limit failures always produce
`Rejected`. A proxy therefore never masks malformed or over-limit input. A
syntactically valid but unsupported normal, tangent accessor, missing
normal-texture basis, primary-coordinate, image format, or texture role may
proxy only under explicit policy. Proxy vertices always contain exact zero
primary coordinates, the disabled fallback tangent, and no imported texture.

At draw time, a resident mesh uses its imported base-color factor, optional
base-color and tangent-space normal textures, metallic, roughness, and normal
scale unless the world entity has an explicit material, which overrides the
imported material as a whole and uses renderer-owned white and neutral-normal
fallbacks. The fixed repeat/linear one-mip sampler samples base color as sRGB
and normals as linear RGB; normal alpha is ignored. Sampled base RGBA
multiplies the factor before the existing unlit or direct-light path. The
source-tangent basis perturbs direct lighting only; depth, identity, and the
normal observation retain the geometric direction. If the referenced mesh is not resident,
an explicit primitive component is used as the author-chosen fallback. Without
that component, preparation fails with `AssetUnavailable`.

`AssetMaterial` carries validated numeric metadata, immutable texture-role
facts, and finite normal scale. `AssetUploadJob` exposes that material and
separate optional base-color and normal `AssetTexture` values; the compatible
`base_color` accessor remains. A source image shared by both roles counts once
in CPU asset residency, while GPU bytes count once per content-hash-and-role
resource because transfer formats differ. Both roles are reserved atomically.
Vertex bytes use the exact 48-byte expanded accounting independently.

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
material retention/defaults/range failures, normal and tangent
normalization/count/value/range/handedness failures, primary-coordinate exact and indexed
retention, zero defaults, full-source validation, embedded RGB/RGBA expansion,
PNG truncation and malformed/reference/format/resource-limit failures,
unsupported encodings and texture roles, winding fallback, exact 48-byte and
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
orientation, sRGB sampling, factor/alpha multiplication, direct-light response,
scene override, and one shared 16-byte GPU texture without changing depth,
identity, normal, or background. The normal-texture check pins linear sampling,
finite scale, source-tangent shading, alpha irrelevance, and direct-light color
change while depth, identity, background, and geometric-normal observations
remain unchanged. The service checks then prove that restored
plain and textured asset references require explicit exact-hash CPU/GPU
rehydration without another logical mutation. The CF019 case
persists recovery and asset source in separate files, drops the source service,
restores the logical reference, observes its exact typed absence, and then
loads/imports/uploads the expected bytes without changing revision, hash, or
replay.
