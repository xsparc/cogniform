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

## 3. Dependency graph

```text
CF000 -> CF001 -> CF002 -> CF003 -> CF004
  -> CF005 -> CF006 -> CF007 -> CF008 -> CF009 -> CF011 -> CF012 -> CF013
  -> CF014 -> CF015 -> CF016 -> CF017 -> CF018 -> CF019 -> CF020 -> CF021
  -> CF022 -> CF023
```

The default is linear merge order so every PR starts from an unambiguous reviewed base. A future maintainer may explicitly approve stacked work, but task dependencies remain the authoritative merge gates. Later work depends on proven semantics rather than only crate existence.

The numbered PR labels above identify product-delivery slices, not guaranteed
GitHub pull-request numbers. Bounded repository, security, or governance work
may interleave without changing the product dependency order.

The tracked roadmap, ADRs, code, tests, and pull-request record are the public
project history.

## 4. CI policy

### 4.1 Default pull-request check

One `ubuntu-latest` job always checks out the repository and classifies the changed paths using Git metadata. For Rust-affecting changes it then runs:

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

No performance threshold becomes a merge gate until reference hardware, fixture, sampling method, and baseline are versioned.

## 6. Deferred roadmap

After the MVP and only with evidence: shared-memory observation leases, authenticated gRPC/QUIC transport, Wasmtime procedures, KTX2/mesh optimization, advanced culling/batching, model bridge, Gaussian splat plugin, browser target, fleet orchestration, and high availability. Each requires a new design decision and approved task rather than silently entering an MVP PR.
