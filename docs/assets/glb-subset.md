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
primary linear vertex-color set as a base-color multiplier. CF065 generates
default MikkTSpace tangents for an otherwise supported normal-textured
triangle primitive under fixed pre-library work guards. CF066 applies bounded
ratified `KHR_texture_transform` affine mappings independently to all four
existing texture roles and uses the normal-role mapping for generated tangents.
CF068 extends the admitted attribute formats through bounded core normalized
coordinates and required `KHR_mesh_quantization`. CF069 appends one optional
consecutive secondary coordinate set, applies exact per-role selector and
extension-override semantics, and expands the fixed decoded/GPU vertex ABI to
72 bytes while preserving the accepted 64-byte prefix. CF071 admits ratified
finite non-negative emissive strength, multiplies it into the existing
surface-only emissive path, and preserves all renderer resource counts. CF072
admits ratified material IOR, derives one bounded dielectric Fresnel base, and
appends one private uniform row without adding a resource. CF073 admits
ratified numeric specular strength and color, validates but defers both
extension texture roles, and appends one more private uniform row. CF074
admits those two roles as linear strength alpha and sRGB color RGB, expands
bounded role accounting from four to six, and preserves every prior byte as a
fixed uniform prefix. CF075 admits ratified numeric clearcoat factor and
roughness, validates but defers its three texture members, and appends one
factor row without changing texture or renderer resource accounting.
CF076 admits those three clearcoat roles with ratified linear channel and
tangent-space semantics, expands bounded role accounting from six to nine,
and preserves the complete CF075 uniform as a fixed prefix.
CF077 admits ratified numeric sheen color and roughness, validates but defers
both sheen texture members, and appends one factor row without changing the
nine-role renderer resource surface. CF078 admits both deferred roles with
sRGB color-RGB and linear roughness-alpha semantics, expands bounded role
accounting from nine to eleven, and preserves the complete CF077 uniform as a
fixed prefix.

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
triangle positions, unit normals, primary and secondary coordinates, one typed immutable
source or generated tangent and unit primary color per vertex, one typed immutable numeric
material per mesh, and
at most eleven role-separated immutable RGBA8 textures. A PNG referenced by
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
- exactly `POSITION`, with optional `NORMAL`, `TANGENT`, consecutive
  `TEXCOORD_0`/`TEXCOORD_1`, and
  `COLOR_0`, within a fixed maximum of sixteen primitive attribute semantics;
- finite non-normalized f32 `VEC3` positions, or signed/unsigned byte/short
  `VEC3` positions under required `KHR_mesh_quantization`, with either
  normalization mode. Quantized positions require exact raw `min` and `max`;
  legacy accepted f32 positions may still omit them;
- optional non-normalized f32 `VEC3` normals, or normalized signed-byte/signed-
  short normals under required `KHR_mesh_quantization`, with the same source
  count as positions; each decoded direction must be finite and non-zero;
- optional finite f32, normalized unsigned-byte/unsigned-short core, or
  `KHR_mesh_quantization` signed-byte/signed-short and unnormalized unsigned-
  byte/unsigned-short `VEC2` `TEXCOORD_0` or `TEXCOORD_1` with the same source
  count as positions. Unnormalized values outside `[0, 1]` are retained
  unchanged. Every declared set must be canonical, consecutive from zero,
  valid, and same-count before `TEXCOORD_2` or later may receive
  unsupported/proxy classification;
- optional non-normalized f32 `VEC4` `TANGENT`, or normalized signed-byte/
  signed-short tangent under required `KHR_mesh_quantization`, with the same
  source count as positions; decoded XYZ must be finite and non-zero, decoded
  W must be exactly `-1` or `1`, and all expanded vertices in one triangle
  must use the same W sign;
- optional same-count `COLOR_0` as `VEC3` or `VEC4`. Components may be
  non-normalized finite f32 or normalized unsigned byte/unsigned short. Finite
  f32 values are clamped to `[0, 1]`, integers expand into that range, and
  VEC3 synthesizes alpha one. Every declared color set must be canonical,
  consecutive from zero, valid, and same-count before a valid `COLOR_1` or
  later set may receive unsupported/proxy classification;
- at most eleven root textures and eleven referenced root images across one shared
  base-color index, one shared metallic-roughness index, one shared normal
  index, one shared emissive index, one shared specular-strength index, and one
  shared specular-color index, one shared clearcoat-intensity index, one shared
  clearcoat-roughness index, one shared clearcoat-normal index, one shared
  sheen-color index, and one shared sheen-roughness index. Every referencing material
  must select coordinate set zero or one, and each referencing primitive must
  provide the selected set plus every preceding set. Each texture info may carry a declared
  `KHR_texture_transform` object with omitted or finite two-component `offset`,
  finite `rotation`, and finite two-component `scale`, using exact defaults and
  Khronos translation-rotation-scale order. Its optional selector overrides
  the core selector and must be zero or one. Each active role's complete affine
  result must stay finite for every expanded selected coordinate;
- `normalTexture` additionally requires its selected set; its optional `scale` must
  be finite and defaults to one. When `TANGENT` is absent, default MikkTSpace
  tangents are generated from the expanded position, normal, and transformed
  normal-role selected-coordinate stream. When `NORMAL` is absent, the existing flat normals are
  generated and any completely validated source tangent is ignored and
  replaced as required by glTF;
- `pbrMetallicRoughness.metallicRoughnessTexture` uses the same selected
  coordinate contract, linear texels, green perceptual roughness, and blue
  metallic; red and alpha are retained but have no material effect;
- `emissiveTexture` uses the same selected-coordinate contract, sRGB-decoded
  RGB multiplied by the numeric linear `emissiveFactor` and retained emissive
  strength, and ignored alpha;
  omission uses a white fallback;
- `KHR_materials_specular.specularTexture` uses the same selected-coordinate
  contract and linear texels; alpha multiplies numeric `specularFactor`, while
  RGB is ignored. Omission uses a linear-white fallback;
- `KHR_materials_specular.specularColorTexture` uses the same selected-
  coordinate contract and sRGB texels; decoded RGB multiplies numeric
  `specularColorFactor`, while alpha is ignored. Omission uses an sRGB-white
  fallback;
- `KHR_materials_clearcoat.clearcoatTexture` uses the same selected-coordinate
  contract and linear texels; red multiplies numeric clearcoat factor while
  green, blue, and alpha are ignored. Omission uses a linear-white fallback;
- `KHR_materials_clearcoat.clearcoatRoughnessTexture` uses the same selected-
  coordinate contract and linear texels; green multiplies numeric coat
  roughness while red, blue, and alpha are ignored. Omission uses a linear-
  white fallback;
- `KHR_materials_clearcoat.clearcoatNormalTexture` uses the same selected-
  coordinate contract and linear tangent-space RGB. Finite scale multiplies X
  and Y before guarded normalization; alpha is ignored. Omission preserves the
  geometric coat normal independently of the base normal map;
- `KHR_materials_sheen.sheenColorTexture` uses the same selected-coordinate
  contract and sRGB texels; decoded RGB multiplies numeric sheen color while
  alpha is ignored. Omission uses an sRGB-white fallback;
- `KHR_materials_sheen.sheenRoughnessTexture` uses the same selected-
  coordinate contract and linear texels; alpha multiplies numeric sheen
  roughness while RGB is ignored. Omission uses a linear-white fallback;
- at most eleven strict root sampler objects. Each optional `magFilter`,
  `minFilter`, `wrapS`, and `wrapT` must be one core integer enum; explicit
  null and every other field or type are invalid. Every texture must reference
  an in-range image and optional in-range sampler. Omitted filters default to
  linear and omitted wraps default to repeat. Valid unused sampler records are
  unsupported/proxy candidates only after all records and references validate;
- each image must have no URI, use an in-BIN buffer view, declare
  `image/png`, and decode as a static non-interlaced 8-bit RGB or RGBA image;
- optional non-normalized scalar u16 or u32 indices;
- optional non-empty unique-string `extensionsUsed` and
  `extensionsRequired`, with required a subset of used. The recognized names
  are `KHR_materials_emissive_strength`, `KHR_materials_ior`,
  `KHR_materials_specular`, `KHR_materials_clearcoat`, `KHR_materials_sheen`,
  `KHR_materials_unlit`,
  `KHR_mesh_quantization`, and `KHR_texture_transform`; every actual supported
  or unknown extension member
  must be declared in used. An extension-only vertex encoding additionally
  requires `KHR_mesh_quantization` in required;
- component-aligned core accessor starts, four-byte-aligned explicit vertex
  strides, four-byte-aligned extension-format starts and elements, and
  component-aligned index strides, up to 252 bytes; and
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
  malformed markers are invalid; and
- optional material `extensions.KHR_materials_emissive_strength` object with
  optional finite non-negative f32 `emissiveStrength`, defaulting exactly to
  one. Null, scalar, array, undeclared, negative, or overflowed values are
  invalid. A well-formed wider property is unsupported only after the supported
  field validates. The extension must not coexist with any declared unlit
  member on one material; and
- optional material `extensions.KHR_materials_ior` object with optional finite
  f32 `ior`, defaulting exactly to `1.5`. Exact zero and values at least one
  are valid; zero-to-one, negative, non-finite, null, scalar, array, or
  undeclared values are invalid. The supported field validates before a wider
  payload becomes unsupported. The extension must not coexist with any
  declared `KHR_materials_unlit` or
  `KHR_materials_pbrSpecularGlossiness` member on the same material; and
- optional material `extensions.KHR_materials_specular` object. Optional
  `specularFactor` is a finite unit f32 with exact default one. Optional
  `specularColorFactor` is exactly three finite non-negative f32 values with
  exact default `[1,1,1]` and no upper bound. Null, scalar, array, malformed,
  undeclared, non-finite, negative, or above-one strength is invalid. The
  extension must not coexist with any declared unlit or legacy specular-
  glossiness member. Supported numeric fields validate before a wider payload
  becomes unsupported. Optional `specularTexture` and `specularColorTexture`
  infos require an in-range role index, optional coordinate selector, declared
  texture transform, root texture/sampler/source/image/PNG resources, and the
  selected primitive coordinate set. Malformed or dangling authority rejects
  without proxy; wider extension data remains unsupported only after these
  supported roles validate; and
- optional material `extensions.KHR_materials_clearcoat` object. Optional
  `clearcoatFactor` and `clearcoatRoughnessFactor` are finite unit f32 values
  with exact default zero. Null, scalar, array, malformed, undeclared, non-
  finite, negative, or above-one values are invalid. The extension must not
  coexist with any declared unlit or legacy specular-glossiness member.
  Supported numeric fields validate before a wider payload becomes
  unsupported. Optional `clearcoatTexture`, `clearcoatRoughnessTexture`, and
  `clearcoatNormalTexture` infos require an in-range role index, optional
  coordinate selector, declared texture transform, root texture/sampler/
  source/image/PNG resources, and the selected primitive coordinate set;
  normal scale must also be finite. Malformed or dangling authority rejects
  without proxy. The three well-formed roles are retained independently; and
- optional material `extensions.KHR_materials_sheen` object. Optional
  `sheenColorFactor` is exactly three finite unit f32 values with exact default
  `[0,0,0]`; optional `sheenRoughnessFactor` is a finite unit f32 with exact
  default zero. Null, scalar, malformed, undeclared, non-finite, negative, or
  above-one values are invalid. The extension must not coexist with declared
  unlit or legacy specular-glossiness. Supported numeric fields validate before
  wider-payload classification. Optional `sheenColorTexture` and
  `sheenRoughnessTexture` infos require valid shape, in-range indices, declared
  transforms, root texture/sampler/source/image/PNG resources, and selected
  primitive coordinates. Malformed or dangling authority rejects without
  proxy. The two well-formed roles are retained independently. Color texture
  RGB is sRGB-decoded and roughness texture alpha is linear; their other
  channels are ignored.

A valid texture-transform coordinate override above one or otherwise
well-formed wider transform property remains an unsupported-extension/proxy
candidate. It is never treated as identity. A malformed payload, undeclared
marker, non-finite component, or non-finite expanded affine result rejects
before that classification.

Indexed geometry is expanded into a triangle vertex stream, using the same
source index for position, normal, tangent, both coordinates, and primary
color. Every selected source attribute count is capped by
`max_vertices_per_mesh`; the complete position, normal, coordinate, tangent,
and color accessors are validated before expanded allocation, including values
not selected by the index stream. Every present selected-attribute `min` or
`max` must exactly equal its raw source extrema, ignoring normalization;
floating bounds are compared after f32 rounding. Normalized integers use the
exact Khronos signed/unsigned equations. Accepted source normals and tangent
XYZ are normalized deterministically from f64 decoded values. When `NORMAL` is
absent, each expanded triangle receives one unit cross-product normal following
its winding; degenerate triangles reject before tangent generation. A normal-
textured primitive generates tangents whenever `TANGENT` or `NORMAL` is absent.
If only the coat-normal role needs a tangent basis, generation uses its
effective transformed coordinate stream. If both base and coat normal roles
need generated tangents, their effective coordinate set and transform must
match exactly or admission returns bounded unsupported at
`glb.decoded.generated_tangent_space`. Authored tangents permit independent
normal-role coordinate streams.
Before library entry, a checked ordered-map preflight requires the sum of cubed
exact position/normal/transformed-normal-coordinate key multiplicities to be at most `268435456`,
and a library-equivalent repeated-position classification, including f32
signed-zero equality, requires
`9 * degenerate_faces * good_faces` to be at most `16777216`. Either exceeded
guard rejects at `glb.decoded.generated_tangent_work`. Generation uses the
exact-pinned corrected `bevy_mikktspace` path and must write every expanded
corner with finite non-zero deterministically renormalized XYZ, W exactly
`-1` or `1`, and one W sign per triangle. An absent or unsuitable result
rejects at `glb.decoded.generated_tangents`; no partial asset is adopted.
Every output vertex is interleaved
position, normal, primary coordinate, tangent, color, and appended secondary
coordinate and consumes exactly 72 decoded and GPU bytes. The prior 64-byte
prefix is unchanged. Missing coordinates are exact zero; missing tangents use
`[1, 0, 0, 1]` and never enable normal sampling; missing colors are exact
white.
Every referenced range and index is checked before use, and the complete
expanded byte requirement is checked before allocation.
A primitive without indices must contain a multiple of three positions; an
indexed primitive must contain a multiple of three indices.

The strict schema rejects unknown fields after recognized unsupported feature
declarations are classified. External buffers or images, data URIs, additional
GLB chunks, sparse accessors, other normal or coordinate encodings,
node-based position dequantization transforms, rendered `TEXCOORD_2` or later,
wider rendered color sets, morph
targets, more than six images/textures/samplers, unused image or texture
records, valid unused sampler records, JPEG and wider PNG forms, coordinate selectors above one,
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
non-finite coordinates, coordinate count mismatches, malformed or skipped sets, malformed
or non-finite primary colors, invalid color normalization/count/ranges or
malformed/skipped color sets, missing positions, multiple primitives, or
excess primitive attribute semantics,
or out-of-range emissive factors, malformed alpha mode/cutoff values,
malformed `doubleSided` or sampler values and indices, missing required mesh-
quantization declarations, invalid accessor bounds or source-attribute counts,
malformed, duplicate, empty, or inconsistent extension declarations, malformed
or undeclared emissive-strength, IOR, specular, clearcoat, sheen, unlit, or texture-transform markers, negative
or non-finite emissive strength, prohibited strength/unlit coexistence, non-finite
or zero-to-one IOR, prohibited IOR/unlit or IOR/specular-glossiness
coexistence, malformed specular factors or texture infos, prohibited
specular/unlit or specular/specular-glossiness coexistence, dangling specular
texture resources or missing selected coordinates,
malformed clearcoat factors or texture infos, prohibited clearcoat/unlit or
clearcoat/specular-glossiness coexistence, dangling clearcoat texture
resources or missing selected coordinates,
malformed sheen factors or texture infos, prohibited sheen/unlit or sheen/
specular-glossiness coexistence, dangling sheen texture resources or missing
selected coordinates,
texture-transform inputs or expanded results,
malformed emissive texture roles or missing
coordinates, malformed or truncated PNG data, invalid
image ranges, degenerate
fallback triangles, and collection or byte-limit failures always produce
`Rejected`. A proxy therefore never masks malformed or over-limit input. A
syntactically valid but unsupported normal, tangent accessor,
coordinate encoding, image format, texture role, or well-
formed wider alpha mode, unknown extension, non-empty unlit payload,
texture-coordinate selector above one, or future transform property may
proxy only under explicit policy and only after malformed peer data is
excluded. Generated-tangent work-limit and unsuitable-result failures always
reject and never proxy. Proxy vertices always contain exact zero
primary and secondary coordinates, the disabled fallback tangent, white color, opaque coverage, zero
emission, no imported texture, and the single-sided material default. The
generated proxy cube topology is unchanged; its faces therefore follow the
same hardware back-cull rule as another imported false material.

At draw time, a resident mesh uses its imported base-color factor, optional
base-color, metallic-roughness, tangent-space normal, and emissive textures,
metallic, roughness, normal scale, emissive RGB, emissive strength, IOR-derived
dielectric F0, specular strength/color factors, and retained finite unit
clearcoat factor and roughness, sheen color, and sheen roughness unless
the world entity has an explicit material,
which overrides the imported material as a whole and uses renderer-owned
white base-color/emissive, factor-one metallic-roughness, and neutral-normal
fallbacks. Each role selects its retained primary or secondary coordinate,
applies its own affine transform, then selects its own immutable sampler descriptor. Repeat,
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
retained tangent basis perturbs ordinary direct lighting only; depth, identity, and the
normal observation retain the geometric direction. Emissive texture RGB
multiplies the numeric factor and retained strength before it is added after the ordinary no-light or
direct-light metallic-roughness response and clamped to one while alpha stays
unchanged; it neither creates a light nor illuminates another entity. If the referenced mesh is not resident,
an explicit primitive component is used as the author-chosen fallback. Without
that component, preparation fails with `AssetUnavailable`.

For imported metallic-roughness direct lighting, retained IOR zero selects
dielectric F0 one and every other admitted value selects finite unit f64-
derived `((ior - 1) / (ior + 1))^2`. Omission, explicit scene materials,
built-ins, fallbacks, and proxies select exact `0.04`. Metallic-one output is
independent of IOR. IOR does not refract, transmit, or illuminate light.

For imported specular data, linear strength-texture alpha multiplies
`specularFactor` and sRGB-decoded color-texture RGB multiplies
`specularColorFactor`; strength RGB and color alpha are ignored. Dielectric
normal reflectance is `min(IOR_F0 * combinedColor, 1) * combinedStrength`, with
the clamp applied before strength. Dielectric grazing reflectance is the
combined scalar strength, RGB
Schlick drives the lobe, and one minus the maximum dielectric Fresnel channel
is the scalar diffuse-energy term. Exact default factors preserve the accepted
CF072 response. Strength zero produces pure dielectric diffuse, and metallic-
one output remains independent of both factors and texture multipliers.

For imported clearcoat data, factor and roughness are independent finite unit
values. Linear clearcoat red and roughness green multiply those factors; their
other channels are ignored. A non-zero combined factor adds a white GGX
direct-light layer with fixed IOR `1.5` and fixed normal reflectance `0.04`.
A separately scaled linear tangent-space normal may replace the geometric coat
normal without changing the base normal. The absolute view-normal cosine
drives its Schlick weight. Each
active directional or point contribution attenuates the complete existing
material response and adds the coat lobe; the same weight attenuates the no-
light compatibility response and surface emission exactly once. Exact factor
zero preserves the complete prior result. The coat does not affect alpha,
depth, identity, normal observation, unlit, explicit scene materials,
built-ins, fallbacks, or proxies. Exact numeric factor zero skips coat texture
and normal work.

For imported sheen data, color is exactly three finite unit linear channels and
roughness is finite unit. Active directional and point lights evaluate the
Khronos Charlie distribution and fitted visibility through the selected base
shading normal with a `1e-6` roughness floor. Cogniform caps the scalar BRDF at
`1 / PI` before color multiplication, attenuates the complete base response by
`1 - max(sheenColor)`, and adds the colored lobe. This deliberate conservative
bound avoids the non-conserving grazing behavior of the non-normative sample
shader without a lookup resource. Exact zero color preserves the prior path;
clearcoat is applied above base plus sheen. Sheen does not alter no-light
compatibility, emission, alpha, observations, unlit, scene overrides,
built-ins, fallbacks, or proxies. sRGB sheen-color RGB and linear sheen-
roughness alpha multiply the retained numeric values; color alpha and
roughness RGB are ignored. Exact zero effective color skips sheen sampling and
math.

`AssetMaterial` carries validated numeric metadata including core emissive
RGB, finite non-negative emissive strength, authored finite IOR, finite unit
dielectric F0, finite unit specular strength, and finite non-negative specular
RGB, finite unit clearcoat factor and roughness, finite unit sheen RGB and
roughness, typed alpha coverage/cutoff,
the retained double-sided value, typed
shading model, immutable texture-role, sampler, and per-role affine-transform facts, and finite
normal scale. `AssetUploadJob` exposes that material and
separate optional base-color, metallic-roughness, normal, emissive, specular-
strength, specular-color, clearcoat-intensity, clearcoat-roughness,
clearcoat-normal, sheen-color, and sheen-roughness `AssetTexture` values; the compatible
`base_color` accessor remains. A source image shared by multiple roles counts once
in CPU asset residency, while GPU bytes count once per content-hash-and-role
resource because role semantics differ. All roles are reserved atomically.
Vertex bytes use the exact 72-byte expanded accounting independently.

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

Normal-textured tangent generation also uses two non-configurable CPU work
guards before the generator runs:

| Generated-tangent work | Fixed maximum |
|---|---:|
| Sum of cubed exact welded-key multiplicities | 268,435,456 |
| Nine times degenerate-face count times good-face count | 16,777,216 |

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
material, emissive, IOR, specular, clearcoat, and sheen retention/default/range/type/derivation/
texture-precedence failures, normal and tangent
normalization/count/value/range/handedness failures, primary/secondary-coordinate exact and indexed
retention, zero defaults, full-source validation, embedded RGB/RGBA expansion,
PNG truncation and malformed/reference/format/resource-limit failures,
unsupported encodings and texture roles, winding fallback, exact 72-byte and
shared/distinct role-texture accounting, atomic reservation, procedure replay,
world extraction, and
renderer upload reservation:

```text
cargo test -p cogniform-assets -p cogniform-procedural --locked --offline
cargo test --workspace --all-features --locked --offline
cargo test --release -p cogniform-assets --test asset_store generated_tangent_maximum_resource_boundaries --locked --offline -- --ignored --exact
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
cargo test --release -p cogniform-renderer --test asset_fixture eleven_texture_roles_upload_evict_and_rehydrate_exactly --all-features --locked --offline -- --ignored --exact --nocapture
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
cargo test --release -p cogniform-renderer --test asset_fixture texture_transforms_apply_independently_to_all_four_roles --all-features --locked --offline -- --ignored --exact --nocapture
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
remain unchanged. The generated-tangent comparison uses proper UVs and proves
the missing-tangent importer produces exactly the same controlled frame as the
explicit reference basis through the unchanged renderer, lifecycle, and
observation path. The secondary-coordinate comparison proves core/extension
selector precedence, independent transformed selection for all four roles,
and unchanged depth, identity, and geometric-normal observations. The maximum-bound CPU case accepts both separated corners
and same-position/distinct-UV corners at 262,143 expanded vertices and rejects
maximum overlap before library entry. The metallic-roughness check proves linear green/blue factor multiplication
for directional and point lights, red/alpha irrelevance, exact unlit and
scene-override behavior and unchanged non-color observations. The emissive
check proves bounded addition after unlit, directional, and point response,
clamping, alpha preservation, scene-override suppression, unchanged non-color
and background output, and unchanged revision/hash/replay. The emissive-texture
checks prove hardware sRGB decoding, RGB-factor multiplication, alpha
irrelevance, white/zero neutrality, both direct-light paths, override
suppression, and unchanged non-color output. The emissive-strength comparison
proves exact one default, zero neutrality, factor-only and textured scaling
before the existing unit clamp, saturation, scene-override suppression, and
unchanged non-color output. The IOR comparison proves exact omitted/default
compatibility, zero/one/water/diamond response under directional and point
lights, metallic-one invariance, scene override, normal-map and emissive-
strength composition, and unchanged non-color output. The specular-factor
comparison proves exact omitted/default identity, zero/tinted/high-color and
IOR composition under directional, point, and combined lights, metallic-one
invariance, scene override, emission composition, and unchanged non-color
output. The specular-texture comparisons prove neutral fallback identity,
linear strength-alpha and sRGB color-RGB multiplication, ignored peer
channels, directional/point composition, explicit scene override, independent
coordinate selectors/transforms/authored samplers, exact six-role residency,
eviction/rehydration, and unchanged non-color output. The clearcoat comparison
proves exact omitted/zero identity, fixed-IOR rough and smooth coat response
over directional, point, and combined lights, IOR/specular/base-normal
composition, no-light and emission attenuation, explicit scene override, and
unchanged non-color output without resource growth. The sheen comparison
proves exact omitted/zero identity, color and roughness response under
directional and point lights, combined-light composition below clearcoat,
exact no-light and emission compatibility, scene-override suppression,
unchanged non-color output, and no texture-role growth. The four-role test proves exact
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
preserving the same lifecycle and causality assertions. The texture-transform
comparison applies independent translation, rotation, and scale combinations
to the four roles of one shared 4-by-4 image and exactly matches four one-texel
references. CPU tests separately prove exact defaults and affine rows,
malformed and wider precedence, finite-result rejection, generated normal-
coordinate tangent behavior, retained source coordinates, and exact explicit
tangents without resource-accounting growth. The vertex-color
checks prove interpolation, an exact white baseline, factor and
sRGB texture multiplication, independent emission, complete scene override,
OPAQUE/MASK alpha behavior, material-free fallback, double-sided back-face
orientation, stable non-color observations, exact eviction/rehydration, and
unchanged revision/hash/replay. The service/storage checks prove that restored
plain and textured asset references require explicit exact-hash CPU/GPU
rehydration without another logical mutation. The textured service case covers
both a base-only source and a missing-tangent normal-textured source while
preserving revision, logical hash, and replay. The CF019 case
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

See [ADR 0065](../adr/0065-bounded-generated-mikktspace-tangents.md) for the
exact generator dependency, corrected algorithms, fixed work guards, output
validation, and deterministic compatibility boundary.

See [ADR 0066](../adr/0066-bounded-gltf-texture-transforms.md) for the strict
transform payload, affine order, finite-result validation, generated-tangent
coordinate rule, uniform append, and compatibility boundary.

See [ADR 0069](../adr/0069-bounded-secondary-texture-coordinates.md) for
canonical coordinate-set sequencing, selector precedence, the appended
72-byte ABI, and the unchanged renderer-resource boundary.

See [ADR 0072](../adr/0072-bounded-gltf-material-ior.md) for the strict value
domain and exclusions, deterministic F0 derivation, appended optical row, and
direct-light compatibility boundary.

See [ADR 0073](../adr/0073-bounded-gltf-material-specular-factors.md) for
strict numeric and deferred-texture admission, IOR composition, scalar
dielectric energy, appended factor row, and unchanged resource topology.

See [ADR 0074](../adr/0074-bounded-gltf-material-specular-textures.md) for the
two role channel/color-space rules, six-role accounting, appended transform
rows, fixed bind-group growth, and adapter capability boundary.

See [ADR 0075](../adr/0075-bounded-gltf-material-clearcoat-factors.md) for
strict numeric and deferred-texture admission, fixed-IOR geometric-normal
layering, appended factor row, and unchanged resource topology.

See [ADR 0076](../adr/0076-bounded-gltf-material-clearcoat-textures.md) for
ratified channel semantics, independent coat-normal behavior, generated-
tangent agreement, nine-role accounting, appended transform rows, fixed bind-
group growth, and adapter capability boundary.

See [ADR 0077](../adr/0077-bounded-gltf-material-sheen-factors.md) for strict
numeric and deferred-texture validation, conservative bounded Charlie
layering, the appended factor row, and unchanged resource topology.

See [ADR 0078](../adr/0078-bounded-gltf-material-sheen-textures.md) for
ratified color-space/channel semantics, eleven-role accounting, appended
affine rows, fixed bind-group growth, and adapter capability preflight.
