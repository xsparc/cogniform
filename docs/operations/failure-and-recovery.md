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
| Procedure entity, patch, or supersession-text budget is exceeded | Reject before output allocation or gateway admission; preserve revision, queue, hash, and replay | procedure and controlled service-procedure tests |
| Full command/result/observation queue | Typed reject, explicit drop, or in-place supersession according to declared delivery | gateway, observation-slot, and readback-pressure tests |
| Replay entry or total-log capacity exhausted | Reject before world mutation | `replay_capacity_rejects_before_world_mutation` |
| Replay byte truncated, reordered, removed, or modified | Return only the longest verified prefix and a typed tail error | replay contract tests |
| Any one replay byte flipped | Reject before the affected entry; verify and replay only the intact prefix | `every_single_byte_corruption_stops_before_the_unverified_entry` |
| Recovery envelope is malformed, over-limit, truncated, extended, or changed at any byte | Return a typed non-sensitive codec error before copying replay bytes or initializing a service | recovery-envelope unit tests |
| Recovery stream has any invalid tail or a frame marker behind replay evidence | Reject the complete recovery point before GPU initialization; never adopt only the verified prefix | recovery unit and controlled service-restoration tests |
| Historical revision is newer than the retained log | Return the requested and latest revisions without changing the source service | exact-revision replay and controlled historical-fork tests |
| Live revert target is current/future or transient work is not drained | Return typed target or exact command/observation/import/upload blockers; preserve world, renderer, frame frontier, replay, hash, queues, and assets | controlled quiescent-revert test |
| Asset content hash mismatch | Consume no record or queue capacity | asset-store tests |
| Restored logical asset reference has no CPU/GPU residency | Return the exact entity and mesh key as `AssetUnavailable`; preserve revision, hash, and replay | controlled service-asset rehydration test |
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

### Procedure rejection

Treat a procedure error as rejection of its complete generated change. Correct
the request's dimensions or declared entity, operation, component, decoded, and
text budgets before resubmission. A rejected procedure consumes no gateway
slot and changes no world or replay state. Reuse an idempotency key only when
the corrected request produces the same intended canonical output; a different
output under that key is an explicit conflict. Do not bypass the service by
granting a procedure mutable world or ambient I/O access.

### Replay tail failure

Preserve the source bytes privately. `ReplayLog::load_prefix` returns the
verified prefix, failure offset, verified-entry count, and stable failure kind
for diagnosis. Only that prefix may be inspected through the lower-level API.
Never remove, reorder, or skip the bad event to continue the same chain.
`LocalService::restore` accepts only a complete valid recovery point and rejects
the entire point before GPU initialization when any tail is invalid.

When recovery state was retained as an envelope, decode it with the intended
`ReplayConfig` before calling `restore`. Do not treat a valid envelope digest as
proof of who produced the bytes: SHA-256 detects accidental change but does not
authenticate a writer. A header, version, bound, exact-length, frame, or digest
failure rejects the complete envelope; do not trim a suffix or extract its
replay payload manually.

### Historical fork request

Request only a revision retained by the source replay log. A newer revision
returns a typed `ReplayRevisionError`; do not reinterpret that error as the
latest available state. A successful historical point is a complete standalone
replay prefix paired with the source renderer's current next frame identity.
Restore it into a separate fresh service. The operation neither changes the
source nor automatically switches a live service or authorizes reuse of older
frame identities.

### In-place historical revert

Stop new producers and drain the mutating-command queue, observations, pending
asset imports, and pending asset uploads before calling
`revert_to_revision`. A `NotQuiescent` error reports all four counts; do not
discard work merely to suppress the error unless that loss is an explicit
caller decision. Equal and future targets are also typed and non-mutating.

The operation creates and validates a complete fresh replacement before swap.
If replay, world reconstruction, adapter/device initialization, or gateway
setup fails, retain and continue using the unchanged original service. On
success, use `LocalRevertReceipt` to account for the removed replay tail and
cleared result/CPU/GPU asset state. Rehydrate retained logical asset references
from approved exact-hash bytes before dependent observations. The revert is not
recorded as a scene event and does not establish authorization, freshness,
branch identity, persistence, or an automatic rollback policy.

### Asset unavailable after recovery

Treat the returned entity ID and content-hash/mesh-index key as a request for
caller-owned rehydration, not permission to fetch arbitrary content. Obtain the
expected source bytes through the caller's approved channel, admit them with
the retained hash, and explicitly process CPU import and GPU upload. A hash
mismatch must not be retried with a substituted identity because that would no
longer resolve the world reference. Rehydration leaves the world and replay
unchanged; fetching, persistence, cache policy, and retry scheduling are not
implemented by the service.

### Renderer, readback, or device failure

Stop admitting work to the affected instance. If a complete
`EngineRecoveryPoint` was captured while the source renderer was available,
retain it or its single encoded envelope only in an approved location, drop the
service to release the device and worker channels, decode if needed, and
restore a fresh instance. Re-establish asset residency separately. The MVP does
not promise in-place device recreation, automatic command retry, queued-result
recovery, or observation continuity across restart.

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
