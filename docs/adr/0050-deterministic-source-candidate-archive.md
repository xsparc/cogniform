# ADR 0050: Deterministic source-candidate archive

- Status: Accepted
- Date: 2026-08-12
- Task: CF050

## Context

ADR 0010 selected a source-first release profile and the release checklist
requires the actual source archive, not only its Git tree, to be inspected and
bound to a SHA-256 checksum. The existing public-repository safeguard reads Git
objects. It cannot establish that a separately produced tar retained every
tracked blob, omitted private local state, used stable metadata, or remained
unchanged after the checksum was written.

Git's generated tar can also depend on the selected object, `tar.umask`, and
`export-ignore` or `export-subst` attributes. Ambient global, system, or
repository-info attributes are not represented by a clean working-tree check.
GitHub's generated source downloads preserve extracted content for a stable
commit but do not promise byte-stable compression. A project-owned verified
asset is therefore required before any separately authorized publication.
Generating a second archive implementation with Python's tar library would
duplicate Git export semantics without proving the bytes Git produced, while
trusting a general-purpose extractor would leave raw extension and termination
roles uninspected. The selected boundary therefore generates with built-in Git
and verifies independently with a narrow raw parser.

## Decision

Add `scripts/source_candidate.py`, a standard-library-only local preparation
and verification tool. Both modes require an exact `refs/tags/...` name whose
ref value is an annotated tag object pointing directly to the clean current
`HEAD` commit. Lightweight tags, nested tags, branches, raw object names,
moving refs, changed `HEAD`, dirty state, and nonempty
`$GIT_DIR/info/attributes` fail closed.

`prepare` requires two absent targets in one existing directory outside the
worktree, Git directory, and common Git directory. It invokes built-in
`git archive` from the captured tag-object identity with inherited,
system/global configuration and attributes neutralized, replacement objects
and lazy object fetching disabled, `tar.umask=0022`, and the fixed
`cogniform-source/` prefix. Cleanliness checks also disable filesystem-monitor
commands and submodule traversal. Ordinary Git metadata responses are limited
to 1,048,576 bytes, while tree inventory capture is limited to 512 bytes per
allowed member; a larger response fails rather than allocating without bound.
The tool streams into one create-new uncompressed tar, enforcing 268,435,456
complete bytes, then writes one create-new sidecar as exactly lowercase
SHA-256, two ASCII spaces, the archive basename, and one LF. Any failure removes
only outputs created by that invocation; inability to prove cleanup is reported
as `cleanup_uncertain`.

Before success, the tool reopens the archive and parses its raw 512-byte tar
blocks without extraction. It permits exactly one Git global PAX `comment`
equal to the peeled commit and no member-specific or GNU extensions. It
requires canonical record padding and termination, at most 20,000 filesystem
members, one safe portable path per member, exact Git tree order and directory
closure, only regular files and directories, fixed uid/gid and owner/group,
the commit timestamp, 0644/0755 file modes, 0755 directory modes, empty
link/device roles, and exact member sizes and Git blob identities under the
repository object format. It requires the lockfile, workspace manifest, pinned
toolchain, Cargo vendor configuration, license, README, docs, tests, and
vendored sources. Non-vendor member ranges are scanned through the same public
path and content rules as the Git-object safeguard by searching a read-only
mapping of the already bounded archive, preserving matches across I/O
boundaries without copying or extracting members.

`verify` applies the same repository, archive, inventory, public-content, and
sidecar checks to existing files. Both modes emit one compact schema-version-one
report containing only the tag ref, tag-object and commit identities, Git
object format and implementation version, archive size/member count, and
SHA-256. Failures emit only a stable category. The tag ref, `HEAD`, cleanliness,
and Git version are checked again immediately before success.

## Consequences

- A maintainer can prepare and independently re-verify the exact source asset
  that may later be attached to a release instead of checksumming GitHub's
  regenerated compressed download.
- The tool proves deterministic bytes only for the recorded Git implementation
  and accepted repository content. A Git implementation change or growth past
  either hard bound requires review rather than silent fallback.
- Archive SHA-256 and Git object identities detect corruption and substitution;
  they do not authenticate the maintainer, sign the asset, establish
  provenance, protect confidentiality, or authorize publication.
- CF050 creates no tag, changes no version, opens no network connection, and
  uploads or publishes nothing. Tag creation, release immutability, signing,
  attestations, SBOMs, and release publication remain separate explicit
  decisions.
- The default quality job runs the disposable source-candidate matrix on Linux
  in addition to the existing public-tree test. No dependency, package,
  workflow permission, artifact upload, or external service is added.

## Status

Accepted and implemented by CF050. Disposable repositories cover annotated,
lightweight, nested, moving, dirty, and mismatched identities; ambient and
committed attribute effects; replacement-ref isolation; SHA-1 and SHA-256
object formats; deterministic repeat output; exact limit edges; executable,
metadata, PAX, path, type, payload, sidecar, padding, and trailing corruption;
public-path and boundary-spanning content rejection; existing outputs; final
recheck cleanup; and partial-cleanup uncertainty.
