# ADR 0009: Recorded engine and local typed service

- Status: Accepted
- Date: 2026-08-02
- Task: CF008

## Context

Cogniform's world, renderer, observation worker, deterministic compiler, local
gateway, and replay log already had bounded contracts, but an embedder still
had to compose them directly. The CLI was a skeleton, and there was no single
unattended path proving the MVP room, table, light, camera, atomic update,
query, observation, visibility, and replay requirements together.

The composition also allowed a lifecycle mistake: an engine could own an
authoritative world while a caller separately owned the accepted-event log.
That arrangement makes it possible to forget to record a newly accepted patch
even though the replay wrapper already provides a fail-before-mutation bound.

Three service shapes were considered:

1. add a socket protocol and background event loop;
2. put command, world, renderer, observation, and replay state into a new
   service implementation; or
3. retain the existing ownership domains and expose a thin, local typed service
   over the gateway-owned engine.

The first option introduces authentication, transport, shutdown, and
deployment concerns before the local semantics are proven. The second
duplicates state and weakens domain ownership. The third proves the public
behavior with the smallest new boundary.

## Decision

`CogniformEngine` owns a `RecordedWorld` rather than a bare
`AuthoritativeWorld`. Every newly accepted engine patch is encoded and checked
against replay entry, entry-size, and total-log bounds before world mutation.
An accepted idempotent replay returns the retained receipt without another log
entry or render extraction. The engine exposes read-only replay evidence,
complete owned replay bytes, chain verification, the live logical hash, and a
fresh-world replayed logical hash. Mutable world access remains private.

`LocalService` is the local composition boundary. It owns one `LocalGateway`,
which owns one engine. Typed methods admit patches or imaginations, process at
most one mutating command, execute exact-revision queries, request and poll
observations, inspect aggregate capacity/revision status, and verify or copy
replay evidence. It creates no socket, listener, filesystem persistence,
shared-memory segment, model call, or remote side effect. Command processing
and observation polling remain caller-driven and bounded.

The canonical scenario is reusable engine code invoked by both the CLI and a
controlled integration test. It requires a newly initialized service and:

1. atomically creates a room, table, light, and camera;
2. atomically moves and restyles the table;
3. verifies the committed revision with an exact logical query;
4. requests color and entity-ID images at that revision;
5. requests visibility and verifies that the table has visible pixels; and
6. verifies the accepted-event chain, replays it into a fresh world, and
   requires the exact same canonical logical hash.

Each observation is admitted and consumed before the next request. Polling has
a caller-configured timeout bounded from one nanosecond through sixty seconds,
metadata must identify the expected request, camera, kind, and current
revision, and staleness must be zero. The report's observation evidence retains
stable causal fields rather than wall-clock timestamps.

## Consequences

- A successful engine mutation and its accepted-event evidence share one
  ownership and capacity boundary.
- The service is directly embeddable and testable offline, but it is not yet a
  remotely accessible daemon or stable wire protocol.
- The CLI now proves the MVP through public engine interfaces without gaining
  access to ECS or GPU handles.
- Replay bytes are an owned in-memory value. Persistence, loading into a live
  service, snapshots, reverts, and log rotation require later lifecycle work.
- Observation results remain owned vectors behind bounded readback slots.
  Streaming and shared-memory leases remain deferred.
- The canonical scene uses built-in primitives. The baseline renderer records
  the light as logical/extracted state but still renders flat material color;
  illumination is not claimed.
- Asset import/upload and built-in procedures keep their separately bounded,
  caller-driven library boundaries. The local service does not yet resolve
  asset bytes or execute procedures on behalf of clients.
- Default GitHub Actions remains the existing single standard offline quality
  job. The controlled GPU scenario is explicit because hosted GPU availability
  is not part of the project's cost-conscious CI contract.
