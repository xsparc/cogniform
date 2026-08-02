# ADR 0007: Pure imagination compiler and bounded local gateway

- Status: Accepted
- Date: 2026-08-02
- Task: CF006

## Context

Cogniform needs its first agent-facing command boundary without introducing a
public network service, authentication scheme, generated transport, or model
dependency before those contracts are ready. The boundary must admit explicit
patches and minimal semantic imaginations, preserve world transaction
ownership, and implement all three delivery semantics without an unbounded
backlog. Semantic compilation must be reproducible from an immutable scene
view and must explain defaults, substitutions, and unresolved constraints.

Three package layouts were considered:

1. add separate gateway, compiler, transport, and session crates together;
2. place gateway and compiler implementation inside `cogniform-engine`; or
3. add a pure compiler crate while keeping the in-process gateway at the
   existing engine composition boundary.

The first option would turn an offline semantic milestone into premature
transport and authentication architecture. The second would make the compiler
depend on engine lifecycle and GPU construction, weakening deterministic unit
testing and future reuse. The third exercises a real dependency boundary
without claiming that the eventual external service topology is settled.

## Decision

`cogniform-protocol` owns version-one `ImaginationEnvelope`, compilation
budgets, primitive entity descriptions, supported relation/constraint values,
and exact-revision logical query/result contracts. Their canonical JSON
projection uses the same bounded decoder and checked-in byte fixtures as scene
patches. Runtime limits now bound imagination entities, relations, constraints,
and query results before compiler or query-result growth.

`cogniform-compiler` depends only on protocol values and the already-pinned
SHA-256 implementation. `DeterministicCompiler::compile` receives an immutable
stable-ID scene view. It performs no world mutation, rendering, I/O, model
call, clock access, ambient randomness, or global lookup. Entities are
normalized by local key. Missing names, transforms, materials, and stable IDs
use documented deterministic defaults. Derived IDs hash a domain separator,
imagination ID, explicit seed, local key, and bounded collision attempt.
Unavailable preferred IDs use the same derivation path and produce a
structured substitution decision.

The initial relation subset is deliberately small:

- `Parent` becomes a stable-ID reparent operation;
- `Above` resolves an explicit positive-Y translation from scale-aware
  axis-aligned primitive bounds;
- `RightOf` resolves an explicit positive-X translation from scale-aware
  axis-aligned primitive bounds.

Relations refer only to entities declared by the same imagination. One entity
cannot receive both hierarchy and spatial placement in this subset, or more
than one placement/parent assignment. Unknown references, self-relations,
conflicts, hierarchy/placement cycles, rotated spatial participants, non-finite
derived transforms, and failed `EntityExists`/`EntityAbsent` preconditions
produce ordered typed unresolved records. Spatial anchors must remain roots;
full rotated or parent-relative bounds require the later spatial subsystem. Any
unresolved record suppresses the entire patch.

`LocalGateway` remains in `cogniform-engine` and owns one engine through public
methods; it never receives mutable ECS or GPU handles. It has two explicit
bounds: uncommitted command capacity and total queued-plus-completed unique
idempotency capacity. The latter cannot exceed the world's accepted-result
retention.

Admission is exact:

- `MustApply` appends in order or returns typed capacity failure.
- `LatestWins(key)` replaces an older uncommitted command with the same key in
  place, preserving queue depth and position. Without a matching key, normal
  capacity failure applies.
- `BestEffort` appends when both capacities are available and otherwise returns
  an explicit dropped outcome.

An identical queued idempotency key is not duplicated. A key bound to a
different command is rejected. A domain-separated canonical-command digest,
rather than the complete command payload, is retained for equality checks.
Accepted patch, imagination, and unresolved compilation results are retained
in a bounded map and returned immediately on exact replay; apply receipts are
marked `IdempotentReplay`. Failed world or compiler operations do not consume
accepted-result capacity. Gateway debug output reports aggregate counts and
never formats queued command content.

Logical scene queries execute immediately because they are read-only. They
require the exact current revision, use bounded unique filters, return entities
and components in canonical order, and fail if the complete result exceeds the
declared limit rather than silently truncating.

## Consequences

- The same imagination, immutable stable-ID view, limits, and seed produces the
  same normalized patch bytes. Input entity order does not affect patch order.
- The compiler is not a natural-language parser or general constraint solver.
  Existing-scene spatial anchors, arbitrary inequalities, assets, procedures,
  and model-assisted choices require later approved contracts.
- Gateway progress is caller-driven through `process_next`; CF006 introduces no
  hidden worker, socket, authentication state, persistence, or background
  runtime.
- Queue depth and admitted/superseded/dropped/rejected counters are observable
  with bounded cardinality. Counter saturation cannot affect correctness.
- Wall-clock queue-age telemetry is deferred to the scheduled service boundary;
  this caller-driven in-process gateway owns no clock or background worker.
- Query implementation currently reads a bounded logical world snapshot.
  Indexed/spatial query acceleration remains a measured later optimization.
- The workspace remains unpublished at `0.0.0`; no release or compatibility
  promise is made by this internal milestone.
- The existing single GitHub Actions job is unchanged. Compiler, overload,
  protocol, and query tests run in ordinary CI; the complete gateway-to-engine
  integration remains an explicit controlled-adapter test.
