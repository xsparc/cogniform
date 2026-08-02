# Cogniform

Cogniform is a deterministic, headless-first 3D scene-materialization engine in
Rust. It is being built to turn bounded agent intent into atomic revisioned
worlds, render machine-readable observations, and link feedback to the exact
scene revision that produced it.

> [!IMPORTANT]
> Cogniform is an early core, not a working 3D engine. The public protocol
> contracts and atomic authoritative world are implemented; hierarchy, replay,
> renderer, and service behavior have not landed yet.

## Architecture

The workspace keeps exclusive ownership boundaries as their runtime
implementations arrive:

| Package | Intended responsibility |
|---|---|
| `cogniform-protocol` | Backend-neutral IDs, patches, receipts, limits, and observations |
| `cogniform-world` | Authoritative world state and transactional mutation |
| `cogniform-replay` | Canonical events, integrity, logical hashing, and replay |
| `cogniform-renderer` | Headless GPU ownership and machine-readable outputs |
| `cogniform-engine` | Bounded orchestration and revision/frame correlation |
| `cogniform-cli` | Local sample, replay, and diagnostic commands |

See the [software design document](docs/architecture/software-design-document.md),
[implementation plan](docs/roadmap/development-implementation-plan.md), and
[architecture decisions](docs/adr/README.md) for the authoritative direction.
The [core contract guide](docs/protocol/core-contracts.md) documents the current
schema and validation boundary.

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
external service after the pinned toolchain is installed. The CLI currently
exits with a clear not-implemented message rather than suggesting runtime
capability.

## Contributing and security

Read [CONTRIBUTING.md](CONTRIBUTING.md), the
[dependency policy](docs/dependency-policy.md), and
[SECURITY.md](SECURITY.md) before proposing changes. Participation is governed
by the [Code of Conduct](CODE_OF_CONDUCT.md), and project decisions follow
[GOVERNANCE.md](GOVERNANCE.md).

Cogniform is licensed under [Apache-2.0](LICENSE).
