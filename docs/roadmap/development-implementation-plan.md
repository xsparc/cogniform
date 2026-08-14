# Cogniform Development Implementation Plan

Status: proposed dependency-ordered delivery plan derived from the supplied research on 2026-08-02. Presence here does not approve implementation; approval must be explicit in an issue, pull request, or maintainer conversation.

## 1. Delivery strategy

Cogniform will be built through small pull requests that each prove one architectural claim. The sequence retires semantic and determinism risk before expensive rendering breadth, keeps optional transports/models out of the critical path, and gives contributors a usable test boundary early.

Rules for every slice:

- one Ready/In Progress task at a time;
- no unrelated dependency or public-contract expansion;
- targeted tests first, then the configured broader checks;
- no unmeasured performance claims;
- no paid calls, deployment, release publication, or GPU larger runner;
- one reviewable branch and pull request per task unless a task is explicitly split;
- merge dependency order before starting a branch whose base would otherwise be ambiguous.

## 2. Pull-request sequence

### PR 1 - CF000: OSS and Rust workspace foundation

Outcome: a contributor can clone, build, test, lint, and understand the intended boundaries with a minimal offline toolchain.

Scope:

- Cargo workspace and pinned Rust toolchain compatible with the selected crate set;
- minimal crate skeletons for protocol, world, replay, renderer, engine, and CLI without pretending later behavior exists;
- workspace lint/unsafe/dependency rules;
- README, CONTRIBUTING, CODE_OF_CONDUCT, SECURITY, and ADR index;
- one cost-conscious Linux PR workflow with cancellation, timeout, no routine artifacts, and no paid services;
- dependency/license policy that preserves Apache-2.0 compatibility.

Non-scope: ECS behavior, GPU initialization, public network schemas, benchmarks, release packaging.

Gate: local and CI formatting, Clippy, tests, and docs pass from a clean checkout; the GitHub Actions definition is linted; crate dependency direction is documented and skeleton code contains no false implementation claims.

### PR 2 - CF001: Bounded public core contracts

Outcome: stable typed contracts express IDs, revisions, limits, operations, patches, receipts, queue semantics, and observation metadata without leaking ECS/GPU/transport types.

Gate: serialization fixtures are deterministic, invalid values and configured limits fail clearly, idempotency/transaction fields are mandatory, and public types contain no backend handles.

### PR 3 - CF002: Atomic deterministic world core

Outcome: an authoritative `hecs` world applies create/delete/component patches against stable IDs and revisions atomically.

Gate: invalid multi-operation patches leave the revision, stable-ID index, and logical snapshot unchanged; stable IDs survive ECS slot reuse; repeated idempotency keys do not duplicate effects; randomized operation/property tests preserve invariants.

### PR 4 - CF003: Hierarchy, transforms, canonical hash, and replay

Outcome: deterministic parent/child updates, generation-based world transforms, canonical scene hashes, and an integrity-checked replay log.

Gate: cycles/depth violations reject atomically; sparse propagation touches changed branches; repeated replay yields the same logical hash; corrupt/reordered log entries are detected.

Implemented contract: stable-ID hierarchy indexes, copy-on-write topology
preflight, parent-before-child derived matrices, versioned SHA-256 logical
hashes, and bounded hash-chained replay with verified-prefix tail recovery. See
[ADR 0004](../adr/0004-stable-hierarchy-canonical-hash-and-replay-chain.md).

### PR 5 - CF004: Headless primitive renderer

Outcome: negotiated `wgpu` initialization renders a deterministic primitive reference scene to offscreen color, depth, and entity-ID targets.

Gate: no visible window is required; exact entity-ID probes and tolerant depth/color probes pass on the declared reference adapter; unsupported adapters return structured capability diagnostics.

Implemented contract: exact-pinned `wgpu` initialization with no surface,
bounded offscreen `Rgba8Unorm`/`Depth32Float`/`R32Uint` targets, a built-in cube
and camera, separate submission/readback, backend-neutral adapter diagnostics,
and exact/tolerant probes on the declared DX12/Vulkan baseline. See
[ADR 0005](../adr/0005-bounded-headless-wgpu-baseline.md).

### PR 6 - CF005: Incremental extraction and revision-linked observation

Outcome: compact changed-record extraction connects world revisions to rendered frame metadata and asynchronous observations without cloning the world or blocking rendering.

Gate: sparse edits update only affected records; every observation reports its source revision/frame; bounded readback pools degrade explicitly under pressure.

Implemented contract: bounded coalesced stable-ID extraction, ordered
generation/base-revision packets, atomic renderer-owned scene updates,
frame-local compact identity mapping, extracted cuboid/camera rendering, fixed
readback leases, and globally bounded asynchronous color/depth/ID/visibility
observations. See [ADR 0006](../adr/0006-coalesced-extraction-and-bounded-observations.md).

### PR 7 - CF006: Bounded agent gateway and imagination compiler

Outcome: a local in-process/loopback gateway accepts explicit patches and a minimal deterministic primitive imagination, enforces budgets and queue semantics, and returns explanations/receipts.

Gate: `MustApply`, `LatestWins`, and `BestEffort` remain bounded; same imagination/scene/seed produces the same normalized patch; substitutions and unresolved constraints are structured.

Implemented contract: canonical bounded imagination/query values, a pure
seeded primitive compiler with stable-key normalization and structured
decisions, exact-revision queries, a fixed command queue implementing all three
delivery semantics, and bounded queued/accepted idempotency routing. See
[ADR 0007](../adr/0007-pure-imagination-compiler-and-bounded-local-gateway.md).

### PR 8 - CF007: Content-addressed assets and built-in procedures

Outcome: immutable asset records, bounded asynchronous glTF/GLB import for an approved subset, and deterministic built-in procedures integrate with world and renderer.

Gate: content-hash mismatch and decompression/size limits fail closed; unsupported glTF features produce diagnostics; asset decode never runs on world/render critical paths; seeded procedures replay exactly.

Implemented contract: exact SHA-256 source admission, retained typed asset
states and diagnostics, an explicit bounded GLB decode queue, immutable logical
mesh references, separately reserved renderer upload/residency, and pure seeded
cuboid-grid procedures that emit ordinary atomic patches. See
[ADR 0008](../adr/0008-content-addressed-assets-and-pure-built-in-procedures.md)
and the [GLB asset guide](../assets/glb-subset.md).

### PR 9 - CF008: Local headless service and canonical MVP scenario

Outcome: a local CLI/service runs the complete room/table/light/camera flow, returns color/entity-ID observation metadata and visibility, and replays to the same hash.

Gate: the six-step MVP scenario passes unattended with no external service, no visible window, and bounded queues. The protocol surface and known limitations are documented.

Implemented contract: the engine records every newly accepted patch through a
bounded `RecordedWorld`; a local typed service composes command admission,
caller-driven processing, exact queries, revision-linked observations, status,
and replay verification without a socket or external side effect. The CLI and
controlled integration test share the six-step canonical scenario. See
[ADR 0009](../adr/0009-recorded-engine-and-local-typed-service.md), the
[local service guide](../protocol/local-service.md), and the
[canonical scenario guide](../getting-started/canonical-scenario.md).

### PR 10 - CF009: MVP hardening and release-candidate evidence

Outcome: threat model, performance fixtures, compatibility evidence, failure injection, packaging decision, and contributor/release documentation support an honest first release candidate.

Gate: correctness/security matrices pass; measured budgets are reported without silent threshold changes; supported platforms are named; release remains a separate explicit action.

Implemented contract: the local single-user threat model and failure/recovery
matrix name every current trust boundary and residual risk; every-byte replay
corruption injection proves verified-prefix containment; the versioned
`world-create-empty-v1` command records controlled release-mode distributions;
guarded GPU retirement keeps renderer destruction off a pending readback's
bounded caller path; and the compatibility baseline names one validated
Windows/Vulkan runtime profile without generalizing it to untested backends.
[ADR 0010](../adr/0010-source-first-release-profile.md)
selects source-first candidate packaging while leaving all publication manual
and separately authorized. See the
[validation baseline](../operations/validation-baseline.md),
[threat model](../threat-model/mvp.md),
[failure guide](../operations/failure-and-recovery.md), and
[release-candidate checklist](../release/release-candidate.md).

### PR 11 - CF011: Revision-linked surface-normal observations

Outcome: the headless renderer and local observation path expose quantized flat
world-space geometric normals with the same bounded lifecycle and exact
revision/frame/camera causality as existing image observations.

Gate: geometry pixels decode to finite unit normals within the documented
RGBA8 quantization tolerance; background pixels are explicitly absent; normals
follow source triangle winding; existing color/depth/entity-ID/visibility
contracts remain unchanged; the fourth readback buffer stays inside each fixed
lease; and unavailable attachment capabilities remain structured.

Implemented contract: schema-v1 metadata accepts the additive `normal` kind;
the local engine returns owned optional unit vectors; the renderer derives flat
geometric normals from rasterized world positions, writes signed directions to
an `Rgba8Unorm` attachment, and decodes and renormalizes them after readback.
The selected adapter must support three color attachments and twelve color
bytes per sample. Smooth imported normals, normal maps, tangent space,
higher-precision targets, remote encoding, and release publication remain
separate work. See
[ADR 0011](../adr/0011-quantized-world-space-normal-observations.md).

### PR 12 - CF012: Complete verified local-service restoration

Outcome: a caller can capture the bounded in-memory state needed to restore a
fresh local service without losing accepted-event, logical-world, renderer
revision, idempotency, or frame causality.

Gate: complete replay bytes and the source renderer's next frame identity
restore the same logical hash, revision, exact replay chain, query results, and
next observation frame; a prior patch remains idempotent; a new patch appends
the next revision; malformed, truncated, noncanonical, integrity-invalid,
over-limit, or frame-inconsistent input fails before GPU initialization; and
transient queues start empty.

Implemented contract: `EngineRecoveryPoint` captures complete replay bytes and
the next unreserved frame identity. Restoration verifies and replays the whole
stream before GPU initialization, rejects verified-prefix-only adoption,
synchronizes one final-state extraction into a fresh renderer, retains the log
for append continuation, and deliberately resets gateway and observation
queues. Storage, atomic persistence, assets, automatic startup, device
recreation, snapshots, revert, and log rotation remain caller or future
concerns. See
[ADR 0012](../adr/0012-complete-in-memory-service-restoration.md), the
[local service guide](../protocol/local-service.md), and the
[failure guide](../operations/failure-and-recovery.md).

### PR 13 - CF013: Versioned recovery-point envelope

Outcome: callers can preserve complete replay bytes and the next renderer frame
identity as one deterministic, bounded, integrity-protected portable value.

Gate: repeated encoding is byte-identical; exact decoding round-trips both
parts; header, version, configured bound, declared length, exact total length,
non-zero frame, and SHA-256 integrity fail closed before replay bytes are
copied; every one-byte corruption is rejected; and the decoded point completes
the CF012 restore/query/observe/idempotency/append scenario.

Implemented contract: `EngineRecoveryPoint` owns version-one envelope encode
and decode methods under an explicit `ReplayConfig`. The 52-byte fixed overhead
contains magic, version, frame, replay length, and digest; the payload remains
the complete independently verified replay stream. The digest detects
corruption but does not authenticate or encrypt the caller's data. Filesystem
I/O, atomic replacement, automatic startup, retention, snapshots, rollback
protection, assets, transient work, device recreation, transport, deployment,
and release remain outside the slice. See
[ADR 0013](../adr/0013-versioned-recovery-point-envelope.md), the
[determinism and replay guide](../architecture/determinism-and-replay.md), and
the [local service guide](../protocol/local-service.md).

### PR 14 - CF014: Exact-revision historical recovery forks

Outcome: a caller can select any retained exact revision and restore it as a
separate fresh-service branch without mutating the source or reusing a frame
identity issued before capture.

Gate: revision zero and every retained revision encode as complete deterministic
standalone replay streams; a newer revision returns a typed requested/latest
error; capture preserves source status, hash, and full replay bytes; the point
uses the source renderer's current next unreserved frame identity; and the
restored fork reproduces exact revision/hash/query state before continuing
observation and append causality.

Implemented contract: `ReplayLog::to_bytes_through_revision` creates one
complete exact-revision prefix. Engine and local-service capture wrap it in the
existing recovery point with the current source frame frontier, and the
existing restoration path validates and reconstructs a fresh branch. This is
not an in-place revert, live source swap, snapshot registry, retention policy,
branch manager, persistence layer, rollback-protection mechanism, asset
restore, transient-work migration, cross-branch frame allocation, or release
action. See
[ADR 0014](../adr/0014-exact-revision-historical-recovery-forks.md), the
[determinism and replay guide](../architecture/determinism-and-replay.md), and
the [local service guide](../protocol/local-service.md).

### PR 15 - CF015: Local-service asset resolution and rehydration

Outcome: the local typed service owns bounded asset ingestion and drives
renderer residency through narrow explicit methods, while logical recovery
preserves content references and requires exact-hash rehydration.

Gate: a hash mismatch consumes no service asset capacity; one explicit call
processes at most one CPU import or GPU upload; aggregate status exposes only
bounded counters; a checked triangle GLB renders with its exact stable entity
identity; fresh and restored services start with empty CPU/GPU asset state; a
restored logical reference fails with its exact missing mesh key until the same
hash is reimported and uploaded; and rehydration changes no world revision,
logical hash, or replay bytes.

Implemented contract: `LocalServiceConfig` bounds a service-owned
`AssetStore`; `LocalService` exposes source admission, import processing,
immutable records, upload admission/processing, and aggregate asset status;
and `CogniformEngine` forwards immutable upload jobs without exposing renderer
handles. No path performs implicit decoding, upload, external fetching, or
asset persistence. See
[ADR 0015](../adr/0015-service-owned-asset-resolution-and-rehydration.md), the
[GLB asset guide](../assets/glb-subset.md), and the
[local service guide](../protocol/local-service.md).

### PR 16 - CF016: Local-service built-in procedure composition

Outcome: the local typed service executes a supported pure built-in procedure
and admits its generated scene change through the ordinary patch lifecycle.

Gate: invalid procedure or text budgets fail before gateway admission; a 2x3
grid reports deterministic stable IDs without mutating before processing;
ordinary delivery, queue, and output-oriented idempotency semantics apply; the
processed entities are exactly queryable; live and replayed hashes match; and
exact resubmission before and after restoration adds no world revision or
replay entry.

Implemented contract: `LocalService::submit_procedure` executes under the
engine's active runtime limits and returns the generated entity IDs with an
ordinary `GatewayAdmission`. The gateway sees only the generated canonical
patch, and replay retains only that accepted patch and receipt. No procedure
command/response schema, ambient I/O, plugin/Wasm host, user code, background
scheduler, or new procedure kind is added. See
[ADR 0016](../adr/0016-service-procedure-composition-through-ordinary-patches.md),
the [gateway guide](../protocol/local-gateway-and-imagination.md), and the
[local service guide](../protocol/local-service.md).

### PR 17 - CF017: Quiescent in-place historical revert

Outcome: a live local service can move to an exact older retained revision by
building a complete restored replacement before an atomic swap.

Gate: equal/future targets and queued commands, observations, imports, or
uploads fail with typed bounded evidence and no state change; a successful
revert returns explicit removed-replay/cache/asset counts, restores exact
query/hash/replay/renderer state, preserves the source frame frontier, clears
transient and asset residency, records no lifecycle event, retains prefix
idempotency, and lets removed-tail keys form one ordinary new branch.

Implemented contract: `LocalService::revert_to_revision` reuses the accepted
exact-prefix and fresh-restoration paths under a privately retained validated
configuration. The old service is assigned over only after replacement
initialization succeeds. No persistence, automatic rollback, snapshot
registry, transient migration, asset preservation, authentication, transport,
device-loss recovery, or release action is added. See
[ADR 0017](../adr/0017-quiescent-live-revert-through-fresh-replacement.md),
the [replay guide](../architecture/determinism-and-replay.md), and the
[local service guide](../protocol/local-service.md).

### PR 18 - CF018: Immutable bounded local recovery files

Outcome: an operator can explicitly persist one complete recovery envelope to
a new local file and load it later without moving filesystem authority into the
engine, world, renderer, or replay domains.

Gate: encoding and bounds validation finish before the target is touched; an
existing target is never overwritten; complete writes are synchronized and
write/sync failures report whether partial cleanup succeeded; load accepts only
a regular non-symlink final path component, bounds metadata before allocation,
detects growth after that snapshot, and returns no recovery point until the
complete envelope digest validates. A controlled save/drop/load/restore path
must preserve exact query/hash/replay/frame state and continue observe/append
causality.

Implemented contract: `cogniform-storage::RecoveryFileStore` is an opt-in,
caller-driven service adapter over the public recovery envelope and replay
bounds. It creates no directories, chooses no paths, and provides no overwrite,
rotation, latest-pointer, automatic checkpoint/startup/rollback, encryption,
authentication, remote storage, asset/transient persistence, deployment, or
release action. See
[ADR 0018](../adr/0018-immutable-bounded-local-recovery-files.md), the
[recovery-file guide](../persistence/recovery-files.md), and the
[failure guide](../operations/failure-and-recovery.md).

### PR 19 - CF019: Immutable exact-hash asset source files

Outcome: an operator can explicitly persist one exact asset source to a
separate new local file and later load it by its expected logical hash for
caller-driven post-recovery rehydration.

Gate: source size and SHA-256 identity validate before the target is touched;
an existing target is never overwritten; complete writes are synchronized and
write/sync failures report partial cleanup; load accepts only a regular
non-symlink final path component, bounds metadata before allocation, detects
growth, and returns no bytes until the complete file matches the supplied
hash. A controlled save/drop/load/restore/rehydrate path must preserve exact
revision, logical hash, replay bytes, reference identity, and observation.

Implemented contract: `cogniform-storage::AssetFileStore` shares only private
bounded file mechanics with recovery storage. It does not bundle files,
discover content, map hashes to paths, decode/import/upload automatically, or
provide directories, overwrite, rename, deletion, catalogs, retention,
eviction, startup, encryption, authentication, remote storage, deployment, or
release action. See
[ADR 0019](../adr/0019-immutable-exact-hash-asset-source-files.md), the
[asset-file guide](../persistence/asset-files.md), and the
[GLB asset guide](../assets/glb-subset.md).

### PR 20 - CF020: Bounded imported vertex normals

Outcome: an approved GLB can carry finite smooth vertex normals through strict
decode, exact CPU/GPU accounting, model transformation, interpolation, and the
existing quantized world-space normal observation.

Gate: a primitive accepts exactly `POSITION` alone or `POSITION` plus a
same-count non-normalized f32 `VEC3` `NORMAL`; accepted directions normalize
deterministically and expand with the same index, while invalid ranges,
non-finite or zero values, count mismatches, and degenerate fallback triangles
reject without proxy adoption. Position-only geometry synthesizes one
winding-derived normal per expanded triangle. Admission reserves exactly 24
bytes per vertex before allocation, the renderer interleaves both attributes,
and controlled GPU tests prove inverse-transpose behavior under non-uniform
scale while preserving the position-only GLB and cube outputs.

Implemented contract: `AssetVertex` now carries position plus a unit normal;
the renderer transforms and interpolates that direction into the unchanged
signed RGBA8 observation target. UVs, tangents, textures, normal maps, material
lighting, alternate normal encodings, scene traversal, compression, automatic
asset work, persistence/catalog changes, transport, and release action remain
excluded. See [ADR 0020](../adr/0020-bounded-imported-vertex-normals.md), the
[GLB asset guide](../assets/glb-subset.md), and the
[renderer guide](../renderer/headless-reference-scene.md).

### PR 21 - CF021: Centered built-in plane rendering

Outcome: the already-public plane primitive renders through the same bounded
headless color, depth, stable-identity, and world-space-normal path as cuboids.

Gate: one immutable six-vertex buffer encodes a centered unit XY square as two
counter-clockwise positive-Z triangles in the existing 24-byte vertex layout;
all positive XYZ dimensions scale the model while local positions remain at
Z = 0. Scene preparation selects the exact cuboid or plane shape, an unavailable
asset honors its explicit primitive fallback, resident assets retain
precedence, and direct or fallback spheres preserve the typed unsupported
error. Unit tests prove layout, selection, dimensions, precedence, and failure;
a controlled adapter test proves plane color, finite depth, stable entity ID,
background, and tolerant positive-Z normal output.

Implemented contract: plane frames add no tessellation, upload job, dependency,
pipeline, or observation format. Sphere tessellation, subdivisions, UVs,
tangents, textures, two-sided lighting policy, collision, asset changes,
transport, persistence, and release action remain excluded. See
[ADR 0021](../adr/0021-centered-built-in-plane-rendering.md) and the
[renderer guide](../renderer/headless-reference-scene.md).

### PR 22 - CF022: Fixed built-in UV-sphere rendering

Outcome: the final already-public primitive shape renders through the bounded
headless color, depth, stable-identity, and world-space-normal path.

Gate: a centered unit-diameter sphere with a positive-Z polar axis uses 16
longitude sectors and 8 latitude bands. Its 224 outward counter-clockwise
triangles expand to 672 vertices with unit radial normals in the existing
24-byte layout. The exact 16,128-byte payload is generated once at renderer
initialization. Positive XYZ dimensions are bounding diameters; direct and
unavailable-asset fallback spheres select this mesh while resident assets keep
precedence. CPU tests prove topology, byte count, radius, winding, radial
normals, selection, precedence, and dimensions. A controlled adapter test
proves color, curved depth, stable identity, background, and smoothly changing
world-space normals.

Implemented contract: sphere frames add no tessellation, upload job, index
buffer, dependency, pipeline, protocol field, or observation format. Fixed
`f32` trigonometry is confined to renderer initialization and visual outputs
retain their tolerant contract. Configurable subdivisions, UV attributes,
tangents, textures, normal maps, LOD, collision, lighting, culling, batching,
asset changes, transport, persistence, and release action remain excluded. See
[ADR 0022](../adr/0022-fixed-built-in-uv-sphere-rendering.md) and the
[renderer guide](../renderer/headless-reference-scene.md).

### PR 23 - CF023: Bounded directional diffuse lighting

Outcome: the already-public directional light produces deterministic bounded
diffuse color through the existing headless render and observation path.

Gate: transformed local positive Z is the normalized surface-to-light
direction, matching negative-Z emission. Directional definitions are processed
in stable entity-ID order and limited to four per scene; zero-intensity
definitions count toward that limit but are inactive. A fifth definition or
an active degenerate direction fails before GPU submission. Active lights sum
clamped Lambert RGB contributions, clamp the result to the unit interval,
multiply material base RGB, and preserve alpha. No active directional light,
including a point-only scene, preserves exact unlit output. CPU tests prove
ordering, direction, inactive-kind behavior, errors, and the exact 304-byte
uniform; controlled GPU tests prove front/back diffuse response while retaining
identity, depth, normals, background, and every prior renderer output.

Implemented contract: four fixed directional-light slots extend the existing
per-draw bind group and pipeline. Point shading/attenuation, ambient,
metallic/roughness response, specular/PBR/IBL, shadows, emissive, textures,
normal maps, HDR/tone mapping, lighting configuration, culling, clustering,
asset work, transport, persistence, and release action remain excluded. See
[ADR 0023](../adr/0023-bounded-directional-diffuse-lighting.md) and the
[renderer guide](../renderer/headless-reference-scene.md).

### PR 24 - CF024: Bounded point diffuse lighting

Outcome: the remaining public point-light kind produces deterministic bounded
diffuse color through the existing headless render and observation path.

Gate: a Point source uses its extracted finite GPU-f32 world translation.
Definitions are processed in stable entity-ID order and limited to four per
scene independently of the directional limit; zero intensity counts but is
inactive. A fifth definition or out-of-range active position fails before GPU
submission. Non-coincident fragments use capped unit-distance inverse-square
attenuation and Lambert facing; exact coincidence or a finite-input derived
f32 squared-distance overflow contributes zero. Point and directional
contributions share one clamped RGB sum, multiply material base RGB, and
preserve alpha. With neither kind active, exact unlit output remains;
without active Point definitions, CF023 directional output remains exact. CPU
tests prove ordering, capacity, finite conversion, and the exact appended
448-byte uniform. Controlled GPU tests prove near/far/back-facing Point output
while retaining identity, depth, normals, background, prior renderer behavior,
and the canonical scenario.

Implemented contract: a point count and four fixed position/color-intensity
slots append to the existing directional per-draw bind group and pipeline.
Configurable range/cutoff/radius, spot lights, ambient, metallic/roughness
response, specular/PBR/IBL, shadows, emissive, textures, normal maps, HDR/tone
mapping, lighting configuration, culling, clustering, asset work, transport,
persistence, and release action remain excluded. See
[ADR 0024](../adr/0024-bounded-point-diffuse-lighting.md) and the
[renderer guide](../renderer/headless-reference-scene.md).

### PR 25 - CF025: Outward built-in cuboid winding

Outcome: built-in cuboid normals and diffuse lighting describe exterior rather
than interior faces.

Gate: the centered unit cuboid remains exactly six faces, 12 non-degenerate
triangles, 36 expanded vertices, 24 bytes per vertex, and one immutable
864-byte initialization payload. Every triangle winds counter-clockwise from
outside, each face has two triangles, and the synthesized normals are the six
exact axis-aligned exterior directions. The fixed reference projection reports
negative Z on its near face; the canonical camera observes the table's
positive-Z exterior and its Point source produces a positive tolerant color.
Unlit color, depth, coverage, exact identity, background, observation
causality, replay, and all non-cuboid geometry remain controlled.

Implemented contract: reverse only the faulty built-in cuboid triangle order.
Positions, face diagonals, extents, transforms, geometry selection, draw path,
pipeline, and no-culling policy remain unchanged. Imported triangles retain
their source winding. Asset rewriting, culling, two-sided normal/lighting
policy, geometry configuration, wider materials/PBR, textures, shadows,
protocol/world/hash/replay changes, transport, persistence, dependencies, CI
expansion, and release action remain excluded. See
[ADR 0025](../adr/0025-outward-built-in-cuboid-winding.md) and the
[renderer guide](../renderer/headless-reference-scene.md).

### PR 26 - CF026: Bounded direct metallic-roughness response

Outcome: the public metallic and perceptual-roughness material inputs produce
distinct bounded color through the existing directional and point path.

Gate: each prepared draw carries the selected camera's finite GPU-f32 world
translation and exact material values. Active lights use one documented direct
Cook-Torrance response with GGX distribution, Schlick-GGX Smith visibility,
Schlick Fresnel, dielectric `0.04` normal reflectance, metallic reflectance
blending, and an energy-conserving Lambert diffuse split. Roughness has a
`0.05` distribution floor only for numerical safety. Directional radiance,
point inverse-square attenuation, exact coincidence/derived-overflow handling,
stable ordering, and independent four-definition limits remain unchanged.
Output and per-light accumulation remain bounded in linear RGB; alpha is
unchanged. With neither kind active, exact base RGBA bypasses the BRDF.

Implemented contract: preserve the complete 448-byte CF024 draw-uniform prefix
and append zero-padded camera-position and metallic/roughness `vec4` slots for
an exact 480-byte layout. Missing materials retain the existing fallback color
with neutral `metallic = 0`, `roughness = 0.8`. Controlled adapter tests pin
dielectric, metallic, rough-metal, and exact unlit color while holding depth,
stable identity, normals, and background constant; all prior geometry, asset,
lighting, observation, and engine regressions remain controlled. Textures,
UVs, tangents, normal maps, IBL, ambient/emissive response, shadows, spot
lights, HDR, tone mapping, gamma changes, transparency, configurable lighting,
culling/clustering, schema/world/hash/replay changes, asset-format work,
transport, persistence, dependencies, CI expansion, deployment, and release
action remain excluded. See
[ADR 0026](../adr/0026-bounded-direct-metallic-roughness-response.md) and the
[renderer guide](../renderer/headless-reference-scene.md).

### PR 27 - CF027: Imported GLB metallic-roughness materials

Outcome: validated numeric GLB material factors reach the existing bounded
direct-light response without duplicating them in world state.

Gate: one typed immutable asset material retains linear base RGBA, metallic,
and perceptual roughness from the approved GLB subset through decoded mesh,
upload job, renderer residency, and scene preparation. Explicitly selected GLB
materials apply the glTF default of one for omitted factors; a primitive with
no material preserves Cogniform's existing `(0.8, 0.8, 0.8, 1.0)`, metallic
`0`, roughness `0.8` fallback, and the magenta proxy keeps the same neutral
numeric factors. Every value is unit-bounded before a record becomes ready.

Implemented contract: a resident asset supplies all three imported values only
when its entity has no `MaterialComponent`; an explicit scene material
overrides all three together. No active light preserves exact selected base
RGBA. Material metadata uses no vertex or GPU buffer bytes, so the 24-byte
expanded vertex, upload reservation, residency accounting, content hash,
explicit import/upload/rehydration, 480-byte draw uniform, bind group, shader,
pipeline, and causal observation contracts remain unchanged. CPU tests pin
retention, defaults, rejection, proxy, accounting, and override precedence; a
controlled adapter probe pins distinct imported and overridden direct color
while depth, stable identity, normals, background, and revision progression
remain fixed. Textures, images, samplers, UVs, tangents, normal maps,
emissive/alpha modes, image-based lighting, shadows, HDR, tone mapping,
schema/world/hash/replay changes, persistence, transport, dependencies, CI
expansion, deployment, and release action remain excluded. See
[ADR 0027](../adr/0027-imported-glb-metallic-roughness-materials.md) and the
[GLB guide](../assets/glb-subset.md).

### PR 28 - CF028: Bounded primary texture-coordinate residency

Outcome: one primary GLB coordinate set reaches renderer residency without
claiming texture support or changing any rendered observation.

Gate: an accepted primitive may add non-normalized finite f32 `VEC2`
`TEXCOORD_0` with exactly the position source count. Values outside the unit
interval are retained unchanged, and indexed expansion uses the same checked
source index as position and normal. The complete coordinate accessor is
validated before expanded allocation; non-finite values, count mismatches, and
invalid ranges reject without proxy, while syntactically valid unsupported
encodings remain typed and obey explicit proxy policy.

Implemented contract: decoded and renderer vertices use one exact 32-byte
position/normal/coordinate layout with shader location 2. Missing coordinates,
built-ins, and proxies use exact zero; cuboid, plane, and sphere payloads become
1,152, 192, and 21,504 bytes. The shader does not read the coordinate, and a
controlled adapter comparison pins every color, depth, stable-identity,
normal, and background sample across equivalent missing/present-coordinate
assets. Images, textures, samplers, `baseColorTexture`, transforms, other
coordinate sets or encodings, tangents, normal maps, shader sampling,
schema/world/hash/replay changes, persistence, transport, dependencies, CI
expansion, deployment, and release action remain excluded. See
[ADR 0028](../adr/0028-bounded-primary-texture-coordinates.md) and the
[GLB guide](../assets/glb-subset.md).

### PR 29 - CF029: Bounded embedded PNG base-color texture

Outcome: one observable embedded GLB base-color texture reaches the existing
causal unlit and direct-light paths without implicit work or schema changes.

Gate: accept at most one shared texture and one in-BIN `image/png` buffer view,
only through `baseColorTexture` texture zero, omitted/zero `texCoord`, omitted
sampler, and an approved `TEXCOORD_0`. Decode only static non-interlaced 8-bit
RGB/RGBA into immutable RGBA8 under dimension, pixel, decoder-working,
retained-byte, per-asset, and aggregate CPU limits. Malformed, truncated,
range-invalid, or over-limit image input rejects without proxy.

Implemented contract: exact-pinned vendored `png` 0.18.1 performs bounded
caller-driven decode. Upload jobs carry shared immutable texels; renderer
admission separately reserves unique texture count/bytes and explicit upload
creates one `Rgba8UnormSrgb` texture per source hash. One fixed repeat/linear
one-mip sampler and white fallback extend the existing bind group. Sampled
linear RGBA multiplies `baseColorFactor`; an explicit scene material disables
the imported texture. Controlled evidence pins top-to-bottom texel orientation,
factor multiplication, lit/unlit distinction, override, unchanged
depth/identity/normals/background, one shared GPU texture, and exact-hash
rehydration. JPEG, URIs, custom samplers, mipmaps, other texture roles, alpha
modes, image-based lighting, persistence catalogs, transport, CI expansion,
deployment, and release action remain excluded. See
[ADR 0029](../adr/0029-bounded-embedded-png-base-color-textures.md) and the
[GLB guide](../assets/glb-subset.md).

### PR 30 - CF030: Explicit content-hash asset eviction

Outcome: a long-lived local service can reclaim every bounded CPU and GPU
asset capacity for one exact source without replacing the service or changing
logical scene history.

Gate: one caller-driven content-hash operation removes its retained lifecycle
record, queued source, decoded meshes/shared texture, all pending uploads,
resident meshes, and unique pending/resident GPU texture. Exact typed outcomes
report each released count and byte total; repeated absence is idempotent and
unrelated import/upload FIFO order is unchanged.

Implemented contract: the world is not scanned or mutated. Asset references,
revision, logical hash, replay bytes, recovery state, frame frontier, and
separate exact-hash source files remain unchanged. A dependent draw uses its
authored primitive fallback or returns `AssetUnavailable`; explicit exact-hash
re-import/upload restores rendering. Submitted frames remain readable while a
backend defers physical GPU destruction. Per-mesh, LRU, reference-counted,
automatic/background eviction, retry policy, catalogs, file deletion,
automatic rehydration, device recreation, dependencies, CI expansion,
deployment, and release action remain excluded. See
[ADR 0030](../adr/0030-explicit-content-hash-asset-eviction.md), the
[GLB guide](../assets/glb-subset.md), and the
[local service guide](../protocol/local-service.md).

### PR 31 - CF031: Monotonic pending-work age status

Outcome: an operator can distinguish healthy caller-driven progress from
stalled commands, observations, imports, or uploads without inspecting queued
payloads or enabling background telemetry.

Gate: the existing command, service, asset-store, and renderer status values
report optional monotonic oldest-pending age in saturating microseconds. Empty
lifecycles report `None`. Exact deterministic tests cover duplicate retention,
`LatestWins` reset, capacity rejection and drop neutrality, FIFO-preserving
processing/eviction, and observation permit reservation through delivery or
error release.

Implemented contract: one process-local `Instant` follows each retained
transient lifecycle object. Status sampling is explicit and bounded; output is
aggregate only. Timestamps never enter canonical encoding, fingerprints,
asset identity, world state, logical hash, replay, recovery, persistence, or
observation metadata. Active-operation timing, background sampling, logging,
tracing, exporters, alerts, remote transport, authentication, CI expansion,
deployment, and release action remain excluded. See
[ADR 0031](../adr/0031-monotonic-pending-work-age-status.md), the
[local gateway guide](../protocol/local-gateway-and-imagination.md), and the
[local service guide](../protocol/local-service.md).

### PR 32 - CF032: Offline recovery-file inspection

Outcome: a local operator can validate one immutable recovery file through the
complete CPU restoration preflight and inspect bounded aggregate evidence
without requiring a compatible GPU or exposing replay contents.

Gate: `inspect_recovery_point` reuses restoration configuration, complete-tail,
frame-frontier, authoritative replay, revision, and logical-hash checks and
returns only entry/byte counts, final revision, next frame, logical hash, and
final replay-entry hash. `cogniform-cli inspect-recovery <path>` accepts exactly
one OS-native path, uses the declared `default-local-64x64` profile, loads
through `RecoveryFileStore`, performs no writes or adapter selection, and keeps
success/error output path- and payload-redacted.

Implemented contract: malformed, corrupt, truncated, extended, oversized,
growing, or non-file inputs fail at the existing bounded storage/envelope
boundary; semantically invalid replay or frame state fails at the exact engine
preflight. Passing proves CPU restorable state under the declared profile, not
GPU compatibility, asset residency, authenticity, freshness, or authorization.
Automatic startup/restore, catalogs, path discovery, latest pointers, directory
sync, asset association/rehydration, profile selection, JSON output, remote
transport, authentication, CI expansion, deployment, versioning, and release
action remain excluded. See
[ADR 0032](../adr/0032-offline-recovery-file-inspection.md) and the
[recovery-file guide](../persistence/recovery-files.md).

### PR 33 - CF033: Versioned recovery-inspection JSON

Outcome: scripts and local agents can consume a stable versioned aggregate
recovery-inspection report without parsing the unchanged human presentation or
gaining access to a path or replay payload.

Gate: `cogniform-cli inspect-recovery --json <path>` emits one deterministic
compact schema-version-one object followed by one line feed. Field names,
order, JSON types, fixed profile, counts, revision/frame values, and lowercase
hashes are exact. `--` permits a reserved option-like filename. Both output
modes finish the existing bounded load and complete CPU restoration preflight
before stdout; failures are nonzero with empty stdout and path/payload-redacted
stderr.

Implemented contract: the JSON report is a CLI-private serializable view over
the existing `RecoveryInspection`; neither the engine nor the recovery
protocol gains encoding. The CLI adds only direct edges to already pinned and
vendored `serde` and `serde_json`, with no package or version change. JSON
input, a general diagnostic schema, profile selection, automatic
startup/restore, discovery/catalog/latest pointers, asset association or
rehydration, remote transport, authentication, logging/exporters, CI
expansion, deployment, versioning, and release action remain excluded. See
[ADR 0033](../adr/0033-versioned-recovery-inspection-json.md) and the
[recovery-file guide](../persistence/recovery-files.md).

### PR 34 - CF034: Versioned controlled-measurement JSON

Outcome: scripts, CI helpers, and researchers can consume the existing
controlled CPU world measurement without parsing the unchanged human report.

Gate: `cogniform-cli measure-world --json` emits one fixed-layout compact
schema-version-one object followed by one line feed after all samples finish.
Top-level and nested field order, JSON types, fixed fixture/profile/sample
metadata, integer nanosecond units, monotonic distributions, and the explicit
informational-only flag are exact. The default human labels, ordering,
microsecond formatting, threshold statement, and debug warning remain intact.
Invalid or extra arguments fail with empty stdout.

Implemented contract: the JSON report is a CLI-private serializable view over
the existing engine measurement result. Checked conversion from engine `u128`
nanoseconds to JSON `u64` and complete in-memory serialization happen before
stdout. Existing CLI serialization dependencies are reused, so no manifest,
lockfile, package, vendor, engine, protocol, fixture, or baseline changes.
Threshold enforcement, arbitrary fixtures/sample counts, hardware identity,
JSON input, scenario JSON, logging/exporters, background sampling, telemetry,
transport, authentication, CI expansion, GPU work, deployment, versioning, and
release action remain excluded. See
[ADR 0034](../adr/0034-versioned-controlled-measurement-json.md) and the
[validation baseline](../operations/validation-baseline.md).

### PR 35 - CF035: Versioned canonical-scenario JSON

Outcome: scripts, CI helpers, and local operators can consume a stable
versioned proof of the successful canonical unattended scenario without
parsing its unchanged human report.

Gate: `cogniform-cli scenario --json` emits one fixed-layout compact
schema-version-one object followed by one line feed after adapter creation,
the complete scenario, three observations, replay verification, and in-memory
serialization succeed. Exact fields cover the fixed scenario/profile,
backend-neutral adapter summary, revision/query evidence, stable IDs, ordered
frames, center color/entity, visible pixels, matching live/replayed logical
hashes, and replay counts. The default 19-line human report remains byte for
byte intact. Invalid or extra arguments fail before GPU work with empty
stdout.

Implemented contract: the JSON report is a CLI-private serializable view over
the existing public `AdapterSummary` and successful `CanonicalScenarioReport`.
Existing CLI serialization dependencies are reused, so no manifest, lockfile,
package, vendor, engine, protocol, scenario, profile, tolerance, or supported-
adapter change. JSON input, scenario configuration, a general diagnostic
schema, adapter selection, performance timings/thresholds, logging/exporters,
telemetry, transport, authentication, CI expansion, deployment, versioning,
and release action remain excluded. See
[ADR 0035](../adr/0035-versioned-canonical-scenario-json.md), the
[canonical scenario guide](../getting-started/canonical-scenario.md), and the
[validation baseline](../operations/validation-baseline.md).

### PR 36 - CF036: Offline exact-hash asset-source inspection

Outcome: a trusted local operator can verify one caller-mapped immutable asset
source against its expected SHA-256 identity without decoding it, selecting a
GPU adapter, or exposing its path or payload.

Gate: `cogniform-cli inspect-asset <content-hash> <path>` accepts exactly one
lowercase 64-character public `ContentHash` and one OS-native path. Hash syntax
validates before filesystem work. The command reuses
`AssetFileStore::default().load` so regular-file, final-symlink, size,
allocation, growth, complete-read, and exact-hash checks remain singular.
Success occurs only after complete verification and reports exactly the hash
and bounded source byte count. Invalid arguments, file failures, and mismatch
are nonzero with empty stdout and path/payload-redacted diagnostics; the file
remains unchanged.

Implemented contract: inspection drops the verified bytes without GLB/PNG
decode, importer/service mutation, network access, upload, or GPU work. The CLI
moves its existing workspace protocol edge from test-only to regular use; no
external package, lockfile entry, storage API, engine, renderer, protocol type,
or asset-format change. JSON, format validation, catalogs/discovery, manifests,
automatic startup/import/upload/rehydration, profile selection, telemetry,
transport, authentication, CI expansion, deployment, versioning, and release
action remain excluded. See
[ADR 0036](../adr/0036-offline-asset-source-inspection.md), the
[asset-file guide](../persistence/asset-files.md), and the
[validation baseline](../operations/validation-baseline.md).

### PR 37 - CF037: Versioned asset-source inspection JSON

Outcome: scripts and local operators can consume stable machine-readable
asset-source identity evidence without parsing the human report or weakening
the CF036 storage and privacy boundary.

Gate: `cogniform-cli inspect-asset --json <content-hash> <path>` retains the
exact strict hash and single OS-native path contract. The default human output
is byte-for-byte unchanged, and a positional path named `--json` remains
ordinary input. After complete bounded file and SHA-256 verification, JSON is
constructed and serialized in memory as one compact LF-terminated object with
fixed order and types: integer `schema_version` 1, lowercase string
`content_hash`, and integer `source_bytes`. Failures are nonzero with empty
stdout and path/payload-redacted diagnostics in both modes.

Implemented contract: the schema is private to the CLI and reuses its existing
Serde dependencies. No package, vendor, lockfile, storage API, engine,
renderer, protocol, asset-format, decode, service, upload, rehydration, GPU,
CI, deployment, version, or release boundary changes. See
[ADR 0037](../adr/0037-versioned-asset-source-inspection-json.md), the
[asset-file guide](../persistence/asset-files.md), and the
[validation baseline](../operations/validation-baseline.md).

### PR 38 - CF038: Bounded observation-payload envelope

Outcome: future local, shared-memory, and authenticated remote adapters can
reuse one deterministic bounded payload representation that remains separate
from canonical causal metadata and renderer/service ownership.

Gate: a new dependency-neutral `cogniform-observation` crate owns the existing
public color, depth, normal, entity-ID, and visibility payload values plus a
version-one binary envelope. The fixed header and big-endian per-kind layouts
are byte-pinned. Encoding validates metadata, kind, counts, canonical values,
ordering, runtime pixels, visibility entries, and complete size before output
allocation. Decoding validates the borrowed input bound, exact framing and
length, metadata binding, and SHA-256 integrity before allocating the decoded
vector. All five kinds round-trip; truncation, extension, metadata substitution,
every-byte corruption, invalid tags/floats/IDs/order, and limit failures reject
with typed payload-redacted errors.

Implemented contract: `cogniform-engine` re-exports the moved values for source
compatibility and offers one explicit encoding method after observation
delivery. The codec performs no I/O and introduces no listener, session,
authentication, authorization, tenancy, shared-memory allocation, gRPC/QUIC,
compression, image format, retention, renderer scheduling, automatic delivery,
deployment, version, or release action. SHA-256 supplies corruption detection,
not writer authenticity or confidentiality. See
[ADR 0038](../adr/0038-bounded-observation-payload-envelope.md), the
[observation-payload envelope guide](../protocol/observation-payload-envelope.md),
and the [validation baseline](../operations/validation-baseline.md).

### PR 39 - CF039: Bounded local stream framing

Outcome: a later stdio agent session can reject declared local-stream bodies
before allocation and carry complete CF038 observations without inventing a
second metadata/payload association or opening a remote security boundary.

Gate: a new dependency-neutral `cogniform-local-transport` crate operates only
on caller-supplied synchronous `Read` and `Write` values. A byte-pinned
version-one 68-byte header carries magic, version, control/observation kind,
reserved zero byte, non-zero correlation ID, control and bulk lengths, and a
SHA-256 digest over the header prefix and both body sections. Independent
control, bulk, and complete-frame limits reject from the stack-read header
before body allocation. Clean pre-frame EOF, partial header/control/bulk,
short/interrupted reads and writes, back-to-back frames, trailing borrowed
input, corruption, invalid layouts, and I/O categories are deterministic and
payload-redacted.

Control bytes remain schema-owned and uninterpreted. Observation frames require
exact canonical `ObservationMetadata` JSON and the CF038 envelope; outer
integrity, canonical metadata, and inner semantic binding all validate before
return. Complete encoding precedes the first write, but arbitrary writer
failure can leave a physical prefix and is not claimed atomic.

Implemented contract: no external package or vendor source is added. The crate
opens no standard stream, pipe, process, file, shared memory, listener, or
socket; starts no thread or async runtime; and defines no operation schema,
version negotiation, session, scheduling, cancellation, authorization,
confidentiality, tenancy, rate policy, retention, automatic delivery,
deployment, version, or release action. SHA-256 detects corruption rather than
authenticating a writer. See
[ADR 0039](../adr/0039-bounded-local-stream-framing.md), the
[local stream-framing guide](../protocol/local-stream-framing.md), and the
[validation baseline](../operations/validation-baseline.md).

### PR 40 - CF040: Bounded local-session control messages

Outcome: one local patch/query/observation agent loop has interoperable,
direction-specific, versioned control semantics before any stdio endpoint or
service executor owns ambient I/O or scheduling.

Gate: a new `cogniform-local-session` crate defines compact canonical
LF-terminated schema-version-one client and server messages carried only by
CF039 control frames. The outer non-zero frame value is the sole correlation
identity. Hello exchanges self-consistent receive/effective limits; patches
separate immediate queued/already-queued/superseded/dropped/replayed admission
from newly applied completion; queries and observations require exact scene
revisions; observation acceptance/pending remains separate from the eventual
complete CF039 observation frame; failure codes are stable and payload-redacted;
and close is explicit.

Decode rejects complete bytes and nesting before deserialization, then rejects
direction/version/unknown/nested semantic failures before returning a value,
and finally requires exact canonical re-encoding. Bounded output serialization
does not first allocate an unbounded intermediate value. The effective control
ceiling is the minimum of core encoded, frame control, and post-header complete
frame limits.

Implemented contract: `ObservationRequest` moves to `cogniform-protocol` with
engine re-export compatibility, mandatory core schema version, exact expected
revision, and canonical bounded JSON. The engine rejects mismatch before
capacity reservation or renderer submission and verifies completion source
revision. No stream, stdin/stdout, pipe, process, file, shared memory, listener,
socket, service driver, scheduler, automatic queue processing, polling loop,
timeout/cancellation, authentication, authorization, confidentiality, replay
protection, tenancy, rate policy, deployment, version, or release action is
added. See [ADR 0040](../adr/0040-bounded-versioned-local-session-messages.md),
the [local-session message guide](../protocol/local-session-messages.md), and
the [validation baseline](../operations/validation-baseline.md).

### PR 41 - CF041: Bounded caller-driven local-session executor

Outcome: one reusable in-process state machine now runs the complete bounded
hello/patch/query/observation/close session contract over an owned local service
before any stdio, pipe, process, or endpoint composition receives authority.

Gate: a separate `cogniform-local-executor` crate requires exactly one hello
and intersects peer, local-frame, and service runtime limits field by field. It
rejects pre-hello work, duplicate hello, duplicate live correlation, duplicate
outstanding observation identity, over-limit renderer dimensions, and
non-quiescent close. Queries are immediate and exact-revision. Patch queue,
idempotency, replay, drop, and latest-wins supersession outcomes preserve exact
outer correlation and deterministic queue position; one explicit `advance`
processes no more than one command. Observation acceptance retains one bounded
mapping, reports pending at most once, and uses correlated completion or
failure delivery to release exactly once.

Every call returns at most two already-validated frames. Oversized completed
observations become redacted limit failures, and service diagnostics map to
stable session codes without payloads or arbitrary strings. The executor opens
no stream, stdin/stdout, pipe, process, file, shared memory, listener, socket,
thread, timer, scheduler, polling loop, or endpoint and adds no timeout,
cancellation, retry, authentication, authorization, confidentiality, remote
replay/rate/tenancy policy, deployment, version, or release action. See
[ADR 0041](../adr/0041-bounded-caller-driven-local-session-executor.md), the
[local-session executor guide](../protocol/local-session-executor.md), and the
[validation baseline](../operations/validation-baseline.md).

### PR 42 - CF042: Bounded fixed-profile stdio session

Outcome: `cogniform-cli serve-stdio` now composes CF039 framing, CF040 messages,
and the CF041 executor into the first executable local agent loop over one
inherited redirected stdin/stdout pair and one fixed 64x64 local service.

Gate: exact arguments and terminal misuse reject before adapter selection. A
frame-boundary EOF before any frame is a successful no-op; truncation,
corruption, EOF after a complete pre-hello frame or active hello, service or
executor failure, output/flush failure, and operation timeout terminate with a
stable redacted stderr category. Hello is emitted first, its effective frame
limits apply immediately, every frame is flushed, and admitted patch or
observation work reaches one terminal response before another input read. Live
completion uses a positive 2 ms poll cadence and fixed 15-second deadline, with
no busy spin or whole-frame retry; synchronous I/O, initialization, and
executor calls are not preempted. A failed write may leave a physical prefix.

The command creates no pipe, process, file, listener, socket, thread, shared
memory, daemon, configuration profile, remote identity, authentication,
authorization, confidentiality, freshness/replay/rate/tenancy policy,
deployment, version, or release action. CPU fake-stream tests cover scheduling
and fault behavior, while one controlled ignored child-process test proves the
hello/patch/query/observation/close exchange on an approved local adapter. See
[ADR 0042](../adr/0042-bounded-fixed-profile-stdio-session.md), the
[stdio-session guide](../protocol/local-stdio-session.md), and the
[validation baseline](../operations/validation-baseline.md).

### PR 43 - CF043: Bounded transport-neutral compilation results

Outcome: deterministic imagination compilation outcomes now have one
dependency-neutral schema-version-one value contract before any session
message or executor transports them.

Gate: a new `cogniform-compilation` crate owns the result schema, decision and
unresolved codes, explicit non-zero report limits, exact canonical LF JSON,
bounded pre-decode bytes/nesting, aggregate text/logical-byte accounting,
code-specific optional-field roles, strict order/uniqueness, and exact
compiled-versus-unresolved outcome and revision binding. Compiled results have
one valid normalized patch and no unresolved entry; unresolved results have no
patch and at least one issue.

`cogniform-compiler` constructs and validates the moved values and re-exports
their original public names. Default result limits derive from the compiler's
existing runtime limits. Normalization, stable IDs, patch bytes, gateway
admission, world/replay state, rendering, and every CF042 schema-version-one
session/stdio byte remain unchanged. The value crate performs no compilation,
service/world/render mutation, I/O, endpoint work, model call, authentication,
deployment, version, or release action. See
[ADR 0043](../adr/0043-bounded-transport-neutral-compilation-results.md), the
[compilation result guide](../protocol/compilation-results.md), and the
[validation baseline](../operations/validation-baseline.md).

### PR 44 - CF044: Versioned local imagination session mapping

Outcome: a local parent can submit one bounded semantic imagination and receive
its deterministic compilation and optional applied patch receipt through the
existing caller-driven executor and fixed inherited-stdio endpoint.

Gate: local-session schema version two preserves every schema-version-one byte
and operation, requires explicit `CompilationLimits` hello negotiation, and
adds imagination submission, admission, completion, and exact retained replay.
Nested imagination, compilation, patch, receipt, identity, revision,
idempotency, and role invariants fail closed. Negotiated compilation bounds are
installed before semantic work is admitted, so an over-limit result fails
before patch application. The executor uses one typed patch/imagination FIFO,
preserves all gateway admission semantics, terminates superseded and failed
correlations exactly once, compiles/applies only during explicit advancement,
and never processes a replay twice. The generic stdio driver remains
half-duplex and unchanged except for pending-imagination accounting. See
[ADR 0044](../adr/0044-versioned-local-imagination-session-mapping.md), the
[message guide](../protocol/local-session-messages.md), and the
[executor guide](../protocol/local-session-executor.md).

### PR 45 - CF045: Bounded MCP stdio adapter

Outcome: a standard local MCP parent can discover one exact-revision query
tool and one bounded semantic imagination tool without importing MCP concerns
into the engine or changing the existing binary local-session profile.

Gate: a separate `cogniform-mcp` crate pins official `rmcp` 2.2.0 and stable
MCP `2025-11-25`, exposes exactly `cogniform.query_scene` followed by
`cogniform.submit_imagination`, and declares deterministic pessimistic tool
annotations. A custom newline transport enforces independent input/output byte
bounds and outer JSON nesting before decode or first write, flushes every
complete line, and reports stable payload-redacted failures. The fixed 64x64
local service is created lazily only for a valid tool call, all calls are
serialized, typed core inputs and outputs are revalidated, and retained replay
cannot compile or mutate twice.

The CLI owns only inherited redirected streams and a current-thread runtime.
No resource, prompt, task, sampling, model, patch, procedure, asset,
observation, recovery, HTTP, socket, OAuth, listener, multiple-client,
full-duplex, deployment, version, or release surface is added. Ordinary SDK
and black-box tests prove initialize/list, exact tool metadata, malformed and
bounded framing, lazy failure, query, application, replay, EOF, and stdout
purity; one controlled ignored child test repeats query/application/replay on
an approved adapter. See [ADR 0045](../adr/0045-bounded-mcp-stdio-adapter.md)
and the [MCP adapter guide](../protocol/mcp-stdio-adapter.md).

### PR 46 - CF046: Bounded MCP apply-patch prerequisite

Outcome: a standard local MCP parent can submit one complete bounded atomic
scene patch, including camera components outside the current semantic compiler
subset, without bypassing the local service or changing narrower domain
contracts.

Gate: append exactly `cogniform.apply_patch` after the two CF045 tools while
preserving MCP `2025-11-25`, the exact dependency graph, bounded newline
transport, fixed lazy 64x64 service, and serialized single-user boundary. Parse
and validate one core `ScenePatch` before service creation; accept it only while
the command queue is empty; submit only through `LocalService`; process at most
one newly queued command; and return one bounded schema-version-one admission
plus receipt. Revalidate transaction, idempotency key, exact base revision,
operation count, and applied/replayed status. Exact retained retry must not
process or mutate twice; stale, conflicting, busy, invalid, and inconsistent
outcomes remain stable and payload-redacted.

No observation tool/resource, compiler change, bootstrap world, procedure,
asset, recovery, prompt, task, sampling, model, HTTP, socket, OAuth,
authentication, multiple-client, shared-memory, deployment, version, or
release surface is added. Ordinary contract tests and controlled official
client/CLI tests prove camera patch application, exact replay, conflict/stale
rejection, query/imagination continuation, protocol-pure stdout, and clean EOF.
See [ADR 0046](../adr/0046-bounded-mcp-apply-patch-tool.md) and the
[MCP adapter guide](../protocol/mcp-stdio-adapter.md).

### PR 47 - CF047: Bounded MCP observation resource

Outcome: a standard local MCP parent can request one exact-revision
observation and explicitly read its canonical CF038 payload as the sole
latest-value MCP resource.

Gate: append exactly `cogniform.observe_scene` after the three merged tools and
advertise resources without subscription or list-change support. Validate one
complete `ObservationRequest` before lazy service creation, submit and poll
only through `LocalService` at a fixed positive 2 ms cadence and 15 second
deadline, and require exact request, revision, camera, kind, quality,
dimensions, metadata, and zero-staleness causality. Encode the existing
`COGOBS01` envelope under the default 4 MiB bound. Return closed structured
success/error output plus one resource link; list zero or one deterministic
resource; read only the exact custom URI as base64 binary content; and replace
the prior resource atomically only after complete success. Admit only one MCP
request through complete response flush so pipelined reads cannot accumulate
handler tasks, payload clones, or encoded buffers.

No resource template, subscription, list-change notification, observation
history, persistence, compression, shared memory, prompt, task, sampling,
model, HTTP, socket, OAuth, authentication, multiple-client, SDK upgrade,
deployment, version, or release surface is added. Ordinary official-client
tests prove observation, link, list, read, exact base64, replacement, bounded
output, and failure preservation. Controlled production-service and CLI child
tests prove camera patching, canonical readback, exact causality, stdout purity,
and clean EOF. See
[ADR 0047](../adr/0047-bounded-mcp-observation-resource.md) and the
[MCP adapter guide](../protocol/mcp-stdio-adapter.md).

### PR 48 - CF048: Current MCP SDK maintenance with a fixed 2025 contract

Outcome: the isolated MCP adapter builds offline on the current stable official
Rust SDK without changing its accepted public protocol, authority, or runtime
behavior.

Gate: replace exact-pinned `rmcp` 2.2.0 with exact-pinned 3.1.2, regenerate the
locked vendored graph, and keep production on default-features-off `server`
only. Restrict the handler's supported revisions to exactly `2025-11-25`, reject
`server/discover`, Tasks and per-request `2026-07-28`, advertise no extensions,
and emit no 2026 `resultType`. Preserve the exact four-tool order, schemas,
annotations, structured results, one-resource list/read behavior, newline and
nesting limits, one-request-through-response-flush backpressure, lazy serialized
`LocalService`, stable failures, CLI stdout, and inherited-stdio trust boundary.

The dependency delta removes `async-trait` and adds the SDK-required
`uuid`/`getrandom` plus target-only `r-efi`; their features, checksums, licenses,
build scripts, and reachable call sites require explicit review. Ordinary
official-client and raw CLI tests prove fixed 2025 bytes and rejection of SDK-
added lifecycle/capability surfaces. Controlled production-service and CLI
child tests repeat the complete query, patch, imagination replay, observation,
resource readback, and clean-close flow. MCP 2026 lifecycle support remains a
separately designed and approved successor. See
[ADR 0048](../adr/0048-pin-current-rust-mcp-sdk-without-protocol-expansion.md).

### PR 49 - CF049: Conformant MCP discovery contract

Outcome: an MCP agent can discover output schemas that accept every actual
structured tool result and receive enough bounded initialization guidance to
use the four-tool workflow without inventing revision, retry, camera, resource,
serialization, or uncertain-effect policy.

Gate: replace the query and imagination success-only output schemas with
closed, mutually exclusive success/error alternatives. Pin their complete
stable error vocabularies to the implemented outcomes while retaining typed
core validation as the recursive authority. Emit one exact 508-byte server
instruction that starts a fresh child at revision zero, requires exact returned
revisions, distinguishes semantic from direct changes, limits idempotency reuse
to exact retries, requires a camera before observation, names resource readback,
states serialized execution, and identifies the outcomes that require child
discard without inferred retry.

Official-client and raw CLI tests assert the exact instruction bytes, all four
closed output-schema branches, error vocabularies, tool order, and representative
success/error results. Tool execution, protocol version, capabilities, core
types, dependencies, transport bounds, service authority, deployment, release,
and workspace version remain unchanged. See
[ADR 0049](../adr/0049-conformant-mcp-discovery-contract.md).

### PR 50 - CF050: Deterministic source-candidate archive

Outcome: a maintainer can prepare and independently verify the exact bounded
source asset required by ADR 0010 without trusting a dirty worktree, ambient
Git attributes, GitHub's regenerated compression bytes, or a tar extractor.

Gate: accept only an exact `refs/tags/...` direct annotated tag whose peeled
commit equals clean `HEAD`; retain and recheck the tag object, commit, `HEAD`,
cleanliness, and Git implementation identity. Create one uncompressed built-in
Git tar and one exact sha256sum-style sidecar with create-new semantics in one
existing caller-owned directory outside the worktree and Git directories.
Neutralize inherited/system/global Git configuration and archive attributes,
disable replacement objects, lazy object fetching, filesystem-monitor commands,
and submodule traversal, reject repository-info overrides, force
`tar.umask=0022`, use one fixed safe root, and enforce bounded Git metadata,
268,435,456 archive bytes, and 20,000 filesystem members while streaming.

Raw re-verification permits only the one Git commit PAX comment; canonical zero
padding/termination; exact stable portable directory/file inventory and Git
blob identities; fixed commit time, owner, group, and modes; and mandatory
offline source, vendor, license, docs, and tests. The actual non-vendor member
bytes pass the reusable public-repository rules without extraction. Disposable
Windows/Linux tests cover exact equality and limit edges plus identity,
attribute, path, type, metadata, PAX, substitution, corruption, sidecar,
trailing-data, existing-output, and cleanup failures. Version changes, tag
creation, network/API access, release assets, upload, signing, attestations,
SBOMs, deployment, and publication remain separately approved work. See
[ADR 0050](../adr/0050-deterministic-source-candidate-archive.md).

### PR 51 - CF051: Exact unpublished source-candidate identity

Outcome: the complete workspace identifies the prospective source-first
candidate consistently while every package remains non-publishable and all
tag, archive, upload, and publication authority stays closed.

Gate: set the shared workspace version, all exact first-party workspace
requirements, and every source-less first-party lock entry to `0.1.0-rc.1`.
Require all sixteen explicit members to inherit the shared version and retain
`publish = false`; require the fifteen crate dependencies to use their exact
reviewed local paths and forbid member-level first-party version/path/package
overrides. Run a bounded standard-library-only checker and disposable drift
matrix in the existing cost-conscious quality job.

Publish candidate notes and synchronize architecture, dependency, security,
failure, support, contributor, and release records. Reproduce the complete
ordinary and approved Windows/Vulkan controlled checklist before review. Do
not create a tag or real archive, mutate repository release settings, upload,
sign, attest, generate an SBOM, publish crates/binaries/containers, deploy,
merge, or publish a release. See
[ADR 0051](../adr/0051-version-source-candidate-without-publication.md).

### PR 52 - CF052: Immutable source-release and support contract

Outcome: maintainers and consumers have one exact publication, verification,
and support contract before any source candidate becomes public, while every
live release authority remains closed.

Gate: accept ADR 0052; require GitHub release immutability before draft
creation; pin the official `v0.1.0-rc.1` assets to
`cogniform-0.1.0-rc.1.tar` and
`cogniform-0.1.0-rc.1.tar.sha256`; document draft-first assembly and distinct
repository-setting, tag, archive, draft, upload, and publication approvals.
Give consumers exact repository-scoped release and per-asset attestation checks
plus an independent `sha256sum --check` step. Define support as latest-
published-candidate only, beginning at publication and ending on replacement
or withdrawal, with fixes issued only as a new incremented candidate.

Synchronize the SDD, release checklist/notes, security policy, README,
changelog, ADR index, and validation evidence. Do not change runtime or MCP
behavior, dependencies, manifests, lockfiles, workflows, permissions, package
version, repository settings, tags, archives, assets, drafts, uploads,
publication, deployment, or merge state. See
[ADR 0052](../adr/0052-immutable-source-release-and-support-contract.md).

### PR 53 - CF053: Bounded terminal MCP cancellation

Outcome: a parent can cancel one active inherited-stdio MCP request without
reopening concurrent tool dispatch or reusing a causally ambiguous child.

Gate: preserve exact MCP `2025-11-25`, four tools, one retained resource, and
all uncancelled bytes. Retain one semantically dispatched request and at most
one decoded bounded pending message. Deliver only an exact matching numeric or
string `notifications/cancelled` before response writing; suppress that
request's response, prevent pending/later dispatch, and terminate the child
successfully after bounded RMCP cleanup. Keep missing, mismatched, pending-
blocked, and post-write cancellation on the prior response-through-flush path.
Convert observation polling to cooperative Tokio waits, poison/drop the service
when an admitted observation is cancelled, and preserve the prior completed
resource until teardown.

Prove exact matching and wrong/missing/late IDs, numeric/string roles, one
pending-message bound, stalled output, cancelled response/error suppression,
cooperative poll wakeup, official-client terminal behavior, prior-resource
preservation, and unchanged ordinary/controlled compatibility. Do not add MCP
`2026-07-28`, concurrent tools, rollback, reusable cancellation, a general
deadline, new dependencies, remote authority, deployment, or release action.
See [ADR 0053](../adr/0053-bounded-terminal-mcp-cancellation.md).

### PR 54 - CF054: Bounded dual-era MCP stdio lifecycle

Outcome: modern MCP clients can discover or directly use Cogniform through
exact self-contained `2026-07-28` requests while legacy `2025-11-25` bytes,
bounds, surface, and authority remain fixed.

Gate: keep the project-owned bounded transport and use RMCP's dual-era server
lifecycle only after a bounded opening preflight preserves identified legacy
wrong-order/version responses. Pin one connection era before semantic
dispatch. Require exact protocol and client-capability metadata on every
modern request without inheriting discovery context. Reject modern initialize,
missing/malformed/unsupported/mixed metadata, unsupported methods, and client
Response/Error directions. Accept an unadvertised client extension declaration
only as authority-neutral metadata for an otherwise core request.

Modern discovery advertises only `2026-07-28`, tools, and resources. Preserve
the exact four-tool order, closed schemas, 508-byte instructions, one latest
resource, lazy serialized service, response-flush backpressure, and terminal
cancellation. Require `resultType` and informational server identity on every
supported modern success; use `ttlMs: 0` and private scope for discovery,
tool/resource lists, and resource reads. Prove official-client, direct-request,
raw mixed-era, malformed metadata, invalid-direction, cancellation, exact
legacy compatibility, controlled production-service, and legacy/modern CLI
child flows. Do not add Tasks, MRTR, subscriptions, Apps, prompts, sampling,
models, transports, clients, auth, persistence, dependencies, deployment, or a
release action. See
[ADR 0054](../adr/0054-bounded-dual-era-mcp-stdio-lifecycle.md).

### PR 55 - CF055: Bounded source-tangent glTF normal textures

Outcome: the approved GLB path can use one bounded source-tangent normal map
for direct lighting without changing geometric-normal observations or making
asset processing implicit.

Gate: accept finite non-zero same-count f32 `TANGENT` `VEC4` accessors with
exact `-1`/`1` handedness and complete-source validation. Accept one shared
`normalTexture` index using `TEXCOORD_0`, finite scale, embedded static PNG
pixels, and explicit source `NORMAL` plus `TANGENT`; retain at most two texture
and image records across base-color and normal roles. Count a source image
shared by both roles once on CPU, but atomically reserve and upload separate
sRGB base-color and linear normal GPU resources.

Expand the interleaved vertex ABI to an exact 48-byte
position/normal/primary-coordinate/tangent layout while preserving the prior
32-byte prefix. Build a transform-safe tangent basis, apply the perturbed
normal only to direct-light response, preserve unlit/depth/identity/geometric-
normal output, and disable both imported texture roles under a scene material
override. Prove malformed tangents/roles, exact shared/distinct accounting,
atomic reservation, eviction/rehydration semantics, and controlled GPU color
change with unchanged geometric outputs. Do not generate tangents, add other
UV sets/samplers/image formats/material textures, alter transport or recovery,
or take release action. See
[ADR 0055](../adr/0055-bounded-source-tangent-normal-textures.md).

## 3. Dependency graph

```text
CF000 -> CF001 -> CF002 -> CF003 -> CF004
  -> CF005 -> CF006 -> CF007 -> CF008 -> CF009 -> CF011 -> CF012 -> CF013
  -> CF014 -> CF015 -> CF016 -> CF017 -> CF018 -> CF019 -> CF020 -> CF021
  -> CF022 -> CF023 -> CF024 -> CF025 -> CF026 -> CF027 -> CF028 -> CF029
  -> CF030 -> CF031 -> CF032 -> CF033 -> CF034 -> CF035 -> CF036 -> CF037
  -> CF038 -> CF039 -> CF040 -> CF041 -> CF042 -> CF043 -> CF044 -> CF045
  -> CF046 -> CF047 -> CF048 -> CF049 -> CF050 -> CF051 -> CF052 -> CF053
  -> CF054 -> CF055
```

The default is linear merge order so every PR starts from an unambiguous reviewed base. A future maintainer may explicitly approve stacked work, but task dependencies remain the authoritative merge gates. Later work depends on proven semantics rather than only crate existence.

The numbered PR labels above identify product-delivery slices, not guaranteed
GitHub pull-request numbers. Bounded repository, security, or governance work
may interleave without changing the product dependency order.

The tracked roadmap, ADRs, code, tests, and pull-request record are the public
project history.

## 4. CI policy

### 4.1 Default pull-request check

One `ubuntu-latest` job always checks out the repository, runs the disposable
public-tree and source-candidate safeguards, and classifies changed paths using
Git metadata. For Rust-affecting changes it then runs:

1. install the pinned Rust toolchain;
2. `cargo fmt --all --check`;
3. `cargo clippy --workspace --all-targets --all-features -- -D warnings`;
4. `cargo test --workspace --all-features --locked`;
5. `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --all-features --locked`.

Documentation-only changes skip the Rust toolchain and Cargo steps while retaining the same required job result. Changes to Rust sources, tests, shaders, manifests, the lockfile, toolchain/lint policy, or workflows are Rust-affecting. The classifier is a checked-in shell step rather than another third-party action.

Policy details:

- `concurrency` cancels superseded work on the same PR/branch.
- `timeout-minutes` starts at 15 and is changed only with evidence.
- permissions are read-only unless a narrowly scoped workflow needs more.
- routine PR runs upload no logs, binaries, screenshots, or benchmark artifacts.
- no secrets, network tests, provider calls, containers, deployment, or release actions.
- dependency caching is introduced only if build timings justify its storage and supply-chain surface.

### 4.2 Risk-triggered checks

| Check | Trigger | Cost control |
|---|---|---|
| Dependency/license review | Changes to manifests, lockfile, or policy; manual dispatch | No unrelated daily matrix; Dependabot alerts stay platform-native |
| Windows/macOS compile | Manual dispatch and release-candidate gate; later on platform-sensitive paths if support demands it | Never multiply every PR by default |
| GPU conformance | Explicit self-hosted runner and relevant renderer changes/manual dispatch | No paid GitHub larger GPU runner |
| Fuzzing | Changed parser/validator corpus plus bounded manual/scheduled campaigns after those surfaces exist | Fixed time budget, corpus retained deliberately |
| Benchmarks | Manual or release candidate on controlled hardware | Never block ordinary PRs on noisy shared runners |
| Packaging/release | Tags or explicit workflow dispatch after approval | No automatic public release |

GitHub states that standard hosted runners are free for public repositories, while larger runners are billed even for public repositories and artifact/cache storage has separate usage. The policy minimizes resource waste and future private-repository exposure rather than treating free minutes as unlimited engineering budget.

## 5. Validation growth

Validation expands with capability:

- CF000-CF001: compile, lint, docs, contract/unit fixtures.
- CF002-CF003: property tests, invariant tests, deterministic replay fixtures.
- CF004-CF005: headless integration, shader/layout validation, exact ID and tolerant numeric probes.
- CF006-CF007: overload, fuzzable decoders, asset/security corpus, deterministic compiler/procedure fixtures.
- CF008-CF009: unattended end-to-end scenario, controlled performance measurements, fault injection, compatibility and packaging evidence.
- CF011: additive observation contracts, normal decode boundaries, controlled
  winding/tolerance probes, and regression coverage for pooled readback and
  causality.
- CF012: complete-stream rejection, restored idempotency/query/replay
  continuation, empty transient queues, and controlled renderer frame
  continuity.
- CF013: deterministic recovery-envelope round trip, every-byte corruption
  rejection, typed header/version/bound/length/frame/integrity failures, and
  controlled decode-before-restore continuation.
- CF014: deterministic exact-revision prefixes, typed future-revision
  rejection, source immutability, current-frame-frontier preservation, and
  controlled historical query/observe/append continuation.
- CF015: exact-hash service admission, explicit single-item import/upload,
  stable-identity GLB observation, empty recovered asset state, typed missing
  residency, and revision/hash/replay-preserving rehydration.
- CF016: pre-allocation procedure/text-budget rejection, deterministic
  stable-ID output, ordinary queue/delivery/idempotency behavior, exact logical
  query, replay/hash equality, and restored world-idempotent resubmission.
- CF017: quiescence rejection without mutation, fresh-before-swap revert,
  explicit cache/asset clearing, exact prefix/query/hash/renderer restoration,
  source-frame continuity, retained idempotency, and removed-tail continuation.
- CF018: encode-before-I/O, create-new immutability, injected write/sync cleanup,
  path-redacted failures, regular-file and allocation bounds, corruption and
  growth rejection, and controlled persisted restoration continuation.
- CF019: pre-I/O source-size and exact-hash rejection, create-new immutability,
  shared write/sync cleanup, bounded regular-file load, growth/substitution
  rejection, and controlled separate recovery/asset restore and rehydration.
- CF020: strict optional-normal accessor/count/value/range validation,
  deterministic normalization and flat fallback, exact 24-byte CPU/GPU vertex
  accounting, interleaved upload checks, and controlled winding,
  inverse-transpose, interpolation, and observation probes.
- CF021: exact fixed plane layout/winding, shape and fallback selection,
  all-axis model scaling, retained sphere rejection, and controlled color,
  depth, identity, background, and world-space-normal probes.
- CF022: exact fixed sphere topology/bytes/radius/winding, radial normals,
  direct and fallback selection, resident-asset precedence, all-axis bounding
  diameters, and controlled color, curved-depth, identity, background, and
  smooth world-space-normal probes.
- CF023: stable directional ordering, normalized positive-Z direction,
  inactive-definition accounting, fixed capacity and uniform layout, typed
  rejection, exact unlit compatibility, and controlled front/back diffuse
  evidence.
- CF024: stable point ordering and translation, inactive-definition accounting,
  independent fixed capacity, finite GPU conversion, exact appended uniform,
  zero-distance safety, and controlled near/far/back-facing diffuse evidence.
- CF025: exact cuboid topology/layout/extents, two triangles per face,
  non-degeneracy, outward axis-aligned winding/normals, controlled reference
  near-face orientation, and positive canonical exterior Point-light response.
- CF026: finite camera/material preparation, exact 480-byte uniform layout,
  bounded direct-response vectors, exact unlit compatibility, and controlled
  dielectric/metallic/roughness evidence.
- CF027: imported numeric material retention/default/range/proxy behavior,
  immutable residency and scene override precedence, plus controlled unlit and
  direct-light evidence.
- CF028: exact/indexed primary-coordinate retention including out-of-unit
  values, full-source finite/count/range validation, typed unsupported
  encodings, exact 32-byte CPU/GPU and zero-default built-in/proxy layouts, and
  controlled whole-frame visual equivalence.
- CF029: strict embedded PNG/reference validation, bounded RGB/RGBA decode and
  expansion, independent CPU/GPU texture accounting, unique explicit upload,
  fixed repeat/linear sRGB sampling and white fallback, factor/override
  semantics, plus controlled orientation, direct-light, and exact-hash
  rehydration evidence.
- CF030: queued/ready/proxy/rejected CPU-record eviction, exact pending and
  resident mesh/texture release accounting, unrelated FIFO preservation,
  idempotent absence, submitted-frame GPU lifetime, fallback/unavailable
  behavior, and logically neutral exact-hash rehydration.
- CF031: exact monotonic command/import/upload/observation ages under injected
  time, duplicate and supersession semantics, rejection/drop neutrality,
  processing/eviction/delivery cleanup, saturation, empty status, and
  controlled service lifecycle regressions.
- CF032: valid empty/nonempty aggregate recovery inspection; exact reuse of
  complete replay/world/frame preflight; storage corruption, truncation,
  extension, growth, oversize, and non-file rejection; semantic replay/frame
  rejection; exact CLI argument/help behavior; path/payload redaction; file
  immutability; and ordinary CPU-only CI evidence without adapter selection.
- CF033: exact compact schema-version-one recovery JSON, fixed field order and
  JSON types, lowercase hashes, unchanged human bytes, reserved filename
  escape, pre-output semantic validation, empty failure stdout, redaction, and
  no new package/version/vendor or GPU boundary.
- CF034: exact compact schema-version-one controlled-measurement JSON, fixed
  top-level and distribution field order/types, integer nanoseconds, monotonic
  distributions, explicit informational-only semantics, unchanged human
  structure and debug warning, exact argument rejection, empty failure stdout,
  and no manifest, lockfile, baseline, package, or GPU boundary.
- CF035: exact compact schema-version-one canonical-scenario JSON, fixed
  top-level/nested field order and types, lowercase identity/hash/color
  encoding, matching live/replayed proof, causal frame ordering, unchanged
  19-line human bytes, pre-GPU argument rejection, complete-before-output
  behavior, and controlled human/JSON GPU regression without a new engine,
  protocol, dependency, profile, tolerance, or supported-adapter boundary.
- CF036: exact hash/path arity and lowercase hash parsing before file work;
  valid immutable aggregate output; option-like OS-native path; mismatch and
  non-file empty-stdout path/payload redaction; unchanged source bytes; reuse
  of existing bounded storage checks; and structural CPU-only execution with
  no decode, service, network, upload, GPU, external dependency, or release
  boundary.
- CF037: exact compact schema-version-one asset-source inspection JSON, fixed
  field order/types and LF framing, lowercase hash, unchanged human bytes,
  positional `--json` filename behavior, complete verification and in-memory
  serialization before stdout, empty failure stdout, redaction, and no new
  package, lockfile, core schema, or GPU boundary.
- CF038: exact all-kind round trips and byte fixtures, fixed big-endian header
  and value layouts, engine re-export compatibility, metadata substitution,
  every-prefix truncation, trailing input, every-byte corruption, invalid
  canonical float/presence/identity/order/count cases, and independent runtime,
  visibility-entry, and complete-envelope limits without GPU or transport I/O.
- CF039: byte-stable fixed header and kind tags; exact control and complete
  observation round trips; header-first independent limit and overflow
  rejection without body reads; clean EOF versus header/control/bulk
  truncation; short/interrupted reads and writes; back-to-back framing;
  trailing borrowed input; every-byte corruption, metadata substitution,
  noncanonical metadata, invalid nested payload, writer prevalidation, stable
  I/O categories, and debug/error redaction without endpoint or GPU work.
- CF040: one exact LF-terminated schema-v1 fixture and CF039 control-frame round
  trip for every client/server variant; outer-only correlation; direction, version, unknown-field,
  noncanonical, nesting, substitution, truncation, trailing, nested-value,
  advertised/effective-limit, receipt-role, and observation-frame rejection;
  exact-revision observation admission ahead of capacity and renderer work;
  and source-compatible engine re-export without endpoint, executor, or GPU
  output changes.
- CF041: field-wise hello limits, exact patch and observation correlation
  lifecycle, deterministic bounded advancement, pending-at-most-once behavior,
  quiescent close, over-limit completion replacement, stable redacted failures,
  and controlled service composition without endpoint authority.
- CF042: exact pre-adapter argument/terminal/first-frame behavior, immediate
  clean EOF, half-duplex no-read-while-live scheduling, negotiated frame
  limits, partial/interrupted write and flush faults, bounded positive-cadence
  polling with a fixed deadline, fatal service/executor handling, and one
  controlled ignored child-process exchange.
- CF043: exact compiled/unresolved schema-version-one LF fixtures; canonical
  round trips; byte, nesting, logical, text, decision, unresolved, and nested
  patch bounds; version/unknown/code-field/order/duplicate/outcome/revision/
  substitution/truncation/trailing rejection; compiler re-export compatibility;
  and unchanged normalized patch behavior without session or I/O work.
- CF044: unchanged exact version-one fixtures; exact version-two hello,
  imagination, completion, and replay fixtures; field-wise compilation-limit
  negotiation; mixed-version and malformed-role rejection; every imagination
  admission outcome; compiled/unresolved completion; mixed-command
  supersession; service-error release; exact replay without duplicate
  processing or mutation; and one controlled ignored version-two child-process
  exchange.
- CF045: exact MCP `2025-11-25` initialization and newer-version rejection;
  deterministic query/imagination tool order, schemas, and annotations;
  incremental input and encode-before-output byte/nesting bounds with equality
  cases; malformed/truncated/redacted failure behavior; lazy invalid-argument
  handling; ordinary initialize/list/EOF stdout purity; and controlled ignored
  adapter-backed library and CLI exchanges covering exact query, compiled
  application, and retained replay without a second revision.
- CF046: exact three-tool discovery and deterministic top-level patch
  schema/annotations with authoritative typed core validation; malformed,
  invalid, and over-limit rejection before lazy service creation; complete
  transaction/key/base/operation/status receipt roles; queued/replayed/busy and
  invalid-service-output mapping; controlled direct camera patch application,
  exact replay, conflicting-key and stale-base rejection, compatible query and
  imagination continuation, stdout purity, and clean EOF.
- CF047: exact four-tool discovery and resources capability without optional
  subscription/list-change support; authoritative request validation before
  lazy service creation; fixed positive polling and deadline behavior; exact
  completion metadata, dimension, revision, and staleness roles; canonical
  `COGOBS01` base64 readback under the 4 MiB payload and 8 MiB output bounds;
  one-request-through-flush backpressure; zero-or-one listing, exact-URI reads, atomic replacement, failure
  preservation, and controlled production-service plus CLI child evidence.
- CF048: exact-pinned official SDK 3.1.2 builds from the regenerated offline
  vendor graph; the handler advertises only `2025-11-25`; raw requests prove
  `server/discover`, Tasks, and per-request `2026-07-28` reject; 2025
  initialize/list/call/resource results omit extensions, execution metadata,
  and `resultType`; every CF045-CF047 bound, stable outcome, backpressure path,
  and controlled production/CLI flow remains compatible.
- CF049: exact 508-byte initialization guidance covers the fresh-revision,
  exact-revision, mutation-choice, retry, camera/resource, serialization, and
  loss-of-trust workflow; every tool advertises closed mutually exclusive
  success/error output schemas, with query and imagination now pinning their
  complete stable error vocabularies; official-client and raw CLI tests assert
  exact discovery bytes and representative errors without service startup,
  while existing controlled production-service evidence covers successful
  query and imagination execution.
- CF054: exact byte-compatible 2025 initialization and exact self-contained
  2026 discovery/direct requests; connection-era pinning; required modern
  protocol/capability metadata on every request; complete result, server-
  identity, and zero-lifetime private-cache roles; invalid direction,
  missing/malformed/unsupported/mixed-era and unsupported-method rejection
  before semantic dispatch; extension-declaration neutrality; unchanged
  four-tool/resource/cancellation/bounds authority; and official-client,
  raw-wire, ordinary CLI, plus controlled legacy/modern production evidence.
- CF055: strict complete-source tangent shape/count/value/handedness and
  normal-texture role validation; exact 48-byte prefix-compatible vertex ABI;
  shared/distinct image CPU accounting; atomic content-hash-and-role GPU
  reservation, upload, eviction, and rehydration semantics; linear normal
  sampling with finite scale and transform-safe TBN; unchanged unlit, depth,
  identity, background, and geometric-normal observations; and controlled
  direct-light perturbation evidence.

No performance threshold becomes a merge gate until reference hardware, fixture, sampling method, and baseline are versioned.

## 6. Deferred roadmap

After the MVP and only with evidence: configurable stdio profiles, MCP resource
templates, subscriptions, list-change notifications, resource history and
persistence, MCP multi-round-trip results, prompts, tasks, sampling, models,
named-pipe or socket creation,
multiple clients, full-duplex scheduling, process supervision, shared-memory
observation leases, authenticated MCP HTTP or gRPC/QUIC transport, Wasmtime procedures,
KTX2/mesh optimization, advanced culling/batching, model bridge, Gaussian
splat plugin, browser target, fleet orchestration, and high availability. Each
requires a new design decision and approved task rather than silently entering
an MVP PR.
