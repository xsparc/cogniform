# ADR 0014: Exact-revision historical recovery forks

- Status: Accepted
- Date: 2026-08-04
- Task: CF014

## Context

CF012 restored a fresh local service from the complete accepted-event stream,
and CF013 bound that stream to renderer frame continuity in one portable
envelope. The target architecture also calls for returning to recorded state,
but the only public capture path selected the latest revision.

An in-place revert would require live command and observation coordination,
renderer replacement, failure atomicity, branch ownership, and a policy for
later history. Durable snapshots would additionally decide storage, atomic
replacement, retention, startup, permissions, and freshness. Those concerns
are wider than the existing caller-owned recovery boundary.

## Decision

`ReplayLog::to_bytes_through_revision` encodes the complete contiguous replay
prefix ending at an exact retained `SceneRevision`. Revision zero produces the
valid header-only stream. Because each accepted patch advances revision exactly
once, the revision identifies the prefix entry count. A request newer than the
retained log returns a typed `ReplayRevisionError` carrying the requested and
latest revisions.

`CogniformEngine::recovery_point_at_revision` and
`LocalService::recovery_point_at_revision` package that prefix in an
`EngineRecoveryPoint`. The point carries the source renderer's current next
unreserved `FrameId`, not a reconstructed historical frame. This prevents a
restored fork from reusing an identity that the source may have reserved or
exposed after the chosen logical revision.

Frame counters are independent after capture. If source and fork both remain
live, they may issue equal future numeric frame identities. Cogniform does not
assign a branch identity or coordinate frame allocation across services.

Capture is read-only with respect to the source. Restoration continues through
the existing fresh-service path, so it verifies the complete prefix and frame
relationship before GPU initialization, performs one final-state extraction,
and starts transient command, result, observation, and readback work empty.

## Consequences

- Callers can create deterministic fresh-service branches from any retained
  exact revision, including the initial empty world.
- A restored fork reproduces the selected logical hash and replay bytes, then
  can query, observe, and append a new valid continuation.
- Source state and later source history remain unchanged by capture.
- Logical revision can move backward in the new service while frame identity
  resumes from the source's current frontier; revision and frame are separate
  causal dimensions, and future frame uniqueness across concurrent branches is
  caller-owned.
- The API does not implement in-place revert, live source replacement,
  persistent snapshots, retention, ancestry or merge semantics, automatic
  rollback, freshness/authentication, asset restoration, transient-work
  migration, or cross-branch frame allocation. Each requires separately
  approved design work.
