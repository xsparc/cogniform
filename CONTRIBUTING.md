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
5. Use placeholders in examples. Never paste a real token, password, private
   endpoint, local home path, production record, or credential-bearing URL.

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

The Cargo commands use the locked dependency sources checked into `vendor/` and
run offline without contacting crates.io. Do not edit vendored code directly;
manifest changes require a fresh locked vendor operation and the dependency
review described below.

GPU conformance and performance measurements are controlled evidence, not
ordinary pull-request CI. Run them only when the changed risk or a release-
candidate review requires them, and report the adapter, profile, fixture,
sampling method, hardware, and limitations:

```text
cargo test --release -p cogniform-renderer --tests --locked --offline -- --ignored
cargo test --release -p cogniform-engine --tests --locked --offline -- --ignored
cargo run --release -p cogniform-cli --locked --offline -- scenario
cargo run --release -p cogniform-cli --locked --offline -- measure-world
```

Do not turn one-machine timing into a merge threshold or silently replace a
recorded baseline. Follow the
[validation baseline](docs/operations/validation-baseline.md) and
[source release-candidate checklist](docs/release/release-candidate.md).

After staging a change, scan exactly what would be committed:

```text
python scripts/check_public_repo.py --staged
```

Use `python scripts/check_public_repo.py --all` to scan the complete tracked
tree. The check reports a rule identifier and path, never the matched value.
GitHub secret scanning and push protection provide provider-aware detection at
the repository boundary; do not bypass a block merely to make a push succeed.

The candidate package identity is also checked without installing a Python
package:

```text
python tests/release/test_package_policy.py
python scripts/check_package_policy.py --repository . --expected-version 0.1.0-rc.1
```

All workspace members must inherit the shared version and keep
`publish = false`. First-party workspace dependencies use exact candidate
requirements and their reviewed local paths; member manifests inherit those
declarations. Any later candidate-version change must update the root manifest,
lockfile, quality invocation, notes, and evidence in one approved slice. A
version edit does not authorize a tag, archive, upload, or publication.

When a release-candidate preparation task is separately approved and an exact
annotated maintainer tag already exists at clean `HEAD`, prepare and recheck the
actual source asset with `scripts/source_candidate.py` as documented in the
[release checklist](docs/release/release-candidate.md). Use two absent targets
in one existing caller-owned directory outside both the worktree and Git
directory. Do not weaken its fixed prefix, 256 MiB/20,000-member limits,
create-new behavior, raw archive checks, or exact sidecar. The tool does not
create a tag, change the already reviewed candidate version, upload an asset,
or authorize publication.

If a real credential is found, revoke or rotate it first, remove it from every
pending commit and working copy, and report the incident privately. Deleting a
file in a later commit does not remove a secret from public history.

Git also publishes commit author and committer names and email addresses. Use a
GitHub-provided no-reply address before committing if you do not intend to make
your personal address part of the public repository history.

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
