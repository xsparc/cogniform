# Changelog

All notable project changes will be recorded here. Cogniform has no published
release yet; the current workspace version remains `0.0.0`.

## Unreleased

### Added

- bounded canonical protocol values, limits, patches, receipts, queries, and
  observation metadata;
- atomic authoritative world state with stable identity, hierarchy, transforms,
  snapshots, render extraction, and canonical logical hashing;
- integrity-chained accepted-event recording, verified-prefix recovery, and
  exact fresh-world replay;
- bounded headless renderer paths compiled for Vulkan/DX12 with primitive and
  strict GLB geometry, color/depth/entity-ID readback, quantized world-space
  normal observations from flat fallback or imported vertex directions,
  structured visibility, and guarded
  background GPU retirement;
- deterministic primitive imagination compilation and pure seeded built-in
  cuboid-grid procedures;
- local-service execution of pure bounded procedures through ordinary patch
  admission, idempotency, processing, query, replay, and restoration;
- bounded content-addressed GLB admission, CPU decode, renderer upload, and
  explicit unsupported/proxy policy;
- optional finite same-count GLB vertex normals, deterministic winding fallback,
  interleaved GPU upload, and inverse-transpose rendering under non-uniform
  scale;
- optional finite same-count f32 `TEXCOORD_0`, including exact out-of-unit and
  indexed retention, exact 32-byte position/normal/primary-coordinate
  accounting, zero defaults, and shader input location 2;
- one bounded shared embedded PNG GLB base-color texture, with strict static
  8-bit RGB/RGBA decode, independent CPU/GPU accounting, explicit unique
  upload, fixed repeat/linear sRGB sampling, white fallback, factor/override
  semantics, and controlled orientation plus rehydration evidence;
- fixed centered XY plane rendering with counter-clockwise positive-Z winding,
  all-axis model scaling, exact primitive fallback selection, stable identity,
  and bounded color/depth/normal readback;
- fixed centered unit-diameter sphere rendering with a positive-Z polar axis,
  16-sector by 8-band initialization-only topology, outward winding, smooth
  radial normals, bounding-diameter scaling, exact fallback selection, and
  bounded curved-depth/identity/normal evidence;
- stable-ID-ordered directional diffuse lighting with a fixed four-definition
  limit, negative-Z emission convention, exact unlit compatibility, typed
  capacity/direction failures, and controlled front/back adapter evidence;
- stable-ID-ordered point diffuse lighting with an independent fixed
  four-definition limit, world-translation sources, capped inverse-square
  attenuation, exact-zero and finite-input distance-overflow safety, typed
  capacity/range failures, and controlled near/far/back-facing adapter
  evidence;
- bounded direct Cook-Torrance metallic-roughness response for the existing
  directional and point lights, with GGX/Smith/Schlick terms, finite camera
  view input, exact unlit compatibility, a fixed 480-byte draw uniform, and
  controlled dielectric/metal/roughness evidence;
- immutable imported GLB base-color, metallic, and roughness metadata through
  existing upload/residency, with glTF factor defaults, scene-material
  override precedence, unchanged vertex-byte accounting, and controlled
  direct-light evidence;
- service-owned asset admission, explicit single-item CPU/GPU processing,
  aggregate residency status, and exact-hash post-recovery rehydration;
- explicit whole-content-hash asset eviction across queued sources, decoded CPU
  state, pending uploads, resident meshes, and shared textures, with exact
  released-resource outcomes, stable unrelated FIFO work, idempotent absence,
  backend-safe submitted-frame completion, and logically neutral rehydration;
- optional monotonic oldest-pending age in aggregate command, observation,
  asset-import, and renderer-upload status, with deterministic duplicate,
  supersession, rejection, processing, eviction, delivery, and saturation
  semantics and no payload or durable-state exposure;
- local typed service and unattended room/table/light/camera scenario;
- complete verified in-memory local-service restoration with retained replay,
  logical state, idempotency, renderer revision, and frame continuity;
- deterministic bounded version-one recovery-point envelopes that bind replay
  bytes and frame continuity with typed validation and SHA-256 corruption
  detection before payload allocation;
- exact-revision replay prefixes and fresh-service historical recovery forks
  that preserve the source service and carry its current next frame identity;
- quiescent in-place historical local-service revert through a fully restored
  replacement, with typed blockers, explicit cache/asset clearing, and frame,
  replay, idempotency, and branch-continuation evidence;
- explicit create-new local recovery files with pre-write envelope validation,
  write/sync failure cleanup evidence, bounded regular-file loading, path-
  redacted errors, and complete persisted restoration continuation;
- separate immutable exact-hash asset-source files with pre-I/O size and
  identity checks, shared create-new/sync/cleanup guarantees, bounded
  regular-file loading, and explicit restart rehydration evidence;
- public-repository safeguards, threat model, failure/recovery matrix,
  controlled compatibility/performance baseline, and source-first candidate
  checklist.

### Changed

- `GatewayQueueStats`, `LocalServiceStatus`, `AssetStoreStats`, and
  `RendererAssetStats` add optional elapsed-microsecond age fields. This is an
  additive runtime contract and a source-breaking change for exhaustive public
  struct construction in the still-unpublished `0.0.0` workspace; no version
  or release action was taken;

- active-light color now honors the required public metallic and roughness
  fields instead of applying diffuse-only base-color modulation. The canonical
  Point-lit table center is now `#371e0bff` on the validated profile while
  revision, identity, visibility, depth, normals, and replay remain intact;
- accepted GLB metallic and roughness factors now reach direct lighting when
  no explicit scene material is present. `AssetMaterial` and
  `AssetUploadJob::material` are additive APIs; `base_color` remains available,
  and the unpublished `0.0.0` workspace receives no release action;
- `AssetVertex` now requires a public primary-coordinate field and
  `AssetUploadJob::byte_len` accounts 32 rather than 24 bytes per expanded
  vertex. Missing, built-in, and proxy coordinates are exact zero. This is a
  source-breaking Rust API and capacity-planning change in the still-unpublished
  `0.0.0` workspace; no version or release action was taken;
- `AssetUploadJob` can now carry one immutable shared `AssetTexture`, renderer
  asset statistics expose separate pending/resident texture counts and bytes,
  and `RendererConfig` adds texture dimension/byte/count limits. The imported
  texture is disabled by an explicit scene material. These are additive and
  source-breaking exhaustive-enum/configuration changes in the unpublished
  `0.0.0` workspace; no version or release action was taken;
- the fixed built-in cuboid now uses 12 outward counter-clockwise triangles and
  exact axis-aligned exterior normals while preserving its 36 vertices,
  32-byte interleaved layout, 1,152-byte initialization payload, extents, and
  no-culling pipeline. Cuboid normal observations and diffuse color
  intentionally replace the prior inward-facing result; the canonical
  Point-lit table center is now positive. The unpublished `0.0.0` workspace
  receives no version or release action;
- `RendererError` now includes the source-breaking
  `PointLightCapacityExceeded` variant for exhaustive Rust matches. The
  workspace remains unpublished at `0.0.0`; no version or release action was
  taken.
- `AssetVertex` now requires a public unit-normal field and
  `AssetUploadJob::byte_len` accounts 24 rather than 12 bytes per expanded
  vertex. This is a source-breaking Rust API and capacity-planning change in
  the still-unpublished `0.0.0` workspace; no version or release action was
  taken.

### Known limitations

- no supported release, stable crates.io API, remote transport, authentication,
  automatic persistence/startup, snapshot retention, shared memory, model
  integration, deployment, or production SLA;
- the validated full-runtime profile is currently Windows 11 x86_64 with a
  Vulkan discrete GPU; other runtime platforms/backends remain unverified;
- renderer materials support base color plus bounded direct metallic-roughness
  response for directional and point lights; configurable point
  range/radius/cutoff, spot lights, ambient/image-based lighting, shadows,
  emissive, HDR/tone mapping, gamma conversion, and lighting configuration are
  not implemented.
  Normal output is quantized and the imported subset supports numeric base
  color, metallic, and roughness but has no normal maps, emissive/alpha modes,
  or tangent space; the GLB subset samples one shared embedded PNG base-color
  texture but excludes external/data images, JPEG, explicit samplers, additional
  texture roles, compression, scene traversal, and most vertex attributes.
  Built-in geometry supports
  cuboids, fixed centered XY planes, and fixed centered unit-diameter spheres;
  configurable subdivisions, plane thickness, generated coordinate mappings,
  and two-sided normal policy remain unsupported;
- recovery points and recovery files do not include queued commands,
  observations, asset bytes, or residency; exact-hash asset sources can be
  stored only as separate caller-mapped files. Files are create-new only and
  provide no automatic startup, overwrite, directory-sync guarantee,
  encryption, authentication, or key management; there is no asset
  discovery/catalog, retention/eviction, bundle, or automatic rehydration.
  In-place revert requires drained transient work, clears asset residency, and
  supplies no automatic rollback or freshness policy;
  future frame identity across concurrently live branches is
  caller-coordinated; device-loss recreation, crash-atomic latest pointers,
  binary packaging, signing, provenance, and automated release publication are
  not implemented.
