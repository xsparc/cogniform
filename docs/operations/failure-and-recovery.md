# Failure injection and recovery

Cogniform fails locally and explicitly. It does not retry durable work,
truncate responses, skip replay events, or expand a queue behind the caller's
back. An explicit adapter can create and load immutable local recovery files
and separate exact-hash asset-source files, but the current process has no
production supervisor, mutable snapshot or asset catalog, retention policy,
or automatic startup/rehydration; operators compose those concerns.

## Verified failure matrix

| Fault | Expected containment | Evidence |
|---|---|---|
| Oversized, deeply nested, unknown, or duplicate protocol fields | Reject before a valid message reaches world preflight | `cogniform-protocol` contract and fixture tests |
| Compilation result is over-limit, noncanonical, reordered, duplicated, role-invalid, revision-mismatched, truncated, or substituted | The codec returns no invalid decoded value and the compiler returns no invalid completed result; CF044 installs negotiated local-session result bounds before compilation/application, and later adapters must invoke validation under their own limits | CF043 compilation-result fixtures and adversarial codec tests plus CF044 compiler/session/executor limit and identity tests |
| Empty, stale, conflicting, or invalid patch operation | Preserve revision, ID index, snapshot, and logical hash | `world_contracts` atomic rejection and randomized model tests |
| Hierarchy cycle, excess depth, or dangling relation | Reject the complete patch; preserve prior derived transforms | `hierarchy_hash` tests |
| Idempotency key reused for different command content | Return typed conflict without duplicate compile or mutation | gateway and world idempotency tests |
| Procedure entity, patch, or supersession-text budget is exceeded | Reject before output allocation or gateway admission; preserve revision, queue, hash, and replay | procedure and controlled service-procedure tests |
| Full command/result/observation queue | Typed reject, explicit drop, or in-place supersession according to declared delivery | gateway, observation-slot, and readback-pressure tests |
| Pending command, observation, import, or upload stops making progress | Inspect optional monotonic oldest-pending age; drain or reschedule the named caller-driven lifecycle without exposing payloads | CF031 deterministic age lifecycle and controlled status tests |
| Replay entry or total-log capacity exhausted | Reject before world mutation | `replay_capacity_rejects_before_world_mutation` |
| Replay byte truncated, reordered, removed, or modified | Return only the longest verified prefix and a typed tail error | replay contract tests |
| Any one replay byte flipped | Reject before the affected entry; verify and replay only the intact prefix | `every_single_byte_corruption_stops_before_the_unverified_entry` |
| Recovery envelope is malformed, over-limit, truncated, extended, or changed at any byte | Return a typed non-sensitive codec error before copying replay bytes or initializing a service | recovery-envelope unit tests |
| Recovery stream has any invalid tail or a frame marker behind replay evidence | Reject the complete recovery point before GPU initialization; never adopt only the verified prefix | recovery unit and controlled service-restoration tests |
| Recovery-file target already exists or its parent is absent | Return a path-redacted create-new error; never overwrite the target or create a directory | storage create-new unit tests |
| Recovery-file write or sync fails | Return the failing operation/error kind and whether the partial file was removed or may remain; never return a success receipt | injected storage write/sync tests |
| Recovery-file target is non-regular, symlinked, over-limit, growing, truncated, extended, or corrupt | Reject before unbounded allocation or recovery-point return; never accept a verified prefix as a complete file | storage load and envelope-corruption tests |
| Offline recovery inspection sees invalid replay semantics or a stale frame frontier | Reject the complete file after bounded load and before adapter selection; print no path or payload and adopt no world state; JSON mode leaves stdout empty | CF032 engine and CF033 CLI inspection tests |
| Asset-file source is oversized or does not match its expected hash | Reject before any filesystem operation or target creation | asset-file preflight tests |
| Asset-file target exists, write/sync fails, or load sees a non-file, symlink, over-limit/growing source, or hash substitution | Never overwrite; report cleanup for a failed create; return no bytes before complete bounded identity validation | asset-file unit and controlled persisted-rehydration tests |
| Offline asset inspection sees invalid hash syntax, a non-file, over-limit/growing source, or hash substitution | Reject before human or JSON success output; keep stdout empty and diagnostics path/payload-redacted; perform no decode, service, network, upload, or GPU work | CF036-CF037 CLI inspection and existing storage tests |
| Historical revision is newer than the retained log | Return the requested and latest revisions without changing the source service | exact-revision replay and controlled historical-fork tests |
| Live revert target is current/future or transient work is not drained | Return typed target or exact command/observation/import/upload blockers; preserve world, renderer, frame frontier, replay, hash, queues, and assets | controlled quiescent-revert test |
| Asset content hash mismatch | Consume no record or queue capacity | asset-store tests |
| Restored logical asset reference has no CPU/GPU residency | Return the exact entity and mesh key as `AssetUnavailable`; preserve revision, hash, and replay | controlled service-asset rehydration test |
| Explicit asset eviction targets queued, partially uploaded, resident, or absent content | Release exact CPU/GPU reservations and residency for that hash, preserve unrelated FIFO and submitted frames, and leave world, revision, hash, replay, frame frontier, recovery, and source files unchanged | CF030 asset-store, renderer, and controlled service tests |
| Truncated, malformed, over-limit, or unsafe-proxy GLB | Reject without panic or GPU upload; only approved unsupported classes may proxy | asset truncation and classification tests |
| GLB normal range/count is invalid, a direction is zero or non-finite, or a position-only triangle is degenerate | Reject before decoded/GPU adoption; never substitute a proxy for malformed direction data | CF020 asset normal and proxy-policy tests |
| GLB primary-coordinate range/count is invalid or any source component is non-finite | Reject before expanded allocation or GPU adoption, including malformed unused indexed values; never substitute a proxy for malformed coordinate data | CF028 asset coordinate and proxy-policy tests |
| Embedded PNG is malformed, truncated, range-invalid, or over a dimension/pixel/working/decoded/residency limit | Reject before CPU/GPU adoption and never substitute a proxy; valid wider formats remain typed unsupported | CF029 asset image and proxy-policy tests |
| Renderer target/capability unavailable | Return a structured initialization error | renderer configuration/capability tests |
| Renderer owner dropped after frame submission | Keep final device/queue destruction on the per-renderer retirement worker so the pending readback reaches its configured deadline | `pending_readback_survives_renderer_drop_after_submission` |
| Draw, mesh-upload, or unique texture residency pressure | Reject before GPU preparation/allocation; duplicate mesh jobs for one content hash reserve one texture | renderer scene and CF029 asset tests |
| More than four directional-light definitions or an active degenerate transformed positive-Z axis | Reject before GPU submission; preserve renderer scene state and issue no partial frame | CF023 renderer preparation tests |
| More than four point-light definitions or an active point translation outside finite GPU-f32 range | Reject before GPU submission; preserve renderer scene state and issue no partial frame | CF024 renderer preparation tests |
| Built-in cuboid topology or orientation regresses | Initialization-only tests require six faces, two non-degenerate outward triangles per face, exact axis normals, zero primary coordinates, 36 vertices, and the fixed 1,152-byte payload | CF025/CF028 renderer unit and controlled adapter tests |
| Built-in sphere generation or selection regresses | Initialization-only topology tests require the exact finite 672-vertex, 21,504-byte payload, outward winding, unit radial normals, zero primary coordinates, direct/fallback selection, and resident-asset precedence; frames never tessellate or substitute another shape | CF022/CF028 renderer unit and scene tests |
| Observation requested from wrong/ahead source | Return typed causal error; do not relabel the frame | revision-causality tests |
| A local frame is over-limit, truncated, corrupted, noncanonical, or direction-invalid | Reject before returning a frame/message; allocate bodies only after header limits pass; do not resynchronize or expose payloads | CF039-CF040 framing/message fixtures and corruption tests |
| Local-session input violates hello, correlation, capacity, exact-revision, or quiescent-close rules | Emit one bounded stable failure when possible, preserve service invariants, and release each terminal correlation exactly once | CF041 executor model and controlled service tests |
| Version-two imagination changes session version, exceeds compilation limits, has inconsistent result/receipt roles, is superseded, or fails during processing | Reject before service work when possible; otherwise emit one correlated terminal outcome, never duplicate compilation/mutation on retained replay, and preserve typed FIFO order | CF044 schema fixtures, executor lifecycle tests, and controlled child-process test |
| `serve-stdio` is given extra arguments or an interactive standard stream | Reject before adapter/service selection with empty binary stdout and one stable stderr category | CF042 CLI argument and terminal-preflight tests |
| `serve-stdio` reaches frame-boundary EOF before any frame | Exit successfully without constructing the local service or writing output | CF042 fake-stream and piped child-process tests |
| `serve-stdio` reaches EOF after a complete pre-hello frame or active hello, receives truncation/corruption, or exceeds a live-operation deadline | Terminate the child session nonzero with one stable redacted category; do not retry or read another frame | CF042 fake-stream scheduling and deadline tests |
| `serve-stdio` encounters service/executor failure or writes/flushes only part of an output | Flush a complete fatal service frame when available, then terminate; otherwise report output/executor failure, preserve any physical prefix, and never retry or resynchronize | CF042 fatal-service, write-zero, prefix-write, flush, and executor-fault tests |
| `serve-mcp-stdio` receives extra arguments, an interactive stream, a wrong initialization version, malformed/truncated JSON, or an input over its byte/nesting limits | Reject before lazy service creation when possible; return a bounded JSON-RPC error for an identified request or terminate nonzero with one stable payload-redacted category | CF045 transport equality/adversarial tests and CLI black-box tests |
| An MCP query, imagination, or direct patch is semantically invalid, stale, conflicting, busy, or inconsistent with compilation/receipt roles | Return one small structured tool error; validate arguments before lazy service creation, serialize service access, process at most one queued command, and use retained replay without a second compilation or mutation | CF045-CF046 portable official-client and role tests plus controlled adapter-backed query, apply, replay, conflict, stale-base, and exact-revision integrations |
| `serve-mcp-stdio` cannot encode, write, or flush a bounded output | Record the first stable transport failure, interrupt a pending read, terminate the session, preserve any physical prefix, and never retry or resynchronize | CF045 bounded writer, output equality/nesting, transport-status, and child-process tests |
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

`LocalServiceStatus::command_queue` and
`oldest_outstanding_observation_age_micros`, plus the import/upload fields in
`asset_status`, distinguish empty work from the oldest retained wait in
saturating monotonic microseconds. Compare successive caller-selected samples;
the value is not a wall-clock timestamp, deadline, SLO, payload identifier, or
automatic alert. A reset can mean processing, eviction, delivery, or valid
`LatestWins` replacement, so interpret it with depth and outcome counters.

### Patch or query rejection

Keep the authoritative revision returned by the last accepted receipt. Correct
the complete request and resubmit with a fresh transaction/idempotency key and
the current exact base revision. Do not split an invalid atomic operation list
unless that changes the intended transaction semantics.

For imagination rejection, preserve the last authoritative revision and inspect
the stable admission/completion role. Correct a stale base revision or declared
budget and use a fresh identity/key for changed work. A replayed admission
already contains the retained exact completion and must not be resubmitted as a
request to compile or apply again. An unresolved completion is successful
deterministic compiler output with no patch or receipt; change the semantic
request or scene preconditions rather than treating it as a partial mutation.

### Fixed-profile stdio-session failure

The launching parent owns both redirected streams, the child lifetime, and any
sensitive frame bytes. Only EOF at a frame boundary before the first frame is
a clean no-op. After any complete input frame, an EOF without orderly `close`
is failure. Truncation, corruption, fatal service/executor state, operation
deadline, write, or flush failure also ends the session; discard that child and
stream rather than retrying a whole frame or attempting resynchronization.

Treat stdout as binary protocol data only and stderr as its stable aggregate
diagnostic channel. Do not merge them, print logs on stdout, or assume an
errored output wrote zero bytes: a physical frame prefix may already have
reached the parent. The 15-second deadline governs only repeated completion
polling and cannot interrupt a blocking read, write, flush, adapter/service
initialization, or synchronous executor/renderer call. The parent must enforce
any stronger process-level timeout and kill/reap policy.

This fixed inherited-stdio profile authenticates no peer, encrypts no data,
creates no pipe or listener, and performs no automatic restart. Do not expose
it as a remote or multi-tenant endpoint. See the
[stdio-session guide](../protocol/local-stdio-session.md) for the exact flow.

### MCP stdio failure

The launching parent owns redirected stdin/stdout, the child lifetime, and all
JSON-RPC, scene, compilation, and receipt values. Initialization must negotiate
exactly MCP `2025-11-25`; identified pre-initialization requests receive a small
JSON-RPC error where possible, while malformed, over-limit, deeply nested,
truncated, or failed input terminates the session with a stable payload-redacted
transport category. Invalid typed tool arguments are rejected before the lazy
`LocalService` is created.

Tool calls are serialized against one service. A query binds to one exact
revision and cannot mutate. An imagination admits at most one command, validates
the returned compilation and receipt against the submitted identities, and
returns retained replay without another `process_next` call. A direct patch is
validated before lazy creation, admitted only through
`LocalService`, and binds the returned receipt to the submitted transaction,
key, base, operation count, and apply/replay role. Correct `invalid_patch` or
`patch_rejected` with a fresh identity/key and the current revision when the
intended content changes. Treat
`invalid_service_output`, `service_failed`, or `output_unavailable` as loss of
trust in that child: discard it and inspect a fresh service or approved
recovery point rather than retrying the request against the same process.

Each response is fully encoded and checked against the byte and nesting limits
before its first write. A later inherited-stream failure may still leave a
physical prefix; never retry or attempt JSON-line resynchronization. This
adapter has no operation deadline or preemptive cancellation. The parent must
enforce any stronger timeout, kill/reap, authorization, confidentiality,
freshness, rate, tenancy, or restart policy. See the
[MCP stdio adapter guide](../protocol/mcp-stdio-adapter.md) for the exact flow.

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

### Recovery-file create or load failure

Choose and authorize the parent directory outside Cogniform. `create_new`
validates the complete envelope before opening the target and never overwrites
an existing path or creates a missing parent. If write or sync fails, inspect
`PartialFileCleanup`: `Removed` means the exact new path is absent, while
`Retained` means an operator must inspect or remove the partial file before
reusing that name. Do not treat file existence as successful persistence.

On load, treat paths and file contents as untrusted local input. A non-regular
or final-component symlink, over-limit metadata, post-inspection growth, read
failure, length mismatch, or digest failure returns no recovery point. Keep the
path out of general logs; errors intentionally report only the operation and
standard error kind. A successfully loaded envelope is still plaintext,
unauthenticated, and not proof of freshness, and `LocalService::restore` must
perform its complete replay/world/frame validation before use.

For a read-only diagnosis under the declared default local profile, run
`cogniform-cli inspect-recovery <path>`. Success reports only bounded counts,
revision/frame values, and final logical/replay hashes. Failure remains nonzero
and path-redacted whether storage/envelope validation or complete semantic
preflight rejects the file. The command neither modifies the file nor restores
a service, initializes a GPU, discovers another file, or repairs a bad suffix.
A passing result is not writer authentication, freshness, GPU compatibility,
or asset-residency evidence.

For automation, use `cogniform-cli inspect-recovery --json <path>` and require
`schema_version` 1; do not parse the default human report. The JSON object is
written only after the complete preflight passes, so any storage, envelope, or
semantic failure leaves stdout empty. The report still contains potentially
sensitive hashes and aggregate counts even though it contains no path or
payload. Use `--json -- <path>` for the reserved filename `--json`.

File `sync_all` is not a portable guarantee that the directory entry or storage
hardware survives power loss. Automatic latest-pointer replacement, directory
sync, retention/rotation, startup, rollback selection, and remote storage need
separate reviewed protocols.

### Asset-file create or load failure

Authorize the path and its expected logical `ContentHash` outside Cogniform.
`AssetFileStore::create_new` rejects source size or identity before opening the
target and otherwise follows the same create-new, sync, and
`PartialFileCleanup` rules as a recovery file. Do not substitute a different
hash merely to accept unexpected bytes.

On load, a non-regular or final-component symlink, over-limit metadata,
post-inspection growth, read failure, or complete hash mismatch returns no
source bytes. Keep paths out of general logs. A valid hash establishes byte
identity only: the source remains plaintext, the writer is unauthenticated,
and the ordinary asset importer must still validate GLB structure and bounds.

Recovery and asset files are independent. Restore the complete recovery point,
then load only caller-approved sources for its retained logical references and
explicitly drive import/upload. Cogniform supplies no directory scan, manifest,
hash-to-path lookup, retry, retention, automatic eviction, or automatic
rehydration. Explicit in-memory eviction never deletes the independently
persisted source.

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

### Explicit asset eviction

Treat `LocalService::evict_asset` as a trusted local capacity-policy action.
Inspect its exact store and renderer outcomes to account for removed queued
bytes, decoded meshes/textures, upload reservations, and logical GPU residency.
Repeating an absent eviction is safe, but do not spin an adversarial
evict/reimport cycle or mistake it for an automatic cache policy.

World references, revision, logical hash, replay, frame allocation, recovery
points, and separately persisted source files remain unchanged. Subsequent
draws use an authored primitive fallback or return `AssetUnavailable`; restore
residency only from approved exact-hash bytes through explicit import/upload.
Already submitted readbacks remain valid, and the backend may defer physical
GPU destruction until submitted work is retired.

### Renderer, readback, or device failure

Stop admitting work to the affected instance. If a complete
`EngineRecoveryPoint` was captured while the source renderer was available,
retain it or explicitly create one immutable recovery file in an approved
location, drop the service to release the device and worker channels, load if
needed, and restore a fresh instance. Re-establish asset residency separately.
The MVP does not promise in-place device recreation, automatic command retry,
queued-result recovery, startup selection, or observation continuity across
restart.

### Secret or private-data finding

Treat credentials as compromised: revoke or rotate first, avoid copying the
value into logs or reports, and coordinate public-history remediation privately.
For non-credential private scene/replay data, stop any publication or artifact
upload and determine whether the destination is already public. A later delete
does not by itself remove public history or downloaded artifacts.

## Known injection gaps

Actual GPU device removal, operating-system thread-creation failure, allocator
failure, process termination during a world commit or file write, actual disk
full, directory-entry loss, and power loss are not deterministically injected.
CF018 and CF019 inject ordinary write and sync failures and verify partial
cleanup, but do not establish disk crash consistency. Final GPU destruction is
kept off the bounded caller path, but a stalled driver may strand that per-renderer
retirement worker until process exit. These are Medium residual operational
risks for local evaluation and become release blockers if automatic
persistence, production supervision, or remote service claims are introduced.

See the [MVP threat model](../threat-model/mvp.md), the
[recovery-file guide](../persistence/recovery-files.md), the
[asset-file guide](../persistence/asset-files.md), and the
[local service contract](../protocol/local-service.md), plus the
[stdio-session guide](../protocol/local-stdio-session.md).
