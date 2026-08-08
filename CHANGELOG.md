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
- a separate `cogniform-compilation` crate with schema-version-one compiler
  outcomes, explicit non-zero report limits, exact canonical LF JSON, bounded
  pre-decode and output allocation, strict entry roles/order/uniqueness, and
  exact compiled-versus-unresolved patch/revision invariants without execution
  or I/O authority;
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
- CPU-only aggregate recovery inspection plus a read-only
  `inspect-recovery <path>` CLI command that reuses the complete restoration
  preflight and redacts paths and replay payloads;
- deterministic compact schema-version-one JSON for opt-in recovery inspection,
  with exact fields/types/order, unchanged human output, pre-output validation,
  empty failure stdout, and a `--` escape for the reserved `--json` filename;
- fixed-layout compact schema-version-one JSON for the controlled CPU world
  measurement, with fixed integer-nanosecond distributions, exact
  fields/types/order, explicit informational-only status, unchanged human
  structure and debug warning, and complete preparation before stdout;
- fixed-layout compact schema-version-one JSON for the canonical unattended
  scenario, with exact adapter/revision/observation/identity/pixel/replay
  fields, unchanged 19-line human output, pre-GPU argument rejection, and
  complete scenario plus serialization before stdout;
- CPU-only `inspect-asset <content-hash> <path>` for bounded read-only
  verification of one caller-mapped immutable asset source, with exact
  lowercase hash parsing, aggregate hash/byte output, file immutability, and
  path/payload-redacted failures before any decode or GPU work;
- deterministic compact schema-version-one JSON for opt-in asset-source
  inspection, with exact fields/types/order, unchanged human output,
  complete verification and serialization before stdout, option-like path
  handling, and empty failure stdout;
- a separate `cogniform-observation` crate with a bounded deterministic
  version-one binary envelope for all five owned observation payload kinds,
  fixed big-endian layouts, strict canonical values and counts, typed failures,
  and SHA-256 binding to canonical causal metadata before decoded allocation;
- a separate `cogniform-local-transport` crate with fixed version-one control
  and observation framing over caller-owned synchronous streams, independent
  control/bulk/total limits enforced from a stack-read header before body
  allocation, exact metadata/payload validation, deterministic short and
  interrupted I/O behavior, and payload-redacted failures;
- a separate `cogniform-local-session` crate with canonical LF-terminated
  schema-version-one client/server hello, patch admission/completion,
  exact-revision query/observation, stable failure, and close control messages;
  outer-only correlation; bounded pre-decode nesting/bytes and bounded output;
  strict direction/unknown/canonical/nested-value validation; and no endpoint
  or service-execution authority;
- a separate `cogniform-local-executor` crate that owns one quiescent local
  service; negotiates peer, local-frame, and service bounds; enforces one hello,
  active, quiescent-close, and terminal states; maps patch/query/observation
  work with exact bounded correlation and deterministic one-command advance;
  returns at most two validated frames per call; and creates no endpoint,
  thread, timer, process, or automatic polling loop;
- `cogniform-cli serve-stdio`, a fixed `default-local-64x64` half-duplex binary
  composition root over inherited redirected stdin/stdout, with pre-adapter
  argument/terminal/first-EOF handling, immediate post-hello negotiated frame
  policy, individual frame flush, 2 millisecond capped polling, a 15 second
  live-operation deadline, fatal service-frame handling, and stable redacted
  diagnostics;
- separate immutable exact-hash asset-source files with pre-I/O size and
  identity checks, shared create-new/sync/cleanup guarantees, bounded
  regular-file loading, and explicit restart rehydration evidence;
- public-repository safeguards, threat model, failure/recovery matrix,
  controlled compatibility/performance baseline, and source-first candidate
  checklist.

### Changed

- compiler decision, unresolved, and result values move to
  `cogniform-compilation`; `cogniform-compiler` re-exports their original names,
  derives report limits from its existing runtime limits, and validates every
  completed result. `CompilationResult` gains mandatory schema version and
  `CompilerConfig` gains explicit result limits, so exhaustive construction is
  source-breaking in the unpublished `0.0.0` workspace;
  `CompileError::InvalidCompilationResult` likewise changes exhaustive error
  matching. `ScenePatch` exposes read-only aggregate text and logical
  size measurements for exact enclosing-value accounting. `Cargo.lock` gains
  only the new local package edge; no external package, version, checksum,
  vendor source, deterministic normalization, stable ID, normalized patch
  bytes, gateway behavior, world/replay state, rendered output, version, or
  release action changed;

- `cogniform-cli` now directly reuses the workspace-local local-transport,
  local-session, and local-executor crates. `Cargo.lock` gains only those local
  package edges; no external package, version, checksum, or vendor source
  changed;

- `cogniform-engine` and `LocalService` add correlated observation-delivery
  polling that retains `ObservationId` on request-specific asynchronous
  failure, while the existing observation polling API preserves its prior
  result shape. `cogniform-local-session` also exposes field-wise limit
  intersection and exact effective frame-policy construction; these are
  additive surfaces in the unpublished `0.0.0` workspace;

- `ObservationRequest` moves to `cogniform-protocol` with an engine re-export
  for source compatibility and adds mandatory schema version plus exact scene
  revision. Exhaustive construction is source-breaking in the unpublished
  `0.0.0` workspace; revision mismatch now rejects before observation capacity
  or renderer work, and no version or release action was taken;

- `cogniform-engine` adds the aggregate `RecoveryInspection` value and
  `inspect_recovery_point`, while the CLI composition root now depends on the
  existing storage crate. These are additive surfaces in the still-unpublished
  `0.0.0` workspace; no version or release action was taken;
- `cogniform-cli` now directly reuses the existing exact-pinned vendored
  `serde` and `serde_json` packages for its private schema-v1 inspection view;
  no package, version, engine/protocol schema, or recovery-file format changed;
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
- compilation results are available only as typed or canonical in-process
  values; the current local-session/stdio schema does not submit imagination or
  carry compilation outcomes;
- `serve-stdio` supports one inherited-stream client, one fixed 64x64 local
  service, half-duplex patch/query/observation work, fixed polling/deadline, and
  negotiated close only. It creates no process or endpoint and supplies no
  confidentiality, authorization, freshness, replay policy, cancellation,
  resynchronization, full duplex, multi-client, daemon, or remote guarantee;
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
  Offline inspection uses one fixed profile. Its optional JSON is CLI schema
  version one and has no JSON input, broader diagnostics contract,
  authenticity/freshness decision, asset validation, or GPU readiness claim;
  aggregate hashes and counts can still be sensitive.
  Controlled measurement JSON is also CLI schema version one and has no
  threshold, baseline replacement, hardware identity, arbitrary fixture/sample
  selection, or automatic upload; timing distributions can still expose local
  performance characteristics.
  Canonical scenario JSON is likewise CLI schema version one and has no JSON
  input, scenario/profile/adapter selection, performance threshold, automatic
  upload, exporter, or additional support claim; adapter identity and exact run
  evidence can fingerprint or correlate the local host.
  Asset-source inspection has optional CLI schema-version-one JSON and proves
  bounded byte identity only, not format validity, renderability, authenticity,
  freshness, recovery association, or authorization; hash values can still
  correlate private content.
  Observation payload envelopes are in-memory corruption-detection values,
  not authenticated or encrypted values. Local stream framing supplies a
  pre-buffer bound over caller-owned `Read`/`Write` values, but neither codec
  creates an endpoint or supplies operation schemas, a session, authorization,
  confidentiality, rate policy, resynchronization, shared memory, compression,
  retention, or automatic delivery.
  In-place revert requires drained transient work, clears asset residency, and
  supplies no automatic rollback or freshness policy;
  future frame identity across concurrently live branches is
  caller-coordinated; device-loss recreation, crash-atomic latest pointers,
  binary packaging, signing, provenance, and automated release publication are
  not implemented.
