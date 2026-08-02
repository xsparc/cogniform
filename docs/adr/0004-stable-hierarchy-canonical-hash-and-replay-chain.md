# ADR 0004: Stable hierarchy, canonical hash, and replay chain

- Status: Accepted
- Date: 2026-08-02
- Task: CF003

## Context

Hierarchy and replay must remain deterministic without exposing ECS handles or
making sparse transform edits clone the complete ECS. Logical hashes need a
fully specified representation that excludes derived and operational state.
Replay loading must be bounded and must not mistake a corrupt final write for a
complete valid history.

Three implementation choices were considered for hierarchy and transforms:

1. store parent relations as ECS-handle components and rebuild children by ECS
   iteration;
2. add a general graph and math dependency; or
3. keep stable-ID parent/child indexes beside the ECS and own the small matrix
   composition needed by this slice.

The first option would make deterministic ordering depend on backend handles.
The second adds broad surface before renderer extraction needs general math.
The third keeps the public boundary stable and the current capability narrow.

For hashing, a custom non-cryptographic digest is too weak for integrity
evidence and implementing SHA-256 locally would create avoidable cryptographic
maintenance risk. `sha2` 0.11.0 is a maintained RustCrypto implementation with
MIT/Apache-2.0 licensing and a small, locked, vendored dependency closure.

## Decision

Keep hierarchy in reciprocal `BTreeMap`/`BTreeSet` indexes keyed only by
`StableEntityId`. Preflight applies ordered operations to a bounded overlay and
a copy-on-write parent map, then validates the final transaction graph when a
create, delete, or effective reparent changes topology. Component-only patches
borrow the existing hierarchy. A parent may only be selected while live.
Deleting a parent is rejected unless its children are deleted or detached
elsewhere in the same atomic patch. Cycles and depths above
`WorldConfig::max_hierarchy_depth` reject the complete patch.

`LocalTransform` remains authoritative. Every live entity has a cached
column-major `WorldTransform`; an entity without a local transform contributes
identity. Dirty roots are semantic local-transform or parent changes plus new
entities. Their descendants are collected in stable order, sorted by
root-relative depth and stable ID, and recomputed parent before child under one
new transform generation. Unaffected branches retain their generations.
Quaternion input is normalized for derived matrix composition. Non-finite
derived output rejects the patch during preflight.

Canonical logical scene hashing uses SHA-256 with an explicit domain and format
version. It encodes entity count, stable IDs, optional stable parent IDs, and
versioned component keys and values in sorted order. Integers and normalized
raw `f32` bits are big-endian. Revision, ECS handles, cached world transforms,
generations, timing, receipts, idempotency storage, and GPU state are excluded.

`cogniform-replay::RecordedWorld` is the only mutable entry point for a world
paired with a replay log. It canonicalizes a patch and checks entry, frame, and
total log bounds before applying a new idempotency key. Accepted patches append
exactly once; idempotent repeats do not append. Each entry records sequence,
patch, visible-frame estimate, previous/new revision, previous/new logical
hash, predecessor entry hash, and its SHA-256 entry hash.

The version-one byte stream has a fixed header followed by unsigned
big-endian length-prefixed entries. Embedded patches must decode within the
active protocol limits and re-encode byte-for-byte canonically. Loading returns
the longest verified prefix plus a typed tail diagnostic for truncation,
malformed framing, limit violations, noncanonical data, sequence/revision gaps,
or hash mismatches. This slice owns the portable bytes and in-memory log; it
does not perform filesystem, object-store, or network I/O.

The exact-pinned `sha2` dependency disables default features. Its locked closure
is `cfg-if`, `cpufeatures`, `digest`, `block-buffer`, `crypto-common`,
`hybrid-array`, `typenum`, and the target-specific `libc` edge used by CPU
feature detection. It performs no network or telemetry calls. Some dependency
targets contain reviewed unsafe CPU detection or optimized hash code; unsafe
code remains forbidden in Cogniform-owned crates.

## Consequences

- Public hierarchy, transform, snapshot, and replay APIs contain stable values,
  never ECS handles.
- Final-graph validation is bounded by the configured entity and hierarchy
  limits; transform propagation work is restricted to changed branches.
- Identical logical state hashes identically even when reached at another
  revision, while parentage and authoritative component bits affect the hash.
- Replay detects accidental missing, reordered, modified, or incomplete data
  and never presents an unverified tail as accepted history.
- The replay chain is integrity evidence, not authentication; an adversary able
  to replace a log can recompute an unkeyed chain. Signing and durable storage
  require a later approved boundary.
- General transform math, snapshots across incompatible versions, external
  persistence, reversion UI, rendering, and remote delivery remain out of
  scope.
