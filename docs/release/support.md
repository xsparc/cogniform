# Release integrity and support policy

Cogniform currently has no published or supported release. The workspace's
`0.1.0-rc.1` version is an unpublished source-candidate identity, not a
download, supported security line, production boundary, or stable API.

This policy defines the contract that applies only after a candidate is
published. The [source release-candidate checklist](release-candidate.md)
remains the maintainer procedure and authority boundary.

## Official source assets

For `v0.1.0-rc.1`, the only official Cogniform release assets are:

- `cogniform-0.1.0-rc.1.tar`; and
- `cogniform-0.1.0-rc.1.tar.sha256`.

They are distributed only as assets attached to the `v0.1.0-rc.1` release in
`xsparc/cogniform`. GitHub's automatically generated `.zip` and `.tar.gz`
source downloads are convenience exports and are not covered by Cogniform's
project-owned checksum or raw-tar verification.

Before a draft is created, GitHub release immutability must be enabled for the
repository. The maintainer creates the draft against the already reviewed
annotated tag, uploads both exact verified files, reviews the final draft, and
publishes it as a prerelease only under a separate authorization. Publication
locks the tag and assets and creates GitHub's signed release attestation.

Repository-setting, tag, archive, draft, upload, and publication actions are
six separate authority gates. Approval of an earlier action never authorizes a
later one. Release operations remain manual; no pull-request or ordinary CI
job receives release-write, identity-token, or attestation permissions.

GitHub documents the immutable-release protection and draft-first workflow in
[Immutable releases](https://docs.github.com/en/code-security/concepts/supply-chain-security/immutable-releases).

## Consumer verification

Use a GitHub CLI version that provides `gh release verify` and
`gh release verify-asset`. Create and enter an empty directory, then run every
command below before extracting or building the tar:

```text
gh release verify v0.1.0-rc.1 --repo xsparc/cogniform
gh release download v0.1.0-rc.1 --repo xsparc/cogniform --pattern cogniform-0.1.0-rc.1.tar --pattern cogniform-0.1.0-rc.1.tar.sha256
gh release verify-asset v0.1.0-rc.1 cogniform-0.1.0-rc.1.tar --repo xsparc/cogniform
gh release verify-asset v0.1.0-rc.1 cogniform-0.1.0-rc.1.tar.sha256 --repo xsparc/cogniform
sha256sum --check cogniform-0.1.0-rc.1.tar.sha256
```

All five commands must succeed. Confirm that `gh release verify` identifies
the `xsparc/cogniform` release, tag `v0.1.0-rc.1`, the commit named in the
release notes, and both exact asset names. The two asset checks prove that the
local files match subjects in that release's signed attestation. The final
check independently proves that the downloaded tar has the SHA-256 recorded
by the separately attested sidecar.

Do not substitute a generated source download, omit either asset attestation,
or continue after a mismatch. Delete the local files, report the discrepancy
privately through [the security policy](../../SECURITY.md), and wait for a new
candidate or maintainer guidance. Successful integrity checks establish
identity and unchanged bytes; they do not make the source safe to execute or
expand Cogniform's documented trust boundary.

GitHub's matching consumer procedure is documented in
[Verifying the integrity of a release](https://docs.github.com/en/code-security/how-tos/secure-your-supply-chain/secure-your-dependencies/verify-release-integrity).

## Support lifetime

Only the latest published Cogniform release candidate is eligible for security
fixes. Support starts when that candidate is published and ends immediately
when either:

1. a newer Cogniform release candidate is published; or
2. the maintainers declare the candidate withdrawn in its release notes and
   `SECURITY.md`.

There is no minimum support duration, response or remediation SLA, bug bounty,
general maintenance promise, or security backport for a superseded or
withdrawn candidate. Prerelease APIs and formats may change in a later
candidate; consumers must pin and verify an exact version.

An eligible fix is released only as a new incremented candidate after ordinary
review, complete candidate validation, and separately authorized immutable
publication. Maintainers never move or reuse a published tag and never replace
or delete a published asset to deliver a fix. A serious unresolved issue can
cause immediate withdrawal without a replacement date.

Support covers only security defects in the exact published source candidate
within its documented local single-user profile. It does not turn the CLI,
stdio adapters, renderer, persistence formats, or prerelease Rust APIs into a
remote, multi-tenant, production, compatibility, availability, or data-
protection boundary. The candidate notes and threat model remain part of the
support scope.

The current OpenSSF baseline calls for release-verification instructions and
explicit support duration/end-of-updates statements; see the
[OSPS Baseline 2026.02.19](https://baseline.openssf.org/versions/2026-02-19).

## Security reports

Report suspected vulnerabilities privately as described in
[`SECURITY.md`](../../SECURITY.md). Do not post exploit details, credentials,
private data, or an uncoordinated disclosure in a public issue or discussion.
