# Changelog

All notable project changes will be recorded here. Cogniform has no published
or supported release yet. The workspace now identifies the unpublished
source-only candidate as `0.1.0-rc.1`; every package remains non-publishable.

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
  indexed retention, a preserved 32-byte position/normal/primary-coordinate
  vertex prefix, zero defaults, and shader input location 2;
- bounded shared embedded PNG GLB base-color, metallic-roughness, and normal texture roles, with strict static
  8-bit RGB/RGBA decode, independent CPU/GPU accounting, explicit unique
  role upload, fixed repeat/linear sRGB/linear sampling, white/neutral fallbacks,
  factor/override semantics, and controlled orientation plus rehydration evidence;
- optional finite non-zero same-count f32 `TANGENT` `VEC4`, exact handedness,
  finite normal scale, exact 48-byte expanded vertex accounting, atomic dual-
  role GPU reservation, transform-safe TBN direct-light shading, and unchanged
  geometric-normal observations;
- optional packed glTF `metallicRoughnessTexture` through the existing bounded
  embedded PNG path, with unique-image CPU accounting, atomic zero-to-three
  role GPU reservation, linear green/blue factor multiplication for
  directional and point direct lighting, factor-one fallback, ignored red and
  alpha, scene-override suppression, and unchanged unlit/non-color outputs;
- optional core glTF `emissiveFactor` as exactly three finite unit-bounded
  linear channels, with zero defaults, unchanged asset accounting, explicit
  scene-material suppression, bounded post-response RGB addition, preserved
  alpha/non-color observations, and no light, texture, HDR, or exposure authority;
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
  view input, exact unlit compatibility, a fixed 496-byte draw uniform with a
  prefix-compatible emissive append, and
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
- compatible local-session schema version two with explicit compilation-result
  limit negotiation, bounded imagination submission, every gateway admission
  outcome, compiled or unresolved completion roles, exact retained replay, and
  unchanged schema-version-one canonical bytes;
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
- a separate `cogniform-mcp` crate and `cogniform-cli serve-mcp-stdio`
  composition for stable MCP `2025-11-25`, with exactly four ordered tools for
  exact-revision scene query, idempotent imagination submission, bounded
  direct atomic patch application, and exact-revision observation; one
  latest-value canonical `COGOBS01` resource with exact-URI base64 read,
  atomic replacement, and failure preservation; one active request plus one
  decoded pending message, exact matching pre-response cancellation with
  response suppression and terminal no-later-dispatch semantics, cooperative
  observation polling with prior-resource preservation, and response-through-
  flush handling for wrong/missing/late cancellation; incremental
  newline byte/nesting preflight; bounded encode-before-write output; one
  serialized lazily created local service; exact compilation, receipt, and
  retained-replay validation; official-client conformance; and no listener,
  socket, credential store, model call, or remote authority;
- separate immutable exact-hash asset-source files with pre-I/O size and
  identity checks, shared create-new/sync/cleanup guarantees, bounded
  regular-file loading, and explicit restart rehydration evidence;
- public-repository safeguards, threat model, failure/recovery matrix,
  controlled compatibility/performance baseline, and source-first candidate
  checklist;
- a standard-library-only source-candidate preparation and verification tool
  that binds a clean direct annotated tag to one bounded deterministic
  uncompressed Git tar and exact SHA-256 sidecar outside repository state,
  then proves raw PAX/termination, portable inventory, Git blob identities,
  fixed metadata, mandatory offline source content, reusable public-content
  rules, and fail-closed cleanup without creating a tag, changing a version,
  opening a network connection, uploading, or publishing;
- a standard-library-only package-policy check with disposable negative
  fixtures for the complete workspace member inventory, inherited shared
  version, non-publishable packages, exact path-bound first-party
  dependencies, and source-less lockfile entries;
- an immutable source-release and support contract that pins the future
  project-owned tar/checksum names, draft-first release order, six separate
  live authority gates, exact release/asset/SHA-256 consumer verification, and
  a latest-published-candidate-only security-update lifetime without enabling
  a repository setting or publishing anything; and
- tracked `0.1.0-rc.1` source-candidate notes that name the intended local
  audience, validated profile, limitations, and the still-separate tag,
  archive, immutable-release, upload, and publication gates.

### Changed

- CF057 adds the additive `AssetMaterial::emissive` accessor and appends one
  private zero-padded emissive vector to the renderer draw uniform, growing it
  from 480 to 496 bytes while preserving the prior prefix. The existing public
  `AssetMaterial::new` signature and `const` behavior remain unchanged with a
  zero-emission default. No vertex, texture, logical scene, observation,
  protocol, dependency, package, version, tag, release-asset, workflow, or release
  contract changed;

- CF056 adds the additive
  `AssetMaterial::has_metallic_roughness_texture` and
  `AssetUploadJob::metallic_roughness_texture` accessors. Asset/renderer
  texture counts now admit zero to three role-separated values, while shared
  source images remain counted once in CPU decoded bytes. No vertex/uniform,
  logical scene, observation, protocol, dependency, package, version, tag,
  asset, workflow, or release contract changed;

- CF055 expands `AssetVertex` and `ASSET_VERTEX_BYTES` from the prior 32-byte
  position/normal/primary-coordinate layout to a prefix-compatible 48-byte
  layout with required tangent storage. It adds normal-texture material/upload
  accessors, `AssetDiagnosticCode::InvalidTangent`, and
  `RenderTargetKind::AssetNormal`; texture outcome/statistics now count unique
  content-hash-and-role GPU resources. The vertex field and exhaustive renderer
  enum variant are source-breaking in the still-unpublished `0.1.0-rc.1`
  candidate workspace. No package version, dependency, tag, asset, or release
  action changed;

- the bounded MCP stdio adapter now serves exact MCP `2026-07-28`
  `server/discover` and self-contained requests beside its byte-compatible MCP
  `2025-11-25` initialize lifecycle. One Cogniform-owned era gate validates
  every modern request, prevents lifecycle switching, retains existing
  identified legacy opening errors, and rejects client response/error
  directions through the existing redacted transport category. Modern
  discovery advertises only tools and resources; supported successes carry
  `resultType` and informational server identity, while discovery, list, and
  read results use zero-lifetime private cache hints. The exact four tools, one
  latest resource, cancellation/backpressure behavior, dependencies, local
  trusted-parent authority, workspace version, deployment, and release state
  are unchanged. `MCP_MODERN_PROTOCOL_VERSION` is an additive public constant;
  `MCP_PROTOCOL_VERSION` remains the legacy initialization revision;

- the shared workspace version, all fifteen exact first-party workspace
  dependency requirements, and all sixteen first-party lockfile package
  entries now use `0.1.0-rc.1`. Every member still inherits the shared version
  and retains `publish = false`; this is source-candidate identity only and
  creates no tag, archive, crates.io package, supported API line, deployment,
  or release;

- every MCP tool now advertises a closed, mutually exclusive success/error
  output schema. Query and imagination discovery now includes the complete
  stable error vocabulary already emitted by their unchanged runtime paths,
  correcting the prior success-only metadata. Initialization also returns one
  exact 508-byte instruction covering fresh and exact revisions, semantic versus
  direct changes, exact retries, camera/resource flow, serialized calls, and
  loss-of-trust outcomes. Protocol version, tool execution, core types,
  dependencies, authority, deployment, workspace version, and release state are
  unchanged;

- the isolated MCP adapter updates its exact-pinned official Rust SDK from
  `rmcp` 2.2.0 to 3.1.2 while keeping stable MCP `2025-11-25` as its only
  advertised and accepted revision. `server/discover`, Tasks, extensions,
  per-request `2026-07-28`, and newer `resultType` output remain excluded. The
  regenerated graph removes `async-trait` and adds SDK-required `uuid` 1.24.0,
  `getrandom` 0.4.3, and target-only `r-efi` 6.0.0; other external versions and
  Tokio features are unchanged. Exact tool/resource shapes, transport bounds,
  local-service authority, CLI behavior, public Cogniform Rust types,
  deployment, workspace version, and release state are unchanged;

- the MCP server now advertises resources without subscription or list-change
  support and appends `cogniform.observe_scene` after the existing three tools.
  `OBSERVE_SCENE_TOOL` is an additive public constant in the unpublished
  `0.0.0` workspace. The MCP crate gains one existing local workspace edge to
  `cogniform-observation`, and CLI tests gain the same development edge;
  `Cargo.lock` changes only those local package dependency arrays. External
  packages, versions, checksums, features, MCP version, existing tool shapes,
  and local-service authority are unchanged. Pipelined MCP requests now
  backpressure until the prior bounded response is flushed instead of creating
  concurrent SDK handler/response work. Deployment, version, and release state are
  unchanged;

- the MCP tool list appends `cogniform.apply_patch` after the existing two
  tools. It accepts one complete core `ScenePatch` through `LocalService`,
  validates exact receipt causality and retained replay, and adds the public
  `APPLY_PATCH_TOOL` constant plus the engine-owned
  `LocalServiceError::is_patch_rejected_without_mutation` classifier in the
  unpublished `0.0.0` workspace. Existing
  query/imagination shapes and behavior, MCP version, external dependencies,
  `Cargo.lock`, transport, trust boundary, deployment, version, and release
  state are unchanged;

- the exact-pinned vendored dependency graph adds `rmcp` 2.2.0, `tokio` 1.53.1,
  and their restricted server/runtime support graph for the isolated MCP
  adapter. HTTP, client, OAuth, TLS, process, and built-in stdio features are
  disabled in production; the adapter supplies its own bounded stdio transport.
  The reviewed `rmcp` build script can change Git hooks only when an unapproved
  repository-root `.githooks` directory exists, which public-tree policy
  excludes. `McpServerConfig`, `McpTransportLimits`, `McpServeError`, and
  `TransportFailureKind` are additive public APIs in the unpublished `0.0.0`
  workspace; no existing local-session wire value, CLI command behavior,
  deployment, version, or release action changed;

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
- `ClientHello`, `ServerHello`, local-session message enums,
  `LocalExecutorConfig`, and `LocalExecutorStatus` gain version-two compilation
  and imagination fields/variants. This is source-breaking for exhaustive Rust
  construction or matching in the unpublished `0.0.0` workspace, while all
  version-one wire fixtures remain exact. `LocalSessionValidationKind` adds
  validation cases and `CompileError` adds `InvalidCompilationEncoding`, so
  exhaustive matching on those public enums is also source-breaking. The
  compiler now enforces negotiated canonical encoded-byte and nesting limits
  before a normalized patch can be applied. Only workspace-local dependency
  edges changed; no external package, checksum, version, deployment, or
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
  now-prefix-compatible interleaved layout, extents, and
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
- `serve-stdio` supports one inherited-stream client, one fixed 64x64 local
  service, half-duplex patch/imagination/query/observation work, fixed
  polling/deadline, and
  negotiated close only. It creates no process or endpoint and supplies no
  confidentiality, authorization, freshness, replay policy, cancellation,
  resynchronization, full duplex, multi-client, daemon, or remote guarantee;
- `serve-mcp-stdio` supports one parent-owned inherited-stream client pinned to
  exact MCP `2025-11-25` or exact MCP `2026-07-28`, four fixed tools, one
  in-memory latest-value observation resource, serialized calls, and one lazy
  64x64 local service. Its observation
  poll has a fixed 15 second deadline and exact matching active cancellation is
  process-terminal before response writing, but it supplies no general
  operation deadline, rollback, effect receipt, reusable cancellation,
  authentication, authorization, confidentiality, freshness/rate/tenancy
  policy, resynchronization, multi-client service, daemon, or remote guarantee;
- the validated full-runtime profile is currently Windows 11 x86_64 with a
  Vulkan discrete GPU; other runtime platforms/backends remain unverified;
- renderer materials support base color plus bounded direct metallic-roughness
  response for directional and point lights; configurable point
  range/radius/cutoff, spot lights, ambient/image-based lighting, shadows,
  emissive textures/strength/cross-surface illumination, HDR/tone mapping,
  gamma conversion, and lighting configuration are not implemented.
  Normal output is quantized and the imported subset supports numeric base
  color, metallic, roughness, and one source-tangent normal map, but no
  generated tangents, emissive textures/strength, alpha modes, or other
  material texture roles;
  the GLB subset samples at most one shared embedded PNG per base-color,
  metallic-roughness, or normal role but excludes external/data images, JPEG,
  explicit samplers, compression,
  scene traversal, and most vertex attributes. Normal mapping affects direct
  light only and never replaces the geometric-normal observation.
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
