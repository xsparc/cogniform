# Local typed service

Status: implemented by CF008 for offline in-process use.

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

## Known limitations

- The boundary is local Rust only; there is no socket, wire compatibility
  promise, session, authentication, authorization, or multi-tenant isolation.
- Processing and polling are caller-driven. There is no long-running daemon,
  subscription stream, shutdown protocol, or automatic retry loop.
- Observation payloads are owned vectors. Shared-memory leases and encoded
  delivery are deferred.
- Replay is bounded in memory and can be copied out, but snapshotting,
  persistence, recovery into a running service, revert, and log rotation are
  not implemented.
- Asset stores, renderer asset upload, and pure procedures remain separate
  library APIs. The service does not fetch or resolve external assets.
- The headless baseline supports controlled DX12 or Vulkan adapters. Browser,
  Metal, OpenGL, and a hosted-GPU CI promise are outside the current contract.

See [ADR 0009](../adr/0009-recorded-engine-and-local-typed-service.md), the
[gateway guide](local-gateway-and-imagination.md), and the
[canonical scenario](../getting-started/canonical-scenario.md).
