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

## 3. Dependency graph

```text
CF000 -> CF001 -> CF002 -> CF003 -> CF004
  -> CF005 -> CF006 -> CF007 -> CF008 -> CF009
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

No performance threshold becomes a merge gate until reference hardware, fixture, sampling method, and baseline are versioned.

## 6. Deferred roadmap

After the MVP and only with evidence: shared-memory observation leases, authenticated gRPC/QUIC transport, Wasmtime procedures, KTX2/mesh optimization, advanced culling/batching, model bridge, Gaussian splat plugin, browser target, fleet orchestration, and high availability. Each requires a new design decision and approved task rather than silently entering an MVP PR.
