# Deterministic hierarchy and replay

Status: hierarchy, derived transforms, logical hashing, in-memory replay, a
portable recovery-point envelope, and exact-revision recovery forks are
implemented through CF014; CF017 adds quiescent live replacement from an exact
prefix, and CF018 adds explicit immutable bounded local recovery files.

## Hierarchy and transforms

The authoritative world stores parent relations only as stable entity IDs.
`AuthoritativeWorld::parent_id` distinguishes a missing entity from a root, and
`AuthoritativeWorld::children` yields stable-ID order. Snapshots include the
optional parent ID; no public type contains an ECS handle.

The default maximum hierarchy depth is 256 parent edges. A patch may reparent,
detach, delete, and create multiple entities, but its final graph must contain
only live endpoints, remain acyclic, and fit the active depth bound. Failure
preserves the revision, hierarchy, components, logical hash, and cached
transform generations.

`AuthoritativeWorld::world_transform` returns a derived column-major 4-by-4
matrix and the generation in which that entity was last recomputed. Missing
local transforms behave as identity. A local or parent change invalidates only
that entity and its descendants; stable depth/ID ordering guarantees every
parent is available before its children.

## Logical hash

`AuthoritativeWorld::logical_hash` and `WorldSnapshot::logical_hash` produce the
same version-one SHA-256 digest. The hash includes stable IDs, parent IDs, and
all authoritative component fields in versioned order. It excludes the scene
revision and every derived or operational value, so two worlds with identical
logical state hash identically even when their histories differ.

The encoding is not the replay byte format. It is an internal versioned binary
hash preimage with an explicit domain, big-endian integers, and big-endian raw
bits from protocol floats. Protocol validation already rejects non-finite
numbers and normalizes negative zero.

## Recording and recovery

Use `RecordedWorld` when accepted mutations require replay evidence. Its world
reference is read-only. Construction rejects a total byte bound too small for
the mandatory stream header; `apply_patch` performs bounded canonical encoding
and log admission before the authoritative mutation. A newly accepted patch
adds one entry. An idempotent repeat returns the recorded world receipt without
adding another entry.

`CogniformEngine` owns a `RecordedWorld`, so engine and local-service patches
cannot bypass accepted-event recording. It exposes immutable verification and
owned stream-copy methods while keeping the wrapped world mutable only through
the recorded apply path.

`ReplayLog::verify` checks the complete in-memory sequence and hash chain.
`ReplayLog::to_bytes` emits the portable version-one stream.
`ReplayLog::to_bytes_through_revision` emits a complete standalone prefix
ending at one retained revision. Revision zero is the valid header-only empty
stream. Because every accepted patch advances the authoritative revision once,
the requested revision selects an exact contiguous entry count. A revision
newer than the retained log returns `ReplayRevisionError` with the requested
and latest values rather than truncating or guessing.
`ReplayLog::load_prefix` applies total, per-entry, count, and protocol limits
before allocation or decoding and returns both:

- the longest complete verified prefix; and
- an optional `ReplayTailError` with the byte offset and stable failure kind.

Callers decide how to persist or replace the bytes. A tail error must be
reported and the suffix must not be replayed. `ReplayLog::replay` starts from an
empty world, checks every recorded pre-state, applies each patch, and compares
the resulting revision and logical hash after every event.

## Recovery-point envelope

`EngineRecoveryPoint::to_envelope_bytes` binds complete replay bytes and the
next unreserved renderer `FrameId` into one deterministic version-one byte
sequence. The envelope uses big-endian fixed-width fields, an explicit replay
length, and a domain-separated SHA-256 digest. Repeated encoding of the same
point is byte-identical.

`EngineRecoveryPoint::from_envelope_bytes` applies the caller's `ReplayConfig`
bound to the complete input, validates header, version, replay length, exact
total length, non-zero frame, and digest from the borrowed slice, and only then
copies the replay payload. It rejects trailing as well as truncated input.
Decoding establishes envelope integrity only: a writer able to replace both
payload and digest is not authenticated, and the replay contents remain
plaintext caller data.

After decoding, `LocalService::restore` still performs complete replay-format,
protocol, hash-chain, world-transition, and frame-continuity validation before
adapter selection. The envelope neither accepts a verified prefix nor performs
filesystem I/O, automatic startup, encryption, or authentication by itself. See
[ADR 0013](../adr/0013-versioned-recovery-point-envelope.md).

## Immutable local recovery files

`cogniform-storage::RecoveryFileStore` composes the envelope with explicit
filesystem I/O without giving that authority to the engine. `create_new`
encodes and validates before touching the path, never overwrites an existing
target or creates its parent directory, writes all bytes, synchronizes the file,
and reports typed partial-cleanup disposition after injected write/sync failure.

`load` accepts only a regular non-symlink final component at inspection time.
It bounds metadata against `EngineRecoveryPoint::envelope_byte_limit` and the
platform address space before reserving exactly that snapshot size, reads
through a fixed stack buffer, and rejects growth with an extra-byte probe. The
complete envelope must then pass exact length and digest validation; a verified
replay prefix is never returned as file-load success.

Errors retain operation and standard error kind but no path or content. Files
remain plaintext and unauthenticated. The caller owns parent-directory trust,
permissions, confidentiality, freshness, retention, and cleanup of a reported
partial file. File synchronization is not a cross-platform directory-entry or
power-loss guarantee. See the
[recovery-file guide](../persistence/recovery-files.md) and
[ADR 0018](../adr/0018-immutable-bounded-local-recovery-files.md).

## Historical recovery forks

`CogniformEngine::recovery_point_at_revision` and
`LocalService::recovery_point_at_revision` combine an exact replay prefix with
the source renderer's current next unreserved `FrameId`. Restoring that point
creates a separate fresh service at the requested logical revision, with empty
transient queues and the ordinary complete replay and frame validation.

The current frame marker is intentional. A historical logical state does not
authorize reuse of frame identities that the source may already have reserved
or exposed after that revision. The fork resumes from the source's current
frame frontier while its scene revision and replay chain resume from the chosen
historical point. Capturing the point does not mutate the source world, replay
log, renderer, or queues.

The counters become branch-local after capture. If the source and restored fork
both continue, each may independently issue the same future numeric `FrameId`;
callers that compare concurrent branches must supply branch identity or other
coordination.

Capturing this point remains a non-mutating caller-directed fork primitive. It
is not itself a live swap, snapshot registry, retention policy,
rollback-protection mechanism, persistence layer, or cross-branch frame
allocator. See
[ADR 0014](../adr/0014-exact-revision-historical-recovery-forks.md).

## Quiescent live revert

`LocalService::revert_to_revision` uses the same exact prefix and current frame
frontier to construct a fresh service under the retained validated
configuration. Commands, observations, imports, and uploads must first be
drained. Replay/world reconstruction and GPU initialization complete before the
new value replaces the old one, so any failure preserves the live source.

Success makes the target prefix authoritative and records no revert event.
Gateway response caches and queue counters reset; CPU asset records and GPU
residency are explicitly reported and cleared; logical asset references remain
and require rehydration. Prefix idempotency survives in the authoritative
world, while keys found only in the removed tail may be accepted on the new
branch. The next observation uses the source's captured frame frontier.

This is a local caller-authorized lifecycle operation, not automatic rollback,
freshness/authenticity proof, persistence, branch management, queued-work
migration, asset preservation, or device-loss recovery. See
[ADR 0017](../adr/0017-quiescent-live-revert-through-fresh-replacement.md).
