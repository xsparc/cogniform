# Cogniform Software Design Document

Status: architecture baseline derived from supplied research on 2026-08-02. Implementation decisions marked Proposed require task approval.

## 1. Product direction

### 1.1 Purpose

Cogniform is a deterministic, headless-first 3D scene-materialization engine for agent-in-the-loop workloads. Its primary job is not simulation or human editing. It repeatedly turns semantic imagination or explicit operations into revisioned scene state, renders machine-readable observations, and returns feedback that proves which scene revision was observed.

The essential loop is:

```text
agent intent or patch
  -> bounded validation and deterministic compilation
  -> atomic authoritative-world commit
  -> compact render extraction
  -> headless color/geometry/identity observation
  -> revision-linked structured feedback
```

### 1.2 Primary users and jobs

- Agent developers need a typed, inspectable world interface that makes causality and failures explicit.
- Research and simulation teams need reproducible logical scenes, replay logs, and machine-readable render outputs.
- Engine contributors need narrow ownership boundaries, deterministic fixtures, and local/offline validation.
- Operators need bounded queues, observable latency stages, controlled resource usage, and recoverable failures.

### 1.3 MVP success

An unattended client creates a room, table, light, and camera; moves and restyles the table in one atomic patch; receives the committed revision; requests color and entity-ID observations; receives visibility linked to that revision; and replays the accepted log to the same canonical logical hash.

The design prioritizes, in order:

1. correct revision and observation causality;
2. atomic scene mutation and deterministic replay;
3. bounded behavior under overload;
4. command-to-visible-result latency;
5. minimal copies and steady-state allocations;
6. portable baseline rendering;
7. replaceable advanced GPU, IPC, and model features.

### 1.4 MVP non-goals

No full editor, rigid-body physics, skeletal animation, audio, arbitrary native plugins, user shaders, distributed world simulation, sophisticated global illumination, neural renderer, differentiable renderer, or required browser deployment.

## 2. Domain architecture

### 2.1 Ownership model

| Domain | Owns | Must never do |
|---|---|---|
| World | Authoritative ECS, stable IDs, revisions, transactions, hierarchy, transforms, ownership, spatial state | Wait for the GPU; perform blocking network or disk delivery |
| Render | GPU adapter/device/queue, resources, passes, extraction consumption, frame metadata, observations | Infer models; decode assets; deliberate for agents; mutate authoritative world state |
| Service | Protocols, sessions, budgets, idempotency, transport, persistence, encoding, optional model workers | Hold mutable ECS/GPU handles or bypass world transactions |

One authoritative world exists per engine instance. The GPU sees an incremental, immutable extraction packet. Services submit typed commands and consume receipts/feedback. Optional model and encoder processes are sidecars, not frame-loop dependencies.

### 2.2 Initial workspace boundaries

The first workspace should prove boundaries without prematurely creating every eventual module:

| Area | Responsibility | Dependency rule |
|---|---|---|
| `cogniform-protocol` | Stable public value types, patches, receipts, observations, limits, errors | No ECS, GPU, network, or generated transport dependency |
| `cogniform-observation` | Owned observation payload values and bounded transport-neutral binary envelopes | Depends only on protocol and deterministic hashing; owns no renderer, service, I/O, session, or shared-memory resource |
| `cogniform-compiler` | Pure seeded primitive imagination normalization and structured explanations | Depends only on protocol and deterministic hashing; owns no world/service state |
| `cogniform-world` | `hecs` implementation, stable-ID index, validation, atomic commit, hierarchy, transforms, queries | Depends on protocol/math; never renderer or service |
| `cogniform-replay` | Canonical event encoding, hash chain, replay and logical scene hashing | Depends on public world snapshots/events, not GPU state |
| `cogniform-renderer` | `wgpu` feature negotiation, headless targets, primitive rendering, and color/depth/normal/ID outputs | Consumes extracted render data; never mutable world access |
| `cogniform-engine` | Bounded channels, domain lifecycle, frame/revision correlation, composition, and complete in-memory restoration | Orchestrates through public interfaces; does not absorb domain state or perform persistence |
| `cogniform-storage` | Explicit create-new and bounded-load adapters for local recovery envelopes and exact-hash asset sources | Depends on public engine recovery values, replay bounds, and asset identities; owns filesystem authority without world, renderer, replay, decode, or upload access |
| `cogniform-cli` | Local sample client, replay and diagnostic commands | Composes public engine/protocol interfaces and explicit service adapters such as storage; owns no domain state |

CF006 establishes the semantic compiler as a separate pure crate while its
offline gateway remains in the engine composition boundary. CF018 establishes
recovery-file persistence as a separate service-domain adapter, CF019 adds
separate exact-hash asset-source files without creating a catalog, CF032
allows the CLI composition root to invoke that adapter and the engine's exact
CPU restoration preflight for offline diagnostics, and CF033 adds a versioned
machine-readable recovery view at that CLI boundary without moving encoding
into the engine. CF034 similarly adds a versioned view over the existing
controlled CPU measurement at the CLI boundary, with no engine or protocol
encoding. CF035 adds the same boundary-preserving machine-readable option to
the successful canonical scenario proof while retaining its default human
report. CF036 lets the CLI invoke the existing bounded exact-hash asset load as
a read-only aggregate diagnostic without moving filesystem authority or asset
bytes into the engine. CF037 adds a CLI-private schema-version-one view over
that verified result while preserving the exact human report and storage
boundary. CF038 establishes a separate payload-codec boundary that binds fixed
binary bulk values to canonical observation metadata without selecting a
transport, listener, session, or storage policy. Spatial
acceleration, shared memory, remote transport, Wasm, and model bridge become
separate crates only when their milestone establishes an independent contract
or dependency footprint.

### 2.3 Dependency direction

```text
protocol <- world <- replay
    ^
    +---- observation
    ^
    +---- compiler
    ^          |
    |          v
    +---- render extraction -> renderer
    ^                              |
    +---------- engine ------------+
                   ^       ^
                   |       |
                   |     storage
                   |       ^
                   +--- CLI
```

The diagram shows allowed information flow, not permission to create circular Cargo dependencies. Shared render DTOs belong in a dependency-neutral boundary rather than making world depend on renderer.

The compiler and observation codec each depend only on protocol values and
deterministic hashing. Engine may orchestrate compiler, world, renderer,
observation coding, and replay through their public interfaces; neither narrow
crate reads mutable world or renderer state. Storage depends on the public
recovery and asset identities it persists. The CLI may compose engine and
storage but must not move filesystem authority into the engine.

## 3. Core contracts and invariants

### 3.1 Identity and revisions

- `StableEntityId` is an opaque 128-bit external identifier and is never a raw `hecs::Entity`.
- `SceneRevision` is a monotonically increasing 64-bit value.
- `FrameId` identifies a produced frame; frame metadata includes the latest fully extracted scene revision.
- Internal ECS handles may be reused without changing external identity semantics.
- A stable-ID index is updated only as part of an accepted transaction.

### 3.2 Scene patches

`ScenePatch` contains schema version, transaction ID, idempotency key, base revision, conflict policy, ordered operations, and declared limits. Initial operations cover create, delete, set/remove component, and reparent. Asset references are ordinary components, and built-in procedures remain pure bounded producers of ordinary patches rather than privileged mutation operations. Observation policy arrives through a separate versioned contract.

Atomic apply follows this contract:

1. reject encoded input beyond configured byte and collection limits;
2. require a supported schema and an acceptable base revision;
3. canonicalize operation order only where the public contract declares order irrelevant;
4. preflight IDs, generations, ownership, hierarchy cycles/depth, component validity, assets, and budgets;
5. build a commit plan without mutating the authoritative world;
6. apply the complete plan and update indexes/generations;
7. increment the scene revision exactly once and persist the accepted canonical event;
8. return a receipt with previous/new revision, operation count, diagnostics, timing, and estimated visible frame.

A failed atomic patch preserves revision, entity state, indexes, and logical hash. Repeating an accepted idempotency key returns the recorded receipt and does not create another revision. Empty patches are rejected rather than manufacturing revisions.

### 3.3 Hierarchy and transforms

Hierarchy is layered over the flat ECS using explicit parent/child relations. Validation rejects cycles and enforces a configured maximum depth. Transform propagation uses stable parent-before-child order and generation tracking so work is proportional to changed branches. `LocalTransform` is authoritative input; `WorldTransform` is cached derived state.

### 3.4 Determinism and hashing

| Tier | Contract |
|---|---|
| Replay deterministic | Same accepted events, build, asset/config hashes, platform class, and seeds produce identical logical state hashes |
| Visual stable | Color/depth/normal output remains within declared numeric or perceptual tolerances |
| Bitwise strict | Selected CPU/output paths match bit-for-bit on a controlled configuration |

The MVP guarantees replay determinism. Canonical hashing sorts entities by stable ID and components by versioned type key, excludes transient handles/timing/GPU resources, defines endianness, and normalizes unsupported floating-point values. Randomized map iteration and implicit entropy are prohibited in hashed or compiled output. Agent/model output is recorded as external input; its production is not claimed deterministic.

Complete engine recovery state can be represented as one bounded, versioned
envelope containing the portable replay stream and next unreserved frame
identity. Its deterministic SHA-256 digest detects corruption before payload
allocation, but does not provide authenticity or confidentiality. Complete
replay and frame validation remain mandatory before restoration.

An opt-in storage adapter may persist the complete envelope to one new local
regular file. Encoding and bounds validation happen before filesystem mutation;
loading is bounded before allocation and returns only a complete digest-valid
envelope. The adapter never overwrites, selects, rotates, authenticates, or
automatically restores a file. Caller-owned path permissions, confidentiality,
directory durability, retention, and freshness remain outside the engine.

The same adapter boundary may separately retain the exact source bytes for one
known content hash. Size and identity validate before create-new I/O; bounded
load returns no bytes until the complete file matches the caller-supplied hash.
Recovery-to-asset association, path discovery, retention, and explicit
load/import/upload scheduling remain caller-owned rather than becoming a
second recovery format or hidden service lifecycle.
The CLI may verify one explicitly mapped asset file through that same bounded
load and report only its expected hash and byte count. This inspection neither
discovers content nor proves format safety and never schedules decode, service,
or GPU work.

Any retained exact revision can also be represented as a complete standalone
replay prefix and restored into a separate fresh service. The point carries the
source renderer's current next unreserved frame identity so logical history may
branch without reusing an identity issued before capture. Capturing a
historical point does not mutate the source; concurrent branches advance
independent frame counters. A quiescent local service may use the same point to
build a fully restored replacement and assign it only after successful
initialization; automatic rollback and cross-branch coordination remain
separate lifecycle concerns.

### 3.5 Backpressure

Every queued command declares one of:

- `MustApply`: ordered durable work; admission fails explicitly when capacity is unavailable.
- `LatestWins(key)`: a new uncommitted value supersedes an older one for the same key.
- `BestEffort`: may be dropped under configured pressure.

Queues have fixed or configuration-bounded capacity, observable depth/age/drop
counters, and no hidden unbounded spill. Durable work receives typed
backpressure; replaceable observations use latest-value behavior. The local
service reports optional monotonic oldest-pending age in saturating
microseconds for commands, outstanding observations, asset imports, and
renderer uploads. Empty lifecycles report no age. Duplicate or already-known
work preserves the original admission age, `LatestWins` replacement resets its
age, and rejected or dropped work does not alter retained age.

### 3.6 Observation causality

Every feedback envelope includes scene revision, frame ID, camera ID when relevant, observation timestamp, production latency, quality level, and staleness. An observation never claims a newer revision than its render input. Readback, encoding, delivery, and optional inference are asynchronous and may complete later while retaining source causality.

Owned color, depth, normal, entity-ID, and visibility payloads stay separate
from canonical causal metadata. An explicit versioned binary codec binds the
metadata's exact canonical JSON to a fixed-layout payload under independent
envelope and visibility-entry bounds. Encoding is caller-invoked after local
delivery; it does not enter the render worker, persist data, or create a
transport. Its SHA-256 digest detects corruption but does not authenticate,
authorize, encrypt, or establish freshness.

## 4. Rendering and assets

### 4.1 Renderer baseline

Use `wgpu` with built-in WGSL and negotiated adapter features/limits. The baseline is a small forward renderer with primitive meshes, camera, depth, color, entity-ID, and quantized world-space normal output. Position-only triangles remain flat; approved imported vertex normals are inverse-transformed and interpolated. Headless mode renders to textures without constructing a visible window. Optional `winit` integration is isolated from the headless core.

Built-in cuboids, centered unit XY planes, and centered unit-diameter spheres
use immutable expanded position-plus-normal-plus-primary-coordinate buffers.
A cuboid is a centered unit box with 12 outward counter-clockwise triangles,
36 expanded vertices, exact axis-aligned exterior normals, and zero
coordinates in one fixed 1,152-byte payload. Plane triangles wind
counter-clockwise toward positive Z, remain at local Z = 0, and apply all
positive XYZ dimensions through the model transform; X/Y set visible extents
while Z participates in normal transformation without creating thickness.
The sphere has a positive-Z polar axis, fixed 16-sector by 8-band topology,
outward counter-clockwise triangles, and unit radial normals. Its XYZ
dimensions are bounding diameters. The fixed 672-vertex sphere payload is
21,504 bytes and is generated once at renderer initialization; no frame
performs tessellation. The plane payload is 192 bytes. All built-ins use exact
zero primary coordinates.
A missing asset uses its exact explicit built-in fallback, and a resident
asset retains precedence.

The baseline shades materials with independently bounded sets of up to four
directional and four point definitions in stable entity-ID order. Local
negative Z is the directional emission axis, so transformed positive Z points
from the surface toward that source. A point source uses its extracted world
translation and capped inverse-square attenuation
`min(intensity / max(distance_squared, 1e-6), 1)`; exact source/fragment
coincidence and finite-input f32 squared-distance overflow contribute zero.
Active definitions use one bounded direct Cook-Torrance metallic-roughness
response: GGX distribution, Schlick-GGX Smith visibility, Schlick Fresnel,
dielectric normal reflectance `0.04`, a metallic base-color reflectance blend,
and an energy-conserving Lambert diffuse split. Perceptual roughness has a
`0.05` distribution floor for numerical safety. Contributions and their sum
are clamped in linear color space, and alpha remains unchanged.
Zero-intensity definitions count toward their kind's capacity but are inactive.
Only a scene with no active definition of either kind bypasses lighting and
preserves exact base RGBA. A resident GLB mesh supplies its imported base
color, optional base-color texture, metallic, and roughness when the entity has
no explicit material. The shader samples the single `Rgba8UnormSrgb` image
through a fixed repeat/linear one-mip sampler and multiplies sampled linear
RGBA by the numeric base-color factor before either shading path. Untextured
draws use a neutral white fallback. A scene `MaterialComponent` overrides the
whole imported material and disables its texture. A built-in or material-free
asset without a scene material uses its existing fallback color with neutral
dielectric parameters `metallic = 0`, `roughness = 0.8`. Ambient, emissive,
image-based lighting, shadows, spot lights, configurable point range/radius,
other texture roles, HDR, and tone mapping are outside this baseline. A
fixed 480-byte per-draw uniform preserves the prior 448-byte model, view-projection,
material-color, identity, directional, and point-light prefix and appends
zero-padded camera-position and metallic/roughness slots. A fifth definition
of either kind, a degenerate active direction, an active point position, or a
selected camera position outside finite GPU-f32 range fails before GPU
submission.

Feature tiers are capability-based:

- Baseline: downlevel/WebGPU-compatible limits, normal bind groups, CPU culling.
- Modern: storage buffers, indirect drawing, compute culling, larger arrays.
- Advanced native: timestamps, multi-draw or subgroup/compression features where proven.
- Browser: experimental with lower budgets and asynchronous APIs.

GPU layouts are explicit and asserted. `bytemuck::Pod` is used only for types with reviewed padding and validity invariants. Readback uses pooled buffers and never blocks render submission for encoding or consumers.

### 4.2 Assets

Runtime assets are immutable and addressed by cryptographic content hash. The
MVP accepts primitives first, then a bounded glTF/GLB subset with finite
positions, optional same-count finite vertex normals, optional same-count
finite f32 `TEXCOORD_0`, one bounded numeric metallic-roughness material per
mesh, and at most one shared embedded PNG base-color texture. The image subset
is static non-interlaced 8-bit RGB/RGBA, decoded under dimension, pixel,
retained-byte, decoder-working-byte, per-asset, and aggregate CPU limits into
one immutable RGBA8 value. Decoders verify declared and decoded sizes before
allocation; expanded upload vertices always reserve exactly 32 bytes for
position, unit normal, and primary coordinate, synthesizing a winding-derived
direction or exact zero coordinate when the source omits one. The local service
owns bounded CPU asset state and explicitly forwards immutable upload jobs into
renderer-owned mesh and unique texture residency; neither patches nor frames
perform implicit asset work. A caller may explicitly evict every CPU record,
queued source, decoded mesh/texture, pending upload, resident mesh, and shared
GPU texture for one content hash while leaving logical world references,
revision, replay, hash, and frame identity unchanged. Unrelated queues retain
their order; GPU destruction may complete after already-submitted work.
Recovery preserves logical content references but starts CPU and GPU asset
state empty, so callers must rehydrate exact matching bytes before dependent
rendering resumes. An opt-in storage adapter can retain
one exact source in a separate immutable bounded file, but it neither maps that
file to recovery state nor decodes, imports, uploads, or schedules the source.
Unsupported extensions and valid out-of-subset image features produce
structured diagnostics or approved proxies; malformed normal,
primary-coordinate, material, image, or over-limit data cannot proxy.
Aggregate asset status includes optional monotonic oldest-import and
oldest-upload ages without exposing source bytes, mesh keys, texture content,
or backend handles. Processing and explicit eviction remove matching age state
with the existing queue entry.

Built-in procedures are pure synchronous preparation functions. The local
service executes a supported typed request under active runtime limits and
admits its canonical output through the existing patch gateway. Procedure
requests have no world, renderer, I/O, time, or entropy authority; only the
accepted output patch and receipt enter replay. Consequently procedure output
inherits ordinary atomicity, delivery, idempotency, extraction, and recovery
semantics without creating a second mutation path.

USD remains an offline authoring boundary. KTX2, mesh optimization, spatial acceleration, and shared-memory delivery are later measured additions rather than foundation dependencies.

## 5. Security and reliability

All agent data, labels, assets, procedures, and transport messages are untrusted. The security baseline includes:

- pre-decode byte caps, bounded collections/nesting, entity/operation/pixel quotas;
- strict asset decoded-size, vertex/index, texture, and GPU-residency limits;
- explicit content-hash-wide asset reclamation with exact released-resource
  accounting and no hidden retry or pressure policy;
- no arbitrary native shaders or plugins;
- stable ownership scopes and expected generation/revision checks;
- scene text treated as data with provenance, never privileged instructions;
- zero or reinitialize recycled observation buffers before crossing trust scopes;
- validate exact observation kind, count, canonical value layout, envelope
  length, and metadata-bound integrity before allocating decoded payload
  vectors; require transports to cap frames before buffering;
- append-only replay integrity, bounded integrity-checked recovery envelopes,
  create-new bounded recovery and exact-hash asset-source files with
  path-redacted failures, and secret-free canonical events;
- optional Wasm/model execution isolated behind explicit capability and resource limits.

Failure behavior is controlled: invalid requests do not mutate state; device
loss terminates the affected instance cleanly; lower-level replay inspection
stops at the last verified entry; complete service restoration rejects any
invalid tail before GPU initialization; optional observers/models degrade
without affecting world or render correctness.

## 6. Performance and observability

Measure latency as separate spans: decode, validate, compile, patch validate,
patch commit, transform propagate, render extract, upload, asset eviction,
encode, GPU frame, observation copy, feedback enqueue, and delivery. Relevant
spans carry request, transaction, revision, frame, and agent identifiers with
bounded cardinality. The current caller-driven service implements only
on-demand aggregate oldest-pending age for its four bounded work lifecycles.
Those values use a monotonic process-local clock, are sampled only by status or
debug inspection, and never enter durable or causal identity. Active-operation
spans, exporters, logging, alerts, and production thresholds remain deferred.

The controlled `world-create-empty-v1` fixture also has an opt-in CLI
schema-version-one JSON view. It reports fixed fixture/profile/sample metadata
and integer nanosecond distributions, declares `informational_only: true`, and
includes no hardware identity or upload behavior. The default human report is
unchanged. This view does not establish a performance threshold or replace a
dated controlled baseline.

The canonical `canonical-mvp-v1` scenario has a separate opt-in CLI
schema-version-one JSON proof. It identifies the fixed 64x64 profile, selected
backend-neutral adapter, successful revision/query/observation evidence, stable
IDs, pixel probe, and matching live/replayed hashes. Serialization finishes
only after the complete scenario succeeds. The default human report remains
unchanged; there is no automatic upload, exporter, or background collection.

Research targets such as 60 Hz, 3 ms p95 CPU engine work, 8 ms p95 GPU time, 8 ms for 1,000 simple operations, 30 ms for 10,000 operations, one-frame commit-to-visibility, and near-zero hot-path allocations are hypotheses until reference hardware and fixtures are recorded. Correctness gates land before performance gates; thresholds cannot be silently weakened.

## 7. External interfaces

The public surface stays small: apply imagination, apply a supported built-in
procedure through an ordinary patch, apply patch, query scene,
request observation, subscribe to bounded feedback, explain compilation,
explicitly encode or decode one bounded metadata-bound observation payload,
capture complete or exact-revision local recovery state, restore it into a
fresh service, explicitly persist/load one immutable local recovery file or
one independent exact-hash asset-source file, inspect one recovery file through
the complete CPU restoration preflight without GPU initialization and emit an
optional versioned CLI JSON report, inspect one asset-source file through its
bounded exact-hash load without decoding or GPU initialization and emit an
optional versioned CLI JSON report, run the
controlled CPU measurement with an
optional versioned CLI JSON report, run the canonical unattended scenario with
an optional versioned CLI JSON proof, revert live recorded state,
resolve assets, and explicitly evict one content hash from CPU/GPU residency.
Initial
implementation can use in-process Rust types and
canonical JSON fixtures. Protobuf/gRPC, MCP, local shared memory, and QUIC are
adapters introduced after the core semantics are tested.

Public schemas always declare version, maximum encoded/decoded size, collection/string/nesting limits, unknown-field behavior, and asset references separate from bulk bytes.

## 8. Delivery and CI constraints

The repository remains Apache-2.0. Pull requests are small, dependency ordered, locally verifiable, and avoid public APIs whose invariants are not yet tested.

Default pull-request CI uses one standard Linux runner and one quality job: workflow validation, formatting, Clippy with warnings denied, workspace tests, and docs. It has an explicit timeout, cancels superseded runs, performs no paid/external calls, and uploads no routine artifacts. Cross-platform builds, GPU validation, fuzzing, performance runs, and release packaging are separate risk-triggered or manual workflows. Paid larger runners are prohibited by policy; GPU work requires an explicitly configured self-hosted runner.

## 9. MVP acceptance matrix

| Capability | Acceptance evidence |
|---|---|
| Stable identity | Entity deletion/reuse and serialization never expose/reassign an ECS handle as external identity |
| Atomic transaction | Any invalid operation rejects the complete patch and preserves prior revision/hash |
| Idempotency | Repeated key returns the same receipt without duplicate mutation |
| Hierarchy | Cycles/depth violations reject atomically; propagation order is stable |
| Replay | Repeating accepted canonical events yields the exact logical hash |
| Restoration | A bounded integrity-checked recovery envelope round-trips exact replay/frame state; the decoded complete point restores logical/replay state and continues frame/revision causality in a fresh service |
| Historical fork | An exact retained revision restores into a fresh service without source mutation or reuse of a frame issued before capture, then continues query/observe/append causality |
| Live revert | A quiescent service builds and validates an exact historical replacement before swap, preserves the source frame frontier, clears named transient/asset state, and continues retained idempotency and ordinary branch append |
| Recovery file | A new immutable local file stores one complete bounded envelope without overwrite; bounded load rejects non-files, growth, corruption, truncation, extension, and over-limit input before restoration |
| Recovery inspection | One explicit CLI path is loaded read-only and passes the same configuration, complete-replay, authoritative-world, logical-hash, and frame-frontier preflight as restoration without adapter selection; default human output is unchanged and optional deterministic schema-v1 JSON remains aggregate and path/payload redacted |
| Controlled measurement | The fixed CPU world fixture retains its human report and emits optional fixed-layout schema-v1 integer-nanosecond distributions that are explicitly informational only and written only after measurement completes |
| Canonical scenario report | The fixed 64x64 end-to-end scenario retains its 19-line human report and emits optional fixed-layout schema-v1 adapter, revision, observation, identity, pixel, and replay evidence only after the scenario succeeds |
| Asset source file | A new immutable local file stores one bounded exact-hash source without overwrite; bounded load rejects non-files, growth, substitution, truncation, extension, and over-limit input before explicit rehydration |
| Asset source inspection | One explicit expected hash and CLI path pass the existing bounded regular-file load; the exact human report and optional fixed-layout schema-v1 JSON expose only hash and byte count after success, mutate no file or service, and perform no format decode or GPU work |
| Headless render | Outward-wound reference cuboid plus extracted plane, sphere, and bounded direct metallic-roughness directional/point-lit scenes render without a visible window |
| Machine outputs | Entity-ID probes are exact; exact unlit and tolerant direct-material color/depth plus quantized outward built-in, source-wound asset, or imported-smooth world-space normals meet declared tolerance |
| Causality | Receipt, extracted revision, rendered frame, observation, and visibility metadata agree |
| Observation payload envelope | All five payload kinds round-trip fixed version-one layouts; bounds, canonical values, metadata substitution, truncation, extension, and every-byte corruption reject before decoded output is returned |
| Overload | Queue capacity stays bounded and each delivery semantic behaves as documented |
| Pending-work age | Empty command/observation/import/upload lifecycles report no age; admitted work reports deterministic monotonic oldest age, and replacement, duplicate, rejection, processing, eviction, error, and delivery preserve exact lifecycle semantics without entering durable state |
| Asset safety | Hash mismatch, oversized geometry/image decode, malformed PNG, and unsupported features fail with structured diagnostics |
| Asset resolution | The local service explicitly imports and uploads bounded content-addressed meshes and shared textures; recovered logical references remain unavailable until exact-hash rehydration without another world mutation |
| Asset eviction | One explicit content-hash operation releases exact queued/CPU/upload/GPU mesh and shared-texture capacity while preserving unrelated order, logical references, revision, replay, hash, frame frontier, and later exact-hash rehydration |
| Procedure composition | The local service produces deterministic stable IDs, queues an ordinary generated patch without immediate mutation, and preserves query/replay/hash/idempotency behavior across restoration |
| End to end | Canonical room/table/light/camera scenario passes unattended and its human and schema-v1 JSON modes prove the same causal result |

## 10. Open decisions

CF009 resolves the initial candidate packaging and validation profile in
[ADR 0010](../adr/0010-source-first-release-profile.md): source-first, no
publication during implementation, and one controlled Windows/Vulkan runtime
entry with Ubuntu CPU build/test evidence. Wider GPU/driver support, prebuilt
artifacts, additional texture roles/tangent-space normals and the remaining visual-quality surface, remote
protocol/authentication and pre-buffer framing, tenancy, observation retention, automatic startup,
recovery-to-asset catalogs and automatic rehydration, mutable/persistent
snapshot registries, crash-atomic latest pointers, automatic
device recreation, in-place revert automation and branch coordination, log
rotation, recovery-inspection profile selection, broader diagnostic schemas,
including schemas beyond the versioned recovery, controlled-measurement, and
canonical-scenario and asset-source-inspection CLI reports, and model policy
remain explicitly open.
Defaults in the roadmap are
planning assumptions, not production commitments.

## 11. Verified foundation references

- `wgpu` 30.0.0: <https://docs.rs/crate/wgpu/latest>
- `winit` 0.30.13: <https://docs.rs/crate/winit/latest>
- `hecs` 0.11.1: <https://docs.rs/crate/hecs/latest>
- `glam` 0.33.2: <https://docs.rs/crate/glam/latest>
- Rust 2024 edition: <https://doc.rust-lang.org/edition-guide/rust-2024/index.html>
- GitHub Actions billing and standard-runner policy: <https://docs.github.com/en/billing/concepts/product-billing/github-actions>
