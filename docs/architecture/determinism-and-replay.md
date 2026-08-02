# Deterministic hierarchy and replay

Status: hierarchy, derived transforms, logical hashing, and in-memory replay
implemented by CF003.

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
`ReplayLog::load_prefix` applies total, per-entry, count, and protocol limits
before allocation or decoding and returns both:

- the longest complete verified prefix; and
- an optional `ReplayTailError` with the byte offset and stable failure kind.

Callers decide how to persist or replace the bytes. A tail error must be
reported and the suffix must not be replayed. `ReplayLog::replay` starts from an
empty world, checks every recorded pre-state, applies each patch, and compares
the resulting revision and logical hash after every event.
