# Local gateway and imagination compiler

Status: implemented by CF006 for offline in-process use.

The current gateway is a Rust composition API, not a network service. It
accepts typed `ScenePatch` and `ImaginationEnvelope` values, owns one
`CogniformEngine`, and exposes caller-driven admission, processing, and logical
query methods. Authentication, sessions, Protobuf/gRPC, MCP, shared memory,
remote delivery, persistence, and model calls are outside this contract.

## Admission and idempotency

`GatewayConfig` declares two non-zero capacities:

- `command_capacity`: uncommitted patch or imagination commands;
- `idempotency_capacity`: the combined number of queued unique keys and
  retained accepted results.

Command capacity cannot exceed `RuntimeLimits::max_queue_capacity`.
Idempotency capacity must cover command capacity and cannot exceed the
authoritative world's remaining accepted-result retention when the gateway is
constructed. Typed commands must also fit the canonical encoded-message limit
before their digest is reserved. The gateway never evicts accepted results to
make room silently.

| Delivery | Available capacity | Pressure behavior |
|---|---|---|
| `MustApply` | Append in FIFO order | Typed capacity error |
| `LatestWins(key)` | Append when no matching key exists | Replace a matching uncommitted key in place; otherwise typed capacity error |
| `BestEffort` | Append | Explicit `Dropped` outcome |

An identical queued command returns `AlreadyQueued`. Reusing its idempotency key
for different content returns `IdempotencyConflict`. Once a result has been
accepted, an exact repeat returns `Replayed` immediately and does not compile
or mutate the world again. Successful receipt status changes to
`IdempotentReplay` in the replayed response. Completed records retain a
domain-separated digest of canonical command bytes instead of the complete
input payload, and gateway debug output contains only aggregate state.

`GatewayQueueStats` reports current depth and monotonic admitted, superseded,
dropped, and rejected counters. These counters are local diagnostics and are
not part of logical replay state.

## Imagination contract

An `ImaginationEnvelope` carries schema, imagination, transaction, and
idempotency identities; exact base revision; delivery behavior; explicit seed;
declared compilation/patch budgets; primitive entities; supported relations;
and scene-view constraints.

Every imagined entity has a non-empty local key and a required primitive with
positive dimensions. Name, stable ID, transform, and material may be omitted.
The compiler explains each selected default:

- display name: the local key;
- transform: zero translation, identity quaternion, unit scale;
- material: linear `(0.7, 0.7, 0.7, 1.0)`, metallic `0`, roughness `0.8`;
- stable ID: the first collision-free non-zero ID from the documented seeded
  SHA-256 derivation.

Entities are emitted in local-key order with name, transform, primitive, and
material component order. Reparent operations follow all creates in child-key
order. This normalization makes patch bytes independent of input entity order.

The relation subset is `Parent`, `Above`, and `RightOf`. Above/right-of chains
are resolved from scale-aware, axis-aligned primitive half-extents and explicit
non-negative gaps. Spatial subjects and anchors must be unrotated roots; rotated
or parent-relative bounds require the later spatial subsystem and fail here as
typed unresolved constraints. A subject cannot also be parented in the same
minimal request, and it cannot have multiple placement assignments. Relation
or constraint failure is returned as an ordered `UnresolvedConstraint`; no
partial patch is emitted.

`EntityExists` and `EntityAbsent` are the initial exact stable-ID constraints.
The scene view revision must match `base_revision`; a mismatch is a typed
compiler error before hashing or patch construction. Applying the normalized
patch still performs authoritative exact-base and atomic world preflight.

## Queries

`SceneQuery` names an exact revision, optional stable-ID and component-kind
filters, and a non-zero result limit. Empty filters select all entities or all
components. Duplicate filters are invalid. Results are stable-ID ordered with
components in component-kind order and use backend-neutral values only.

If the requested revision is stale, the query returns
`QueryRevisionMismatch`. If the complete match set exceeds the requested
limit, it returns `QueryResultCapacityExceeded`; no silent truncation or hidden
pagination state is created.

## Validation

Canonical fixtures cover imagination, query, and query-result JSON. Compiler
tests cover normalized byte stability, deterministic substitutions, spatial
resolution, unresolved references and constraints, cycles, stale views, and
budget rejection. Gateway tests cover all overload semantics, idempotency
conflicts, stable query filtering, and fail-before-truncate behavior. The
controlled adapter integration compiles, applies, queries, replays, and rejects
a stale explicit patch without duplicate mutation.

See [ADR 0007](../adr/0007-pure-imagination-compiler-and-bounded-local-gateway.md)
for the package and lifecycle decision.
