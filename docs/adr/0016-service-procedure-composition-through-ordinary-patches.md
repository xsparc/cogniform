# ADR 0016: Compose service procedures through ordinary patch admission

- Status: Accepted
- Date: 2026-08-04
- Task: CF016

## Context

CF007 established pure, seeded, bounded built-in procedures that emit ordinary
`ScenePatch` values. CF008 deliberately kept that library separate when it
introduced `LocalService`, so an embedder had to execute a procedure and then
submit its output as two unrelated API steps.

Adding a first-class procedure command to the gateway would create a second
canonical command format, fingerprint, response, and replay decision for an
operation whose only authoritative effect is already represented by a patch.
A privileged procedure-to-world path would bypass the atomic validation,
idempotency, delivery, extraction, and replay invariants shared by every other
mutation.

## Decision

`cogniform-engine` depends on the existing `cogniform-procedural` workspace
crate. `LocalService::submit_procedure` synchronously executes one typed
`ProcedureRequest` under the engine's active `RuntimeLimits`, then submits the
generated ordinary patch through `LocalGateway::submit_patch`. It returns the
deterministically generated stable entity IDs together with the ordinary
`GatewayAdmission` result. World mutation still occurs only when the caller
invokes `process_next`.

Procedure execution remains a pure, bounded preparation step with no world,
filesystem, network, clock, entropy, renderer, or mutation authority. Invalid
procedure limits fail before gateway admission. Entity, operation, component,
decoded-size, and supersession-text bounds are checked against declared and
runtime budgets before output allocation. Entity-ID collisions and all
authoritative world constraints remain ordinary atomic patch application
failures.

The gateway fingerprints the canonical generated patch, not the source
procedure request. Exact repeats therefore use output-oriented idempotency:
requests producing the same canonical patch are identical for admission, while
a changed output under the same idempotency key conflicts. Replay stores only
the accepted patch and receipt; it does not add procedure kind, seed, request,
or library implementation metadata.

After restoration, gateway response caches are intentionally empty. Repeating
a procedure may queue its generated patch again, but authoritative world
idempotency returns the retained receipt without a revision increment, replay
append, or duplicate extraction.

## Consequences

- Service callers can execute supported built-in procedures without composing
  two crates or gaining lower-level mutation access.
- Procedure output follows the same delivery, queue, conflict, atomicity,
  idempotency, extraction, replay, query, and recovery contracts as an explicit
  patch.
- Pure preparation happens synchronously before gateway capacity is known, but
  its work and output are bounded by the request and active runtime limits.
- Replay remains implementation-independent scene history; it cannot recover
  the originating procedure request or seed from metadata.
- No new procedure kind, gateway command/response variant, external code
  execution, plugin/Wasm host, ambient I/O, persistence, transport, or
  background scheduler is introduced.
