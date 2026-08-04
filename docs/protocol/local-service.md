# Local typed service

Status: implemented by CF008 for offline in-process use; complete caller-owned
in-memory restoration was added by CF012 and a portable bounded recovery-point
envelope by CF013. CF014 adds exact-revision recovery points for fresh-service
historical forks. CF015 adds service-owned bounded asset resolution and
explicit recovery rehydration. CF016 composes pure built-in procedures through
ordinary patch admission.

`LocalService` composes one bounded `LocalGateway`, authoritative recorded
world, service-owned asset store, headless renderer, and observation worker. It
is a Rust API, not a network listener or daemon. No method performs
authentication, remote I/O, filesystem persistence, deployment, model access,
or a paid service call.

## Lifecycle and configuration

`LocalServiceConfig` contains an `EngineConfig`, `GatewayConfig`, and
`AssetStoreConfig`.
`LocalServiceConfig::new(width, height)` selects the existing bounded defaults
for an offscreen target. Initialization validates replay, world, renderer,
observation, command, and idempotency capacities before adapter/device
allocation; asset bounds are structurally non-zero typed values and remain
independent from renderer residency bounds.
Renderer initialization is asynchronous because adapter/device discovery is
asynchronous; steady-state service methods are caller-driven.

The service owns all mutable state. It does not expose a mutable world, ECS
handle, renderer device, queue, staging buffer, or replay log.

## Method contract

| Method | Behavior |
|---|---|
| `adapter` | Return the backend-neutral selected adapter summary for diagnostics |
| `enqueue_asset_source` | Verify exact source identity and admit bytes to the bounded import queue |
| `process_next_asset_import` | Decode at most one queued source into service-owned CPU meshes |
| `asset_record` | Return one immutable lifecycle record without source bytes |
| `enqueue_asset_upload` | Create an immutable ready-mesh job and reserve renderer capacity |
| `process_next_asset_upload` | Upload at most one renderer-owned queued mesh |
| `asset_status` | Return aggregate CPU-store and renderer-residency counters |
| `submit_patch` | Validate and admit one explicit patch under its delivery semantic |
| `submit_imagination` | Validate and admit one deterministic primitive imagination |
| `submit_procedure` | Execute one pure bounded built-in procedure and admit its generated ordinary patch |
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

## Built-in procedures

`submit_procedure` synchronously executes a typed `ProcedureRequest` under the
engine's active `RuntimeLimits`. It returns the procedure's deterministic
stable entity IDs and an ordinary `GatewayAdmission`. The resulting patch is
queued, processed, extracted, replayed, queried, and recovered exactly like a
caller-supplied patch; admission itself does not mutate the world.

Procedure and supersession-text budgets are checked before output allocation
or gateway admission. The gateway fingerprints the generated canonical patch,
so idempotency is output-oriented: an exact output repeats normally, while a
different output using the same key conflicts. The replay retains only the
accepted patch and receipt, not the procedure request, seed, or implementation
metadata. After restoration, an exact resubmission can re-enter the empty
gateway queue, but world idempotency returns the original receipt without
revision or replay growth.

Execution is a pure preparation step. It has no ambient I/O or authoritative
world access, and stable-ID collisions remain ordinary atomic patch failures
when `process_next` applies the command. Only the built-in cuboid-grid
procedure exists; user code, native plugins, Wasm, and external procedure
loading remain unsupported.

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

Command queues, cached gateway responses, outstanding observations, readback
work, asset records/imports, and renderer asset residency start empty.
Resubmitting an accepted patch may therefore enter
the fresh gateway queue, but world-level idempotency returns the original
receipt without mutation or another replay entry. Logical asset references do
restore because they are ordinary world components. Until their exact bytes
are admitted, imported, and uploaded again, a dependent observation returns
typed `AssetUnavailable`. Rehydration changes neither world revision nor
replay state.

## Asset ownership

Asset admission checks the supplied SHA-256 identity before consuming service
capacity. Import and upload are separate caller-driven, single-item steps;
patch processing, frame submission, initialization, and recovery do not perform
either step. `LocalService` owns decoded CPU records, the renderer owns upload
reservations and GPU meshes, and `CogniformEngine` passes only immutable upload
jobs between them. Public status and records contain no source bytes, ECS
handles, device handles, queues, or GPU buffers.

The service does not locate content by hash. Filesystem/network fetching,
durable caches, eviction, retries, and scheduling remain caller concerns.

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
- Only the pure built-in cuboid-grid procedure is supported. External
  procedures, plugins, Wasm, and user code are not loaded. The service accepts
  caller-supplied exact-hash asset bytes but does not fetch external assets,
  persist them, evict them, or restore their residency automatically.
- The headless baseline supports controlled DX12 or Vulkan adapters. Browser,
  Metal, OpenGL, and a hosted-GPU CI promise are outside the current contract.

See [ADR 0009](../adr/0009-recorded-engine-and-local-typed-service.md),
[ADR 0012](../adr/0012-complete-in-memory-service-restoration.md),
[ADR 0013](../adr/0013-versioned-recovery-point-envelope.md),
[ADR 0014](../adr/0014-exact-revision-historical-recovery-forks.md),
[ADR 0015](../adr/0015-service-owned-asset-resolution-and-rehydration.md),
[ADR 0016](../adr/0016-service-procedure-composition-through-ordinary-patches.md), the
[gateway guide](local-gateway-and-imagination.md), and the
[canonical scenario](../getting-started/canonical-scenario.md).
