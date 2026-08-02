# ADR 0003: Atomic world preflight and stable identity

- Status: Accepted
- Date: 2026-08-02
- Task: CF002

## Context

The authoritative world must preserve stable external identity while using a
generational ECS whose internal slots may be recycled. A rejected ordered patch
must leave the revision, stable-ID index, and logical scene unchanged. The same
accepted idempotency key must not apply twice, and all retained state must have
an explicit bound.

Cloning the complete ECS before every patch would make rollback simple but
would turn sparse edits into whole-world work. Applying first and attempting to
undo later would expose partial-mutation risk and make component replacement
rollback unnecessarily complex.

## Decision

Use `hecs` 0.11.1 with only its `std` feature. Internal `hecs::Entity` handles
never cross the `cogniform-world` boundary. A `BTreeMap` maps opaque
`StableEntityId` values to live ECS handles, and every ECS entity carries a
private matching stable-ID component so index invariants can be audited.

Patch application has two phases. Preflight validates the protocol message,
base revision, revision capacity, entity and idempotency bounds, and every
ordered operation against a small overlay containing only touched entities.
It produces a complete commit plan without mutating the ECS. Commit then runs
the already-proven create, delete, set, and remove operations in message order,
updates the stable-ID index, increments the revision exactly once, and records
the accepted receipt. Hierarchy operations remain rejected until CF003 can
validate their complete graph semantics.

Idempotency records are bounded and retained for the lifetime of the world. A
repeat of the same key and transaction returns the recorded receipt with
`IdempotentReplay` status. Reusing a key with another transaction is rejected;
records are never silently evicted because eviction could permit duplicate
effects.

Logical snapshots sort entities by stable ID and components by their versioned
`ComponentKind`. They exclude ECS handles, timing, idempotency records, and
other operational state. Canonical hashing and replay persistence remain CF003
work.

## Consequences

- Invalid input is rejected before authoritative mutation, so ordinary error
  paths require no rollback.
- Preflight cost is proportional to the patch's touched entities rather than
  the whole world.
- ECS allocation failure or an internal invariant panic is process-fatal; it is
  not reported as a recoverable rejected patch after partial commit.
- Entity and idempotency capacities fail closed and can be configured per
  world.
- The locked and vendored dependency graph grows by `hecs` and its small
  runtime dependency closure. Dependency and license checks are required for
  this change.
