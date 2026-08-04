# Source release-candidate checklist

Cogniform has no published or supported release. CF009 prepares the evidence
for a future source-first `0.1.0-rc.1`; it does not authorize a version change,
tag, archive, GitHub release, crates.io publication, or deployment.

## MVP acceptance evidence

| Capability | Status | Evidence |
|---|---|---|
| Stable identity | Pass | Stable-ID serialization, deletion/reuse, compact renderer mapping, and exact entity-ID probes |
| Atomic transaction | Pass | Full preflight, late-operation rejection, randomized model, unchanged revision/hash on failure |
| Idempotency | Pass | World and gateway replay the retained result; content conflicts reject without mutation |
| Hierarchy and transforms | Pass | Cycle/depth/dangling rejection and stable parent-before-child sparse propagation |
| Deterministic replay | Pass | Canonical logical hash, chained entries, verified-prefix recovery, every-byte corruption injection |
| Headless render | Pass on validated profile | No surface/window; Vulkan cube, GLB, readback pressure, renderer-drop retirement, and canonical scenario evidence |
| Machine outputs | Pass on validated profile | Tolerant color/depth, exact stable entity ID, structured visibility, and quantized flat world-space normals following triangle winding |
| Revision causality | Pass | Receipt, extraction, renderer revision, frame, camera, observation, staleness, and visibility agree |
| Overload | Pass | Fixed capacities and tested `MustApply`, `LatestWins`, `BestEffort`, readback, asset, and replay behavior |
| Asset safety | Pass for documented GLB subset | Exact hash, strict framing/ranges/counts, truncation corpus, typed unsupported/proxy policy |
| End to end | Pass on validated profile | Room/table/light/camera create and atomic restyle, exact query, three observations, same replay hash |
| Repository and dependency hygiene | Pass | Redacted Git-object scan, secret scanning/push protection, pinned vendor/lock/action, cargo-deny |

Inside the local single-user source profile, the correctness matrix passes and
the threat model has no unresolved Critical or High residual. Medium residuals,
unsupported platforms, and operational gaps are named in the linked records.
This does not make the current `0.0.0` workspace a supported release.

## Required evidence before proposing a candidate tag

- [ ] Start from a clean, protected `main` commit whose pull-request checks and
      reviewed tree are recorded.
- [ ] Confirm no open private security advisory is a Critical or High blocker
      for the declared profile.
- [ ] Reproduce the ordinary offline format, Clippy, workspace-test, rustdoc,
      public-tree, and dependency-policy checks.
- [ ] Reproduce all ignored engine/renderer conformance tests on at least one
      matrix entry named in the compatibility baseline.
- [ ] Re-run `measure-world` in release mode and append rather than overwrite
      the dated baseline if hardware, fixture, or result materially changes.
- [ ] Review `CHANGELOG.md`, the threat model, failure/recovery guide, support
      matrix, known limitations, and license from the exact candidate tree.
- [ ] Change the shared workspace version from `0.0.0` to the explicitly
      approved candidate version without changing `publish = false`.
- [ ] Build the source archive from the annotated candidate tag, not a dirty
      working tree, and verify it includes `Cargo.lock`, `vendor/`, docs, tests,
      `LICENSE`, and no ignored local orchestration state.
- [ ] Run the public-repository scan against the archive file list and content,
      then compute and independently verify its SHA-256 checksum.
- [ ] Obtain maintainer approval for the exact tag, archive hash, release notes,
      support statement, and residual risks.

## Publication procedure

Publication is deliberately manual and requires a new explicit authorization.
After every checklist item passes, the maintainer may create an annotated tag,
produce the source archive and checksum, and publish a prerelease entry that
links this evidence. Do not upload binaries, containers, symbols, runtime logs,
benchmark artifacts, observations, replay streams, or private test data.

The GitHub release must be marked prerelease and state:

- source-only, early local evaluation;
- validated Windows/Vulkan profile and build-only Ubuntu evidence;
- no remote service, authentication, persistence, production SLA, or semver-
  stable crates.io API;
- normal output is flat and quantized, with no imported smoothing, normal maps,
  or tangent-space contract; unvalidated platforms/backends remain
  unsupported; and
- performance figures are one-machine informational measurements.

If a candidate is defective, close or supersede its release entry as
appropriate and issue a new incremented candidate after review. Do not move a
published tag or replace an archive under the same version.

## Current disposition

The implementation and evidence are suitable for a separately reviewed source
release-candidate preparation task. No tag or release is created by CF009.

See [ADR 0010](../adr/0010-source-first-release-profile.md), the
[validation baseline](../operations/validation-baseline.md), the
[failure guide](../operations/failure-and-recovery.md), and the
[MVP threat model](../threat-model/mvp.md).
