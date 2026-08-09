# Local gateway and imagination compiler

Status: implemented by CF006 for offline in-process use; CF016 routes service
procedure output through the unchanged patch gateway, CF031 adds monotonic
oldest-pending command age to aggregate status, and CF043 gives compiler
outcomes a bounded transport-neutral value contract without changing gateway
execution. CF044 maps those existing gateway outcomes into local-session
schema version two. Before semantic admission, the quiescent service may
narrow the compiler's result bounds to the negotiated session policy; the
compiler validates complete canonical bytes and nesting before the gateway can
apply an optional normalized patch.

The current gateway is a Rust composition API, not a network service. It
accepts typed `ScenePatch` and `ImaginationEnvelope` values, owns one
`CogniformEngine`, and exposes caller-driven admission, processing, and logical
query methods. CF045's separate MCP adapter translates query and imagination
tools into these methods without changing the gateway; CF046 appends the
existing explicit-patch path with the same boundary. Authentication, general sessions,
Protobuf/gRPC, shared memory, remote delivery, persistence, and model calls are
outside this contract.

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

`GatewayQueueStats` reports current depth, optional monotonic
`oldest_pending_age_micros`, and admitted, superseded, dropped, and rejected
counters. An empty queue reports `None`. `AlreadyQueued` retains the original
age; `LatestWins` replacement starts a new age while preserving queue
position; dropped, replayed, conflicting, invalid, or capacity-rejected work
does not change retained age. Processing removes the selected timestamp with
the command. Status and debug output remain aggregate and contain no command,
key, supersession text, or system-clock timestamp. Age and counters are local
diagnostics and are not part of command fingerprints, world state, logical
hashing, replay, or recovery.

## Procedure output

`LocalGateway` has no procedure command or response variant. The local service
executes a supported pure procedure under the engine's runtime limits, then
passes its generated `ScenePatch` to `submit_patch`. Consequently delivery,
queue pressure, supersession, conflict, processing, and replay behavior are
identical to an explicitly submitted patch.

Idempotency is based on canonical output-patch bytes. An exact repeat returns
`AlreadyQueued` or `Replayed` when the gateway retains the corresponding
record; a different output under the same key returns
`IdempotencyConflict`. A restored service has no gateway response cache, so an
exact generated patch can queue once more, but world idempotency returns the
original receipt without another mutation or replay entry.

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

The complete outcome is a schema-version-one `CompilationResult` from the
dependency-neutral `cogniform-compilation` crate. It binds the imagination ID
and exact scene revision to either one normalized patch or at least one
unresolved issue, carries canonical ordered unique explanations, and supports
bounded exact LF JSON. `cogniform-compiler` preserves its original public type
paths by re-exporting the moved values. See the
[compilation result contract](compilation-results.md).

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
budget rejection. Gateway tests cover all overload semantics, exact age
retention/reset/removal, microsecond saturation, idempotency conflicts, stable
query filtering, and fail-before-truncate behavior. The controlled adapter
integration compiles, applies, queries, replays, and rejects a stale explicit
patch without duplicate mutation while proving queued and drained age status.

See [ADR 0007](../adr/0007-pure-imagination-compiler-and-bounded-local-gateway.md)
for the package and lifecycle decision and
[ADR 0016](../adr/0016-service-procedure-composition-through-ordinary-patches.md)
for service procedure composition, and
[ADR 0031](../adr/0031-monotonic-pending-work-age-status.md) for transient age
semantics, and [ADR 0043](../adr/0043-bounded-transport-neutral-compilation-results.md)
for the versioned result boundary. The separate
[ADR 0044](../adr/0044-versioned-local-imagination-session-mapping.md) records
the bounded local-session mapping, and
[ADR 0045](../adr/0045-bounded-mcp-stdio-adapter.md) records the separate MCP
stdio adaptation, and
[ADR 0046](../adr/0046-bounded-mcp-apply-patch-tool.md) records its direct
bounded patch translation.
