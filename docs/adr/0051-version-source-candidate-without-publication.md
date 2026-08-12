# ADR 0051: Version the source candidate without publishing it

- Status: Accepted
- Date: 2026-08-12
- Task: CF051

## Context

ADR 0010 selected `0.1.0-rc.1` as Cogniform's first eligible source-only
candidate. CF050 then made the prospective project-owned tar and checksum
deterministic and independently verifiable, but deliberately left the workspace
at `0.0.0`. The candidate tree now needs one coherent package identity and a
reviewable release statement before a maintainer can consider creating the
annotated tag required by that tooling.

The workspace contains sixteen first-party packages and fifteen shared local
dependency declarations. A partial version edit can leave Cargo's manifest and
lock resolution inconsistent even though no package is intended for crates.io.
Relying only on the next Cargo invocation to notice that drift would make the
release policy implicit and would not prove that every member remains
non-publishable.

## Decision

Set the shared workspace version to `0.1.0-rc.1`, set every first-party
workspace dependency requirement to exact `=0.1.0-rc.1`, and regenerate the
lockfile so all sixteen first-party package entries use the same version. Keep
`publish = false` on every member. This identifies one source candidate; it
does not establish a semver-stable Rust API or make any package publishable.

Add a standard-library-only package-policy checker. It reads a bounded explicit
workspace member inventory, requires every member to inherit the shared
version and remain non-publishable, requires each crate member to have one
path-correct exact shared dependency declaration, rejects direct first-party
version/path overrides in member manifests, and requires one source-less
lockfile entry at the candidate version for every first-party package. The
existing quality job runs both disposable negative tests and the checker with
the expected candidate version.

Prepare tracked candidate notes and rerun the complete release-candidate
evidence matrix on the approved Windows/Vulkan profile. Keep the final
clean-`main`, tag, archive, immutable-release setting, upload, and publication
steps open until their separate explicit approvals and exact post-merge
identities are available.

## Consequences

- Source consumers and diagnostics now report `0.1.0-rc.1`; any consumer that
  treated `0.0.0` as an identity observes an intentional version change.
- The Rust package surface remains explicitly pre-stable and local packages
  remain unavailable from crates.io. Source users build from the complete
  project-owned candidate with the pinned toolchain, lockfile, and vendor tree.
- Future candidate or release version changes must update the shared version,
  exact local requirements, lockfile, expected quality invocation, release
  notes, and evidence together. The policy checker fails closed on partial
  drift.
- This decision creates no tag or release asset, changes no GitHub setting,
  opens no network connection, and authorizes no upload, signing, attestation,
  SBOM, binary/container/package publication, deployment, merge, or public
  release.
- After the reviewed PR is squash-merged, a maintainer must separately approve
  the exact annotated tag. The verified archive/hash and release statement then
  require another explicit approval before publication. A failed candidate is
  superseded by an incremented candidate rather than moving a published tag or
  replacing an asset.

## Status

Accepted for CF051. The workspace candidate, deterministic package-policy gate,
release notes, and pre-tag validation evidence are implemented in this slice;
tagging and publication remain open gates.
