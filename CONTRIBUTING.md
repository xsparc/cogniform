# Contributing to Cogniform

Thanks for helping build Cogniform. The project is intentionally developed in
small, dependency-ordered slices so that correctness and architectural claims
remain reviewable.

## Before opening a change

1. Read the [software design document](docs/architecture/software-design-document.md)
   and [implementation plan](docs/roadmap/development-implementation-plan.md).
2. Discuss behavior or architecture changes in an issue before investing in a
   large implementation.
3. Keep a pull request focused on one approved task or one clearly bounded fix.
4. Do not add credentials, private data, paid-service calls, deployment steps,
   or release automation.

## Local setup

Cogniform pins Rust in `rust-toolchain.toml`. Install
[rustup](https://rustup.rs/), then run:

```text
rustup show active-toolchain
cargo build --workspace --locked --offline
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features --locked --offline -- -D warnings
cargo test --workspace --all-features --locked --offline
cargo doc --workspace --no-deps --all-features --locked --offline
```

The Cargo commands run offline after the pinned toolchain and locked dependency
sources are present. CF000 has no external Cargo dependencies.

Local maintainer tools and working notes that match ignored paths are not part
of the project and must not be force-added. Record accepted outcomes in public
documentation, code, tests, issues, and pull requests so a clean clone and CI
remain self-contained.

## Code and architecture expectations

- Preserve the world, render, and service ownership boundaries.
- Keep all queues and externally supplied data bounded.
- Do not expose backend ECS or GPU handles through public contracts.
- Prefer explicit failures over silent fallback or partial mutation.
- Add tests near new behavior and document public APIs.
- Unsafe Rust is forbidden across the workspace unless a future ADR and task
  explicitly revise that policy.

## Dependencies

Read the [dependency policy](docs/dependency-policy.md) before changing a
manifest. New dependencies need a concrete boundary-level reason, compatible
licensing, a locked version, and review of their maintenance and supply-chain
impact. Wildcard, unknown-registry, and unapproved Git dependencies are denied.

## Pull requests

Describe the outcome, scope and non-scope, risks, tests actually run, and any
documentation or compatibility impact. Default CI uses one standard Linux job;
expensive cross-platform, GPU, fuzz, benchmark, and release checks are invoked
only when the change creates the corresponding risk.

Unless explicitly stated otherwise, contributions intentionally submitted for
inclusion are provided under the repository's Apache-2.0 license, as described
in section 5 of that license.
