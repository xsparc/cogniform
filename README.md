# Cogniform

Cogniform is a deterministic, headless-first 3D scene-materialization engine in
Rust. It is being built to turn bounded agent intent into atomic revisioned
worlds, render machine-readable observations, and link feedback to the exact
scene revision that produced it.

> [!IMPORTANT]
> Cogniform is an early core, not a complete service or general-purpose 3D
> engine. Public contracts, the atomic authoritative world, deterministic
> hierarchy/hash/replay, compact render extraction, bounded headless cuboid
> rendering, and revision-linked color/depth/entity-ID/visibility plus
> quantized flat world-space normal observations are implemented. A bounded
> in-process gateway, exact logical queries, and a
> deterministic primitive imagination compiler are also available. The current
> asset baseline adds content-addressed GLB geometry, bounded caller-driven CPU
> decode and GPU upload, plus pure seeded cuboid-grid procedures. A bounded
> local typed service and CLI now run the canonical unattended MVP flow,
> verify replay to the same logical hash, and restore a fresh service from a
> complete caller-owned in-memory recovery point. That point has a deterministic
> bounded envelope for portable corruption detection, without claiming
> encryption, authentication, or durable storage. A caller can also capture an
> exact retained revision as a fresh-service historical fork without mutating
> the source or reusing a frame identity issued before capture. In-place revert,
> cross-branch frame coordination, remote
> transport, durable persistence, and release packaging have not landed yet.
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
| `cogniform-compiler` | Pure seeded primitive imagination compilation and explanations |
| `cogniform-assets` | Content-addressed GLB admission, strict bounded decoding, and immutable upload jobs |
| `cogniform-procedural` | Pure seeded built-in procedures that emit ordinary scene patches |
| `cogniform-world` | Authoritative world state and transactional mutation |
| `cogniform-replay` | Canonical events, integrity, logical hashing, and replay |
| `cogniform-renderer` | Headless GPU ownership and color/depth/normal/identity outputs |
| `cogniform-engine` | Bounded orchestration and revision/frame correlation |
| `cogniform-cli` | Local sample, replay, and diagnostic commands |

See the [software design document](docs/architecture/software-design-document.md),
[implementation plan](docs/roadmap/development-implementation-plan.md), and
[architecture decisions](docs/adr/README.md) for the authoritative direction.
The [core contract guide](docs/protocol/core-contracts.md) documents the current
schema and validation boundary. The
[determinism and replay guide](docs/architecture/determinism-and-replay.md)
documents hierarchy, transform, hash, and recovery behavior. The
[headless renderer guide](docs/renderer/headless-reference-scene.md) documents
the current offscreen targets, backend boundary, probes, and limitations. The
[extraction and observation guide](docs/renderer/incremental-extraction-and-observations.md)
documents sparse world updates, frame causality, pressure behavior, and owned
machine-readable payloads. The
[local gateway guide](docs/protocol/local-gateway-and-imagination.md) documents
command admission, idempotency, deterministic compilation, and logical queries.
The [GLB asset guide](docs/assets/glb-subset.md) documents exact source
admission, the approved format subset, lifecycle, capacity limits, proxy policy,
and controlled GPU validation. The
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

The controlled CPU fixture is informational and should be run in the optimized
profile:

```text
cargo run --release -p cogniform-cli --locked --offline -- measure-world
```

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
