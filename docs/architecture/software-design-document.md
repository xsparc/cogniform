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
| `cogniform-world` | `hecs` implementation, stable-ID index, validation, atomic commit, hierarchy, transforms, queries | Depends on protocol/math; never renderer or service |
| `cogniform-replay` | Canonical event encoding, hash chain, replay and logical scene hashing | Depends on public world snapshots/events, not GPU state |
| `cogniform-renderer` | `wgpu` feature negotiation, headless targets, primitive rendering, ID/depth/normal outputs | Consumes extracted render data; never mutable world access |
| `cogniform-engine` | Bounded channels, domain lifecycle, frame/revision correlation, composition | Orchestrates through public interfaces; does not absorb domain state |
| `cogniform-cli` | Local sample client, replay and diagnostic commands | Depends on public engine/protocol interfaces only |

Gateway, semantic compiler, assets, spatial acceleration, shared memory, remote transport, Wasm, and model bridge become separate crates only when their milestone establishes an independent contract or dependency footprint.

### 2.3 Dependency direction

```text
protocol <- world <- replay
    ^          |
    |          v
    +---- render extraction -> renderer
    ^                              |
    +---------- engine ------------+
                   ^
                   |
                  CLI
```

The diagram shows allowed information flow, not permission to create circular Cargo dependencies. Shared render DTOs belong in a dependency-neutral boundary rather than making world depend on renderer.

## 3. Core contracts and invariants

### 3.1 Identity and revisions

- `StableEntityId` is an opaque 128-bit external identifier and is never a raw `hecs::Entity`.
- `SceneRevision` is a monotonically increasing 64-bit value.
- `FrameId` identifies a produced frame; frame metadata includes the latest fully extracted scene revision.
- Internal ECS handles may be reused without changing external identity semantics.
- A stable-ID index is updated only as part of an accepted transaction.

### 3.2 Scene patches

`ScenePatch` contains schema version, transaction ID, idempotency key, base revision, conflict policy, ordered operations, and declared limits. Initial operations cover create, delete, set/remove component, and reparent. Asset instantiation, procedures, and observation policy arrive in later versioned additions.

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

### 3.5 Backpressure

Every queued command declares one of:

- `MustApply`: ordered durable work; admission fails explicitly when capacity is unavailable.
- `LatestWins(key)`: a new uncommitted value supersedes an older one for the same key.
- `BestEffort`: may be dropped under configured pressure.

Queues have fixed or configuration-bounded capacity, observable depth/age/drop counters, and no hidden unbounded spill. Durable work receives typed backpressure; replaceable observations use latest-value behavior.

### 3.6 Observation causality

Every feedback envelope includes scene revision, frame ID, camera ID when relevant, observation timestamp, production latency, quality level, and staleness. An observation never claims a newer revision than its render input. Readback, encoding, delivery, and optional inference are asynchronous and may complete later while retaining source causality.

## 4. Rendering and assets

### 4.1 Renderer baseline

Use `wgpu` with built-in WGSL and negotiated adapter features/limits. The baseline is a small forward renderer with primitive meshes, camera, depth, color, entity-ID, and later normal output. Headless mode renders to textures without constructing a visible window. Optional `winit` integration is isolated from the headless core.

Feature tiers are capability-based:

- Baseline: downlevel/WebGPU-compatible limits, normal bind groups, CPU culling.
- Modern: storage buffers, indirect drawing, compute culling, larger arrays.
- Advanced native: timestamps, multi-draw or subgroup/compression features where proven.
- Browser: experimental with lower budgets and asynchronous APIs.

GPU layouts are explicit and asserted. `bytemuck::Pod` is used only for types with reviewed padding and validity invariants. Readback uses pooled buffers and never blocks render submission for encoding or consumers.

### 4.2 Assets

Runtime assets are immutable and addressed by cryptographic content hash. The MVP accepts primitives first, then a bounded glTF/GLB subset. Decoders verify declared and decoded sizes before allocation. CPU decoding and preprocessing happen off the render domain; GPU upload jobs are bounded. Unsupported extensions produce structured diagnostics or approved proxies.

USD remains an offline authoring boundary. KTX2, mesh optimization, spatial acceleration, and shared-memory delivery are later measured additions rather than foundation dependencies.

## 5. Security and reliability

All agent data, labels, assets, procedures, and transport messages are untrusted. The security baseline includes:

- pre-decode byte caps, bounded collections/nesting, entity/operation/pixel quotas;
- strict asset decoded-size, vertex/index, texture, and GPU-residency limits;
- no arbitrary native shaders or plugins;
- stable ownership scopes and expected generation/revision checks;
- scene text treated as data with provenance, never privileged instructions;
- zero or reinitialize recycled observation buffers before crossing trust scopes;
- append-only replay integrity and secret-free canonical events;
- optional Wasm/model execution isolated behind explicit capability and resource limits.

Failure behavior is controlled: invalid requests do not mutate state; device loss is restartable or terminates the engine cleanly; corrupt replay stops at the last verified entry; optional observers/models degrade without affecting world or render correctness.

## 6. Performance and observability

Measure latency as separate spans: decode, validate, compile, patch validate, patch commit, transform propagate, render extract, upload, encode, GPU frame, observation copy, feedback enqueue, and delivery. Relevant spans carry request, transaction, revision, frame, and agent identifiers with bounded cardinality.

Research targets such as 60 Hz, 3 ms p95 CPU engine work, 8 ms p95 GPU time, 8 ms for 1,000 simple operations, 30 ms for 10,000 operations, one-frame commit-to-visibility, and near-zero hot-path allocations are hypotheses until reference hardware and fixtures are recorded. Correctness gates land before performance gates; thresholds cannot be silently weakened.

## 7. External interfaces

The public surface stays small: apply imagination, apply patch, query scene, request observation, subscribe to bounded feedback, explain compilation, revert recorded state, and resolve assets. Initial implementation can use in-process Rust types and canonical JSON fixtures. Protobuf/gRPC, MCP, local shared memory, and QUIC are adapters introduced after the core semantics are tested.

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
| Headless render | Reference primitive scene renders without a visible window |
| Machine outputs | Entity-ID probes are exact; depth/normals meet declared tolerance |
| Causality | Receipt, extracted revision, rendered frame, observation, and visibility metadata agree |
| Overload | Queue capacity stays bounded and each delivery semantic behaves as documented |
| Asset safety | Hash mismatch, oversized decode, and unsupported features fail with structured diagnostics |
| End to end | Canonical room/table/light/camera scenario passes unattended |

## 10. Open decisions

Minimum GPU/driver matrix, product packaging, glTF/visual-quality subset, remote protocol/authentication, tenancy, observation retention, and model policy remain explicitly open. Defaults in the roadmap are planning assumptions, not production commitments.

## 11. Verified foundation references

- `wgpu` 30.0.0: <https://docs.rs/crate/wgpu/latest>
- `winit` 0.30.13: <https://docs.rs/crate/winit/latest>
- `hecs` 0.11.1: <https://docs.rs/crate/hecs/latest>
- `glam` 0.33.2: <https://docs.rs/crate/glam/latest>
- Rust 2024 edition: <https://doc.rust-lang.org/edition-guide/rust-2024/index.html>
- GitHub Actions billing and standard-runner policy: <https://docs.github.com/en/billing/concepts/product-billing/github-actions>
