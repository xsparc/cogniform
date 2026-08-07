# Cogniform

Cogniform is a deterministic, headless-first 3D scene-materialization engine in
Rust. It is being built to turn bounded agent intent into atomic revisioned
worlds, render machine-readable observations, and link feedback to the exact
scene revision that produced it.

> [!IMPORTANT]
> Cogniform is an early core, not a complete service or general-purpose 3D
> engine. Public contracts, the atomic authoritative world, deterministic
> hierarchy/hash/replay, compact render extraction, outward-wound headless
> cuboid, centered-plane, fixed-sphere, and independently bounded
> directional/point direct metallic-roughness rendering, and revision-linked
> color/depth/entity-ID/visibility plus
> quantized world-space normal observations are implemented, including flat
> winding fallback and bounded imported vertex normals. Owned observation
> payloads also have an opt-in bounded version-one binary envelope that binds
> them to canonical causal metadata for future transport adapters. It detects
> corruption but supplies no listener, authentication, encryption, retention,
> or automatic delivery. A bounded
> in-process gateway, exact logical queries, and a
> deterministic primitive imagination compiler are also available. The current
> asset baseline adds content-addressed GLB geometry with optional finite vertex
> normals, one retained finite primary coordinate set, and unit-bounded numeric
> metallic-roughness materials, plus one bounded embedded PNG base-color
> texture. Exact caller-driven CPU decode and explicit GPU upload remain
> separate; sampled sRGB RGBA multiplies the imported factor and drives the
> existing direct-light path when the entity has no explicit scene material.
> The baseline also includes pure seeded cuboid-grid procedures. The local
> service now owns that bounded asset lifecycle, requires exact-hash
> rehydration after recovery, can explicitly evict all CPU/GPU state for one
> content hash without changing its logical references, and executes supported
> pure procedures through the ordinary patch admission and replay path.
> Aggregate status now reports optional monotonic oldest-pending age for
> commands, observations, imports, and uploads without exposing payloads or
> creating background telemetry. A bounded
> local typed service and CLI now run the canonical unattended MVP flow,
> verify replay to the same logical hash, and restore a fresh service from a
> complete caller-owned in-memory recovery point. That point has a deterministic
> bounded envelope for portable corruption detection. An explicit storage
> adapter can create a new immutable local recovery file and load it within the
> same bound. The CLI can inspect one such file read-only through the complete
> CPU restoration preflight, reporting only aggregate revision/frame/count/hash
> evidence without selecting a GPU or exposing the path or replay payload. It
> can separately retain one exact-hash asset source in another
> immutable bounded file so a caller can explicitly rehydrate a restored
> logical reference. Neither adapter claims encryption, authentication,
> automatic startup/rehydration, a recovery-to-asset catalog, or crash-atomic
> directory updates. A caller can also capture an
> exact retained revision as a fresh-service historical fork without mutating
> the source or reusing a frame identity issued before capture. A quiescent
> local service can also revert in place by constructing that exact historical
> replacement before swap; queued work blocks the operation and asset residency
> is explicitly cleared. Automatic rollback, snapshot retention, cross-branch
> frame coordination, remote transport, and release packaging have not landed
> yet.
> The current
> hardening baseline adds a
> threat model, fault/recovery matrix, controlled compatibility and performance
> evidence, and a source-first release-candidate checklist. No release has been
> published.

## Architecture

The workspace keeps exclusive ownership boundaries as their runtime
implementations arrive:

| Package | Intended responsibility |
|---|---|
| `cogniform-protocol` | Backend-neutral IDs, patches, receipts, limits, and observations |
| `cogniform-observation` | Owned payload values plus bounded transport-neutral binary encoding and causal integrity binding |
| `cogniform-compiler` | Pure seeded primitive imagination compilation and explanations |
| `cogniform-assets` | Content-addressed GLB admission, strict bounded geometry/primary-coordinate/material/embedded-PNG decoding, immutable upload jobs, and explicit CPU-state eviction |
| `cogniform-procedural` | Pure seeded built-in procedures that emit ordinary scene patches |
| `cogniform-world` | Authoritative world state and transactional mutation |
| `cogniform-replay` | Canonical events, integrity, logical hashing, and replay |
| `cogniform-renderer` | Headless GPU ownership and color/depth/normal/identity outputs |
| `cogniform-engine` | Bounded orchestration and revision/frame correlation |
| `cogniform-storage` | Explicit create-new and bounded-load recovery and exact-hash asset-source files |
| `cogniform-cli` | Local sample, replay, and diagnostic commands |

See the [software design document](docs/architecture/software-design-document.md),
[implementation plan](docs/roadmap/development-implementation-plan.md), and
[architecture decisions](docs/adr/README.md) for the authoritative direction.
The [core contract guide](docs/protocol/core-contracts.md) documents the current
schema and validation boundary. The
[determinism and replay guide](docs/architecture/determinism-and-replay.md)
documents hierarchy, transform, hash, and recovery behavior. The
[recovery-file guide](docs/persistence/recovery-files.md) documents explicit
local persistence, bounds, failure cleanup, and path/durability limitations. The
[asset-file guide](docs/persistence/asset-files.md) documents separate
exact-hash source persistence and caller-driven rehydration. The
[headless renderer guide](docs/renderer/headless-reference-scene.md) documents
the current offscreen targets, built-in cuboid/plane/sphere geometry, bounded
directional and point direct material lighting, backend boundary, probes, and
limitations. The
[extraction and observation guide](docs/renderer/incremental-extraction-and-observations.md)
documents sparse world updates, frame causality, pressure behavior, and owned
machine-readable payloads. The
[observation-payload envelope guide](docs/protocol/observation-payload-envelope.md)
specifies the opt-in binary layout, bounds, integrity limits, and future
transport responsibilities. The
[local gateway guide](docs/protocol/local-gateway-and-imagination.md) documents
command admission, idempotency, deterministic compilation, and logical queries.
The [GLB asset guide](docs/assets/glb-subset.md) documents exact source
admission, the approved format subset, lifecycle and explicit eviction,
capacity limits, proxy policy, texture sampling contract, and controlled GPU
validation. The
[local service guide](docs/protocol/local-service.md) documents the in-process
composition boundary and known limitations, while the
[canonical scenario guide](docs/getting-started/canonical-scenario.md) provides
the unattended MVP command and expected evidence. The
[validation baseline](docs/operations/validation-baseline.md),
[failure and recovery guide](docs/operations/failure-and-recovery.md),
[MVP threat model](docs/threat-model/mvp.md), and
[release-candidate checklist](docs/release/release-candidate.md) state the
validated profile, residual risks, and release limits.

## Build and test

Install [rustup](https://rustup.rs/). The checked-in toolchain file selects the
exact Rust version and components:

```text
cargo build --workspace --locked --offline
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features --locked --offline -- -D warnings
cargo test --workspace --all-features --locked --offline
cargo doc --workspace --no-deps --all-features --locked --offline
```

Dependency sources are checked into `vendor/`, so these commands need no
external service after the pinned toolchain is installed. On a supported
headless DX12 or Vulkan adapter, run the canonical local scenario with:

```text
cargo run -p cogniform-cli --locked --offline -- scenario
```

The command opens no window and performs no network call. It verifies the room,
table, light, camera, atomic update, exact query, color/entity-ID/visibility
causality, and replay hash contract before returning success.

Scripts can request the same completed scenario proof as compact versioned
JSON:

```text
cargo run -p cogniform-cli --locked --offline -- scenario --json
```

Consumers must require `schema_version` 1 and `scenario`
`canonical-mvp-v1`. The adapter summary and run evidence can fingerprint or
correlate the local host, so the report is opt-in and must not be uploaded or
published by default. See the
[canonical scenario guide](docs/getting-started/canonical-scenario.md).

To inspect one immutable recovery file without selecting a GPU adapter:

```text
cargo run -p cogniform-cli --locked --offline -- inspect-recovery state/checkpoint-0001.cnfr
```

This read-only diagnostic uses the fixed `default-local-64x64` profile and
prints only aggregate replay/revision/frame/hash evidence. A pass does not prove
writer authenticity, freshness, asset residency, or GPU/service readiness; see
the [recovery-file guide](docs/persistence/recovery-files.md).

Scripts can request the compact versioned report without changing the default
human output:

```text
cargo run -p cogniform-cli --locked --offline -- inspect-recovery --json state/checkpoint-0001.cnfr
```

Consumers must require `schema_version` 1. The JSON still contains potentially
sensitive aggregate hashes and counts; failures write no partial JSON to
stdout.

To verify one caller-mapped immutable asset source against its expected hash
without decoding it or selecting a GPU adapter:

```text
cargo run -p cogniform-cli --locked --offline -- inspect-asset [--json] <content-hash> assets/triangle.glb
```

The command reports only the verified lowercase hash and bounded source byte
count. Paths and payloads remain redacted and the file is not modified. A pass
does not prove format validity, renderability, writer authenticity, freshness,
or association with a recovery point; see the
[asset-file guide](docs/persistence/asset-files.md).
Without `--json`, the three-line human report is unchanged. JSON is one compact
line with CLI `schema_version` 1, `content_hash`, and `source_bytes`; consumers
must require version 1. Failures write no partial JSON to stdout.

The controlled CPU fixture is informational and should be run in the optimized
profile:

```text
cargo run --release -p cogniform-cli --locked --offline -- measure-world
```

Scripts can request the same completed measurement as compact versioned JSON:

```text
cargo run --release -p cogniform-cli --locked --offline -- measure-world --json
```

Consumers must require `schema_version` 1 and `unit` `nanoseconds`. The report
declares `informational_only: true`; it is not a release or merge threshold and
its timing values can reveal characteristics of the local host or process.

See [CHANGELOG.md](CHANGELOG.md) for the unreleased capability and limitation
summary. The workspace remains `0.0.0`, source packages remain unpublished, and
creating a tag or release requires a separate explicit maintainer action.

## Contributing and security

Read [CONTRIBUTING.md](CONTRIBUTING.md), the
[dependency policy](docs/dependency-policy.md), and
[SECURITY.md](SECURITY.md) before proposing changes. Participation is governed
by the [Code of Conduct](CODE_OF_CONDUCT.md), and project decisions follow
[GOVERNANCE.md](GOVERNANCE.md).

Cogniform is licensed under [Apache-2.0](LICENSE).
