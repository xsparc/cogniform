# ADR 0012: Complete in-memory service restoration

- Status: Accepted
- Date: 2026-08-04
- Task: CF012

## Context

The local service records every newly accepted patch in a bounded canonical
replay stream and can verify or replay that stream into a fresh logical world.
It could not, however, adopt verified state as a new running service. A bare
replay stream is also insufficient for complete renderer causality: observation
frames that do not accompany a mutation are not replay entries, so inferring
the next frame only from recorded patch estimates can reuse a frame identity.

Three approaches were considered:

1. add filesystem-backed checkpoints and automatic startup recovery;
2. restore from replay bytes and infer the next frame from recorded patch
   estimates; or
3. define a caller-owned in-memory recovery point containing the complete
   replay stream and the source renderer's next unreserved frame identity.

Filesystem persistence would introduce storage policy, atomic writes, crash
consistency, permissions, retention, and deployment concerns. Replay-only
restoration cannot preserve every produced frame identity. The typed recovery
point preserves the state needed by the current in-process domains without
claiming a durable service design.

## Decision

`EngineRecoveryPoint` owns complete version-one replay bytes and the next
`FrameId` available to the source renderer. `LocalService::recovery_point`
captures both values together; `from_parts` lets a caller reconstruct the typed
value after applying its own storage and atomicity policy. Debug output reports
only the replay byte count and frame marker, never replay contents.

`LocalService::restore` creates a fresh service. Before adapter selection or GPU
initialization, it validates configuration, parses the complete bounded replay
stream, rejects any malformed, truncated, noncanonical, integrity-invalid, or
over-limit tail, and replays every entry into a new authoritative world. The
verified-prefix inspection API remains available at the replay layer, but a
service never adopts a prefix as successful recovery.

The restored `RecordedWorld` retains the same verified log so later accepted
patches append to the original chain. One complete final-state extraction
synchronizes the fresh renderer. The renderer starts at the captured next frame
identity; restoration rejects a marker lower than any recorded
`estimated_visible_frame`. Equality is valid because an estimate may identify
a frame that had not yet been produced when the point was captured.

Command queues, cached gateway responses, active or completed observations,
and readback work start empty. Authoritative world idempotency records are
replayed, so resubmitting an accepted patch returns its retained receipt without
another mutation or log entry.

## Consequences

- A caller can restore logical state, replay continuity, renderer revision, and
  frame progression into a fresh bounded local service.
- Invalid recovery input fails closed before GPU initialization, and an intact
  prefix is never silently treated as the requested complete state.
- Callers that persist a recovery point must preserve its bytes and frame
  marker together. `from_parts` cannot prove that a caller supplied the actual
  source renderer marker beyond the consistency visible in replay entries.
- Transient queued commands, gateway result caching, observations, and GPU
  readbacks are not recovered. Accepted mutations retain world-level
  idempotency; other in-flight work must be reconciled by the caller.
- Asset source bytes, decoded asset stores, and renderer residency are not part
  of the point. Callers must re-establish asset availability separately.
- Filesystem persistence, automatic startup, snapshots, revert, log rotation,
  device recreation, transport, authentication, tenancy, deployment, and
  release publication remain separate decisions.
