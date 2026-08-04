# ADR 0017: Revert a quiescent local service through fresh replacement

- Status: Accepted
- Date: 2026-08-05
- Task: CF017

## Context

CF012 established complete fresh-service restoration, and CF014 added exact
historical replay prefixes paired with the source renderer's current frame
frontier. Callers could create a separate fork, but the SDD also names live
recorded-state revert as a local lifecycle operation.

Rewriting the current authoritative world and renderer in place would require a
second atomic replacement protocol across ECS indexes, derived transforms,
render scene state, observations, assets, replay, and idempotency. Silently
discarding queued commands or active observations would also make a requested
historical state ambiguous.

## Decision

`LocalService` privately retains its validated configuration and exposes an
asynchronous `revert_to_revision` method for a strictly older retained
`SceneRevision`. The method captures the exact replay prefix with the source's
current next frame identity and constructs a complete fresh replacement through
the existing restoration path. It assigns that replacement only after replay,
world, renderer, observation, gateway, and configuration initialization all
succeed. Any preparation failure leaves the original service unchanged.

The operation requires quiescence: command depth, outstanding observations,
pending asset imports, and pending asset uploads must all be zero. A typed error
returns those exact bounded blocker counts. A target equal to the current
revision is typed separately, and a future target retains the existing
requested/latest replay error.

Successful replacement truncates live replay and authoritative state to the
exact target prefix, resumes renderer frames from the source frontier, and
records no revert event. Gateway response caches, queue counters, observation
state, service-owned asset records, and renderer-owned asset residency start
empty as they do after ordinary restoration. `LocalRevertReceipt` makes the
removed replay-entry count and cleared cache/CPU/GPU asset counts explicit.
Logical asset references in the retained prefix remain and require exact-hash
rehydration.

World idempotency retained by the prefix still returns the original receipt
without replay growth. Idempotency keys that existed only in the removed tail
are no longer retained and may form the new branch through ordinary patch
admission.

## Consequences

- The local API can atomically move a live quiescent service to an exact older
  retained state without a second world/renderer mutation implementation.
- Replacement may temporarily require resources for both old and new GPU
  domains. Initialization failure is controlled and preserves the old service.
- Callers must drain bounded work first and explicitly rehydrate any retained
  logical asset references after success.
- Revert is a caller-authorized local lifecycle action, not a replay event or
  proof of freshness, authenticity, rollback policy, or branch identity.
- Persistence, automatic startup, scheduled/remote rollback, snapshot
  registries, transient-work migration, asset preservation, device-loss
  recovery, transport, deployment, and release publication remain separate
  work.
