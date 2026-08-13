# ADR 0052: Immutable source release and support contract

- Status: Accepted
- Date: 2026-08-13
- Task: CF052

## Context

ADR 0050 defines one deterministic project-owned source tar and SHA-256
sidecar, and ADR 0051 gives the still-unpublished workspace the matching
`0.1.0-rc.1` identity. The remaining publication boundary needs to prevent a
release tag or asset from being replaced after consumers have accepted it,
give consumers an exact verification path, and state how long an early
candidate can receive security updates.

GitHub release immutability locks a published release's tag and assets and
automatically creates a signed release attestation. GitHub recommends creating
the release as a draft, attaching every asset, and publishing only after the
draft is complete. Its verification commands apply to uploaded release assets,
not GitHub's generated source downloads. That matches Cogniform's existing
decision to distribute one independently verified uncompressed tar rather than
trust a regenerated compressed archive.

The OpenSSF OSPS Baseline also calls for consumer integrity/authenticity
instructions and an explicit support scope, duration, and end-of-security-
updates statement. The current `SECURITY.md` truthfully says that no release is
supported, but it does not yet define the rule that would apply after the first
candidate is published.

## Decision

Before any Cogniform release draft is created, a maintainer must separately
authorize and enable GitHub release immutability for the repository. A source
candidate then follows distinct authorization gates for annotated tag
creation, archive preparation, draft creation, asset upload, and publication.
An approval for one gate does not authorize any later gate.

The `v0.1.0-rc.1` release may contain exactly these project-owned assets:

- `cogniform-0.1.0-rc.1.tar`; and
- `cogniform-0.1.0-rc.1.tar.sha256`.

The release is prepared as a draft against the already reviewed annotated tag.
The two exact verified assets are attached without replacement semantics, and
the completed draft is reviewed before a separate publication authorization.
It is published as a prerelease only while release immutability is enabled.
Generated GitHub source archives are convenience downloads, not official
Cogniform release assets.

Consumers verify the repository-scoped release attestation, both downloaded
assets, and the sidecar-to-tar SHA-256 relationship before inspecting or using
the source. The exact commands and expected identities live in
`docs/release/support.md`.

No release is supported before publication. After publication, only the latest
published Cogniform release candidate is eligible for security fixes. Its
support begins when it is published and ends immediately when a newer
candidate is published or the candidate is declared withdrawn, whichever
happens first. There is no minimum support lifetime, response SLA, bug bounty,
or backport promise. A fix is delivered only through a new incremented,
reviewed, separately authorized immutable candidate; a published tag or asset
is never moved or replaced.

## Consequences

- A published source candidate has a platform-signed record binding its tag,
  commit, and exact uploaded assets, while the project sidecar independently
  binds the tar bytes by SHA-256.
- Draft-first assembly keeps all assets reviewable before immutability closes
  mutation. A defective draft is discarded; a defective published candidate
  is withdrawn or superseded by a new version rather than repaired in place.
- Support remains deliberately narrow for an early volunteer prerelease. A
  consumer must pin an exact candidate and cannot infer production suitability
  or API compatibility from the existence of a release.
- Release verification uses an online GitHub attestation check plus a local
  digest check. Neither proves that extracted source is safe to execute.
- CF052 changes documentation only. It does not enable a repository setting,
  create a tag or archive, create a draft, upload an asset, publish a release,
  change CI permissions, or authorize any of those actions.

## Status

Accepted and documented by CF052. The publication and consumer contracts are
defined, but Cogniform still has no published or supported release. Repository
release immutability and every live publication gate remain unexecuted pending
separate maintainer authorization.
