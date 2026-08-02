# Failure injection and recovery

Cogniform fails locally and explicitly. It does not retry durable work,
truncate responses, skip replay events, or expand a queue behind the caller's
back. The current process has no production supervisor or persistent service
store; operators compose those concerns outside the MVP.

## Verified failure matrix

| Fault | Expected containment | Evidence |
|---|---|---|
| Oversized, deeply nested, unknown, or duplicate protocol fields | Reject before a valid message reaches world preflight | `cogniform-protocol` contract and fixture tests |
| Empty, stale, conflicting, or invalid patch operation | Preserve revision, ID index, snapshot, and logical hash | `world_contracts` atomic rejection and randomized model tests |
| Hierarchy cycle, excess depth, or dangling relation | Reject the complete patch; preserve prior derived transforms | `hierarchy_hash` tests |
| Idempotency key reused for different command content | Return typed conflict without duplicate compile or mutation | gateway and world idempotency tests |
| Full command/result/observation queue | Typed reject, explicit drop, or in-place supersession according to declared delivery | gateway, observation-slot, and readback-pressure tests |
| Replay entry or total-log capacity exhausted | Reject before world mutation | `replay_capacity_rejects_before_world_mutation` |
| Replay byte truncated, reordered, removed, or modified | Return only the longest verified prefix and a typed tail error | replay contract tests |
| Any one replay byte flipped | Reject before the affected entry; verify and replay only the intact prefix | `every_single_byte_corruption_stops_before_the_unverified_entry` |
| Asset content hash mismatch | Consume no record or queue capacity | asset-store tests |
| Truncated, malformed, over-limit, or unsafe-proxy GLB | Reject without panic or GPU upload; only approved unsupported classes may proxy | asset truncation and classification tests |
| Renderer target/capability unavailable | Return a structured initialization error | renderer configuration/capability tests |
| Renderer owner dropped after frame submission | Keep final device/queue destruction on the per-renderer retirement worker so the pending readback reaches its configured deadline | `pending_readback_survives_renderer_drop_after_submission` |
| Draw or asset-upload residency pressure | Reject before GPU preparation/allocation | renderer scene and asset tests |
| Observation requested from wrong/ahead source | Return typed causal error; do not relabel the frame | revision-causality tests |
| Public path or credential pattern staged | Fail with rule/path only; never echo the matched value | public-repository safeguard fixtures |

Run the CPU failure matrix through the ordinary offline workspace suite:

```text
cargo test --workspace --all-features --locked --offline
python tests/security/test_public_repo_check.py
python scripts/check_public_repo.py --all
```

The every-byte replay case is deterministic fault injection, not fuzzing. A
future parser fuzz campaign must have a fixed time budget and separately
approved corpus-retention policy; it does not belong in every pull request.

## Runtime response

### Capacity or backpressure

Inspect `LocalServiceStatus` and the typed admission error. Consume pending
results, reduce the producer rate, or create a new service with reviewed larger
bounds. Do not spin-retry `MustApply` or observation work while the same queue
remains full. `BestEffort` drops and `LatestWins` supersession are explicit
outcomes, not successful durable application.

### Patch or query rejection

Keep the authoritative revision returned by the last accepted receipt. Correct
the complete request and resubmit with a fresh transaction/idempotency key and
the current exact base revision. Do not split an invalid atomic operation list
unless that changes the intended transaction semantics.

### Replay tail failure

Preserve the source bytes privately. `ReplayLog::load_prefix` returns the
verified prefix, failure offset, verified-entry count, and stable failure kind.
Only that prefix may be inspected or replayed. Never remove, reorder, or skip
the bad event to continue the same chain. Recovery into a new `LocalService` is
not implemented; embedders may replay through the lower-level recorded-world
API or restart from their own reviewed composition.

### Renderer, readback, or device failure

Stop admitting work to the affected instance and retain its accepted replay
bytes only if the caller has an approved persistence location. Drop the service
to release the device and worker channels, then start a new process or instance
and re-establish state through a verified application-specific recovery path.
The MVP does not promise in-process device recreation, automatic command retry,
or observation continuity across restart.

### Secret or private-data finding

Treat credentials as compromised: revoke or rotate first, avoid copying the
value into logs or reports, and coordinate public-history remediation privately.
For non-credential private scene/replay data, stop any publication or artifact
upload and determine whether the destination is already public. A later delete
does not by itself remove public history or downloaded artifacts.

## Known injection gaps

Actual GPU device removal, operating-system thread-creation failure, allocator
failure, process termination during a world commit, and disk-full persistence
are not deterministically injected. There is no built-in persistence write, so
disk crash consistency is outside the current process. Final GPU destruction is
kept off the bounded caller path, but a stalled driver may strand that
per-renderer retirement worker until process exit. These are Medium residual
operational risks for local evaluation and become release blockers if automatic
persistence, production supervision, or remote service claims are introduced.

See the [MVP threat model](../threat-model/mvp.md) and
[local service contract](../protocol/local-service.md).
