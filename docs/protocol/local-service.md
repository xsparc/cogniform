# Local typed service

Status: implemented by CF008 for offline in-process use; complete caller-owned
in-memory restoration was added by CF012 and a portable bounded recovery-point
envelope by CF013. CF014 adds exact-revision recovery points for fresh-service
historical forks.

`LocalService` composes one bounded `LocalGateway`, authoritative recorded
world, headless renderer, and observation worker. It is a Rust API, not a
network listener or daemon. No method performs authentication, remote I/O,
filesystem persistence, deployment, model access, or a paid service call.

## Lifecycle and configuration

`LocalServiceConfig` contains an `EngineConfig` and `GatewayConfig`.
`LocalServiceConfig::new(width, height)` selects the existing bounded defaults
for an offscreen target. Initialization validates replay, world, renderer,
observation, command, and idempotency capacities before adapter/device
allocation.
Renderer initialization is asynchronous because adapter/device discovery is
asynchronous; steady-state service methods are caller-driven.

The service owns all mutable state. It does not expose a mutable world, ECS
handle, renderer device, queue, staging buffer, or replay log.

## Method contract

| Method | Behavior |
|---|---|
| `adapter` | Return the backend-neutral selected adapter summary for diagnostics |
| `submit_patch` | Validate and admit one explicit patch under its delivery semantic |
| `submit_imagination` | Validate and admit one deterministic primitive imagination |
| `process_next` | Compile/apply at most one queued mutating command |
| `query` | Return one bounded, exact-revision logical result immediately |
| `request_observation` | Reserve one observation slot and submit a revision-linked frame |
| `try_receive_observation` | Poll at most one completed owned observation without waiting |
| `status` | Return revision, renderer, queue, observation, and replay occupancy counters |
| `verify_replay` | Verify sequence, revision, scene-hash, predecessor, and entry-hash chains |
| `logical_hash` | Hash the current canonical logical world |
| `replayed_logical_hash` | Replay accepted events into a fresh world and hash the result |
| `replay_bytes` | Copy the complete bounded version-one replay stream |
| `recovery_point` | Capture complete replay bytes and the next unreserved renderer frame identity |
| `recovery_point_at_revision` | Capture a complete exact-revision replay prefix with the source's current next frame identity |
| `restore` | Create a fresh service from one complete validated recovery point |

Patch and imagination admission preserves the gateway's `MustApply`,
`LatestWins`, `BestEffort`, idempotent replay, and capacity behavior. Admission
does not secretly process work. `process_next` handles no more than one queued
item, so embedders choose their own scheduling policy without an unbounded
background loop.

Observation requests are separate from the mutating command queue. A request
reserves capacity across GPU submission, readback, worker processing, and
delivery. The slot is released only when the result or error is received and
dropped. `try_receive_observation` never waits; callers that need a deadline
must apply a bounded polling policy such as the canonical scenario's per-result
timeout.

Image requests return owned color, normalized depth, quantized flat world-space
normal, or exact stable-identity vectors. Normal background pixels are `None`,
matching the identity payload's explicit absence rather than using a zero
vector sentinel. Visibility requests return stable-ID-sorted pixel counts and
remain the only observation kind without dimensions.

## Replay ownership

The engine applies patches through `RecordedWorld`. Canonical patch encoding
and replay capacity checks happen before a new world mutation. Newly accepted
patches append one entry. An exact idempotent replay appends nothing and emits
no duplicate extraction.

`verify_replay` checks the complete retained chain. `replayed_logical_hash`
also applies every recorded patch to a fresh world using the original world
bounds. A caller can compare it with `logical_hash`; the canonical scenario
treats any difference as failure. Returned replay bytes are a copy. The
service does not persist, rotate, transmit, or load them automatically.

## In-memory restoration

`recovery_point` captures the complete replay stream together with the source
renderer's next unreserved frame identity. A caller may retain that owned value,
drop the source service, and pass it to `LocalService::restore` with reviewed
bounds. `EngineRecoveryPoint::to_envelope_bytes` and `from_envelope_bytes` let a
caller preserve both parts as one deterministic, versioned byte sequence under
the same replay bound. Decoding checks header, version, exact length, non-zero
frame, and SHA-256 integrity before copying replay bytes. Callers still own
storage, confidentiality, authenticity, freshness, and atomic replacement.

Restoration validates and replays the complete stream before adapter selection
or GPU initialization. Any invalid tail rejects the point; the service never
adopts only the longest verified prefix. It reconstructs authoritative state
and idempotency records, retains the original log for later append, applies one
final-state extraction to a fresh renderer, and resumes at the captured frame
identity. A marker behind any replay-recorded estimated-visible frame is
rejected.

Command queues, cached gateway responses, outstanding observations, and
readback work start empty. Resubmitting an accepted patch may therefore enter
the fresh gateway queue, but world-level idempotency returns the original
receipt without mutation or another replay entry. Asset stores and renderer
asset residency are separate and must be re-established by their owner.

## Historical recovery forks

`recovery_point_at_revision` captures the complete replay prefix through one
retained revision. Revision zero is supported as the empty-world stream. A
request newer than the current retained revision returns a typed error naming
the requested and latest revisions. The source service is observationally
unchanged: its world, full replay bytes, renderer, and transient queues remain
in place.

The point carries the source renderer's current next unreserved frame identity,
not a frame inferred from the historical entry. Passing it to `restore`
therefore creates a fresh logical fork at the historical revision without
reusing a frame the source may already have reserved or exposed. The fork can
query, observe, and append normally; subsequent entries form a new continuation
from that exact prefix.

Frame counters are independent after capture. Keeping the source and fork live
can therefore produce the same future numeric frame identity in both branches.
Callers comparing concurrent branches must add branch identity or coordinate
frame allocation outside Cogniform.

The API does not switch the live source, overwrite later history, retain a
snapshot catalog, compare branch ancestry, or authenticate freshness. Callers
must explicitly own and coordinate every fork.

## Known limitations

- The boundary is local Rust only; there is no socket, wire compatibility
  promise, session, authentication, authorization, or multi-tenant isolation.
- Processing and polling are caller-driven. There is no long-running daemon,
  subscription stream, shutdown protocol, or automatic retry loop.
- Observation payloads are owned vectors. Shared-memory leases and encoded
  delivery are deferred.
- Recovery is into a fresh service from a complete in-memory point. Exact
  retained revisions can seed separate historical forks, but filesystem
  persistence, automatic startup, snapshot registries, in-place recovery or
  revert, branch management, cross-branch frame allocation, and log rotation
  are not implemented.
- Asset stores, renderer asset upload, and pure procedures remain separate
  library APIs. The service does not fetch or resolve external assets.
- The headless baseline supports controlled DX12 or Vulkan adapters. Browser,
  Metal, OpenGL, and a hosted-GPU CI promise are outside the current contract.

See [ADR 0009](../adr/0009-recorded-engine-and-local-typed-service.md),
[ADR 0012](../adr/0012-complete-in-memory-service-restoration.md),
[ADR 0013](../adr/0013-versioned-recovery-point-envelope.md),
[ADR 0014](../adr/0014-exact-revision-historical-recovery-forks.md), the
[gateway guide](local-gateway-and-imagination.md), and the
[canonical scenario](../getting-started/canonical-scenario.md).
