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
| GLB source tangent range/count/handedness is invalid or texture roles are inconsistent | Reject malformed source data without proxy before any generated fallback; only valid unsupported encodings may use the explicit proxy policy; never partially reserve GPU roles | CF055/CF065 tangent precedence, role, and atomic-reservation tests |
| Missing normal-map tangents exceed a fixed generation-work guard or MikkTSpace leaves an absent/unsuitable result | Reject before immutable adoption with `CollectionLimitExceeded` at `glb.decoded.generated_tangent_work` or `InvalidTangent` at `glb.decoded.generated_tangents`; never expose a partial mesh or proxy | CF065 exact budget, degenerate, adversarial, maximum-bound, and no-partial-admission tests |
| GLB metallic-roughness texture index/coordinate/resource shape is invalid or three-role texture pressure exceeds a bound | Reject before CPU/GPU adoption; decode shared source images once on CPU and reserve every missing content-hash-and-role GPU resource atomically | CF056 decode, accounting, and three-role reservation tests |
| GLB core emissive factor has the wrong length/type, is non-finite, or leaves the unit interval | Reject before asset readiness without proxy substitution; retain zero emission for omitted, material-free, proxy, and explicit-scene-material paths | CF057 decode/default/override tests |
| GLB emissive texture index/coordinate/resource shape is invalid or four-role texture pressure exceeds a bound | Reject before CPU/GPU adoption without proxy; decode shared source images once on CPU, reserve every missing content-hash-and-role GPU resource atomically, and preserve white/zero neutrality plus scene override | CF058 decode, accounting, four-role reservation, and controlled renderer tests |
| GLB alpha mode/cutoff is malformed, non-finite, negative, or paired incorrectly | Reject before readiness without proxy; only well-formed wider modes may use explicit proxy policy after malformed peers are excluded | CF059 decode, precedence, and controlled renderer tests |
| Imported MASK alpha is below its cutoff | Discard before color, depth, entity-ID, or normal output; exact equality survives, OPAQUE emits alpha one, and scene material disables imported coverage | CF059 controlled alpha-coverage tests |
| GLB `doubleSided` is null or non-boolean, including in an unused material | Reject before readiness without proxy; valid unsupported peer features cannot hide malformed material data | CF060 decode and precedence tests |
| Imported omitted/false back face is presented, or explicit true presents its back face | Cull the omitted/false face before every attachment; render true through the fixed unculled pipeline and reverse completed geometric/shaded normals before observation and lighting; built-ins, authored fallback, and scene override remain unculled and unflipped | CF060 CPU and controlled mixed-draw/normal/MASK tests |
| GLB extension declarations are empty, duplicated, non-string, inconsistent, an actual extension member is undeclared or non-object, or an unlit marker is not an exact empty object | Reject before readiness without proxy, including unused material records; never retain an arbitrary extension map | CF061 declaration/marker and malformed-peer tests |
| A selected exact `KHR_materials_unlit` material is rendered | Use only multiplied base color across no, directional, point, or combined lights; keep OPAQUE/MASK, face policy, observations, and lifecycle accounting; an explicit scene material restores ordinary lighting | CF061 CPU and controlled unlit/light/alpha/double-sided tests |
| A GLB sampler record/type/enum/count or texture sampler index is invalid, including unused input | Reject before readiness without proxy and before any valid unsupported peer; retain only strict bounded core metadata | CF062 exhaustive decode and malformed-peer tests |
| An imported sampler selects wrapping/filtering or a source mip mode while Cogniform retains one image level | Apply S/T repeat, mirrored-repeat, or clamp independently; apply authored magnification and the documented nearest/linear one-mip minification fallback through the fixed 36-entry table; texture accounting and recovery remain unchanged | CF062 controlled wrap/filter/four-role and full lifecycle tests |
| GLB primary vertex color has an invalid name, type, normalization, alignment, range, count, or non-finite component, including beside a valid wider attribute | Reject before readiness without proxy; validate every declared color set before unsupported classification and reserve the exact 64-byte expanded stream before allocation | CF063 exhaustive decode, precedence, and byte-bound tests |
| Imported primary vertex color is omitted, explicitly overridden, or participates in alpha coverage | Use exact white for omission/built-ins/proxies; multiply imported color with factor/texture before OPAQUE/MASK; explicit scene material disables imported color; preserve emission and non-color observations | CF063 CPU and controlled Vulkan composition tests |
| Embedded PNG is malformed, truncated, range-invalid, or over a dimension/pixel/working/decoded/residency limit | Reject before CPU/GPU adoption and never substitute a proxy; valid wider formats remain typed unsupported | CF029/CF055 asset image and proxy-policy tests |
| Renderer target/capability unavailable | Return a structured initialization error | renderer configuration/capability tests |
| Renderer owner dropped after frame submission | Keep final device/queue destruction on the per-renderer retirement worker so the pending readback reaches its configured deadline | `pending_readback_survives_renderer_drop_after_submission` |
| Draw, mesh-upload, or role-texture residency pressure | Reject before GPU preparation/allocation; atomically reserve only missing content-hash-and-role textures | renderer scene and CF029/CF055/CF056/CF058 asset tests |
| More than four directional-light definitions or an active degenerate transformed positive-Z axis | Reject before GPU submission; preserve renderer scene state and issue no partial frame | CF023 renderer preparation tests |
| More than four point-light definitions or an active point translation outside finite GPU-f32 range | Reject before GPU submission; preserve renderer scene state and issue no partial frame | CF024 renderer preparation tests |
| Built-in cuboid topology or orientation regresses | Initialization-only tests require six faces, two non-degenerate outward triangles per face, exact axis normals, zero primary coordinates, disabled fallback tangents, white colors, 36 vertices, and the fixed 2,304-byte payload | CF025/CF028/CF055/CF063 renderer unit and controlled adapter tests |
| Built-in sphere generation or selection regresses | Initialization-only tests require the exact finite 672-vertex, 43,008-byte payload, outward winding, unit radial normals, zero primary coordinates, disabled fallback tangents, white colors, direct/fallback selection, and resident-asset precedence; frames never tessellate or substitute another shape | CF022/CF028/CF055/CF063 renderer unit and scene tests |
| Observation requested from wrong/ahead source | Return typed causal error; do not relabel the frame | revision-causality tests |
| A local frame is over-limit, truncated, corrupted, noncanonical, or direction-invalid | Reject before returning a frame/message; allocate bodies only after header limits pass; do not resynchronize or expose payloads | CF039-CF040 framing/message fixtures and corruption tests |
| Local-session input violates hello, correlation, capacity, exact-revision, or quiescent-close rules | Emit one bounded stable failure when possible, preserve service invariants, and release each terminal correlation exactly once | CF041 executor model and controlled service tests |
| Version-two imagination changes session version, exceeds compilation limits, has inconsistent result/receipt roles, is superseded, or fails during processing | Reject before service work when possible; otherwise emit one correlated terminal outcome, never duplicate compilation/mutation on retained replay, and preserve typed FIFO order | CF044 schema fixtures, executor lifecycle tests, and controlled child-process test |
| Either inherited-stdio command receives a missing, unknown, non-Unicode, reordered, duplicate, or extra profile argument, or `serve-stdio` receives an interactive standard stream | Accept only omission or one exact `--profile` allowlist value; reject before stream/runtime/adapter/service selection with empty protocol stdout and one stable payload-redacted category | CF042/CF045 terminal preflight plus CF064 parser and black-box tests |
| `serve-stdio` reaches frame-boundary EOF before any frame | Exit successfully without constructing the local service or writing output | CF042 fake-stream and piped child-process tests |
| `serve-stdio` reaches EOF after a complete pre-hello frame or active hello, receives truncation/corruption, or exceeds a live-operation deadline | Terminate the child session nonzero with one stable redacted category; do not retry or read another frame | CF042 fake-stream scheduling and deadline tests |
| `serve-stdio` encounters service/executor failure or writes/flushes only part of an output | Flush a complete fatal service frame when available, then terminate; otherwise report output/executor failure, preserve any physical prefix, and never retry or resynchronize | CF042 fatal-service, write-zero, prefix-write, flush, and executor-fault tests |
| `serve-mcp-stdio` receives extra arguments, an interactive stream, a wrong initialization version, malformed/truncated JSON, or an input over its byte/nesting limits | Reject before lazy service creation when possible; return a bounded JSON-RPC error for an identified request or terminate nonzero with one stable payload-redacted category | CF045 transport equality/adversarial tests and CLI black-box tests |
| An MCP query, imagination, or direct patch is semantically invalid, stale, conflicting, busy, or inconsistent with compilation/receipt roles | Return one small structured tool error; validate arguments before lazy service creation, serialize service access, process at most one queued command, and use retained replay without a second compilation or mutation | CF045-CF046 portable official-client and role tests plus controlled adapter-backed query, apply, replay, conflict, stale-base, and exact-revision integrations |
| An MCP observation is rejected, fails delivery, exceeds its payload bound, times out, or returns inconsistent causality | Return one stable structured tool error and preserve the last fully completed resource; request-level rejection/failure remains usable, while timeout, poll failure, or invalid service output poisons further service-backed calls and requires child replacement | CF047 fake-backend replacement/failure tests, causal and deadline unit tests, and controlled canonical resource readback |
| An MCP peer pipelines ordinary messages while a prior response is stalled | Decode at most one bounded pending message without dispatching it; read no further line until the active response flushes, retaining one handler/response cycle under fixed reader/pipe backpressure | CF047-CF053 stalled-writer and pending-message transport tests |
| An MCP peer cancels the exact active request before response writing begins | Deliver the matching control to RMCP, suppress that request's response, poison an admitted observation wait, preserve the prior completed resource until teardown, dispatch no pending/later work, and terminate the child successfully after bounded cleanup; never infer an effect or reuse the child | CF053 exact-ID transport, cooperative-poll, response-suppression, retained-resource, and official-client tests |
| An MCP cancellation is missing, mismatched, queued behind another pending message, or arrives after response writing begins | Treat it as nonmatching/late and preserve the active response-through-flush contract; the parent still owns a process timeout and kill/reap fallback | CF053 missing/wrong-ID and response-write race tests |
| `serve-mcp-stdio` cannot encode, write, or flush a bounded output | Record the first stable transport failure, interrupt a pending read, terminate the session, preserve any physical prefix, and never retry or resynchronize | CF045 bounded writer, output equality/nesting, transport-status, and child-process tests |
| Public path or credential pattern staged | Fail with rule/path only; never echo the matched value | public-repository safeguard fixtures |
| Source-candidate tag, repository, archive, checksum, metadata, inventory, content, or bound is invalid | Fail with one path/payload-redacted `source-candidate` category; never overwrite an output or claim a partial candidate; remove files created by the failed preparation and report `cleanup_uncertain` if removal cannot be proved | CF050 disposable identity, attribute, limit, corruption, sidecar, and cleanup matrix |
| Workspace candidate members, versions, publish policy, local dependency declarations, or lock entries drift | Fail the package-policy check with one stable category and repository-relative path; correct the reviewed source together and rerun the complete candidate evidence before tagging | CF051 disposable drift matrix and live sixteen-package inventory |

Run the CPU failure matrix through the ordinary offline workspace suite:

```text
cargo test --workspace --all-features --locked --offline
python tests/security/test_public_repo_check.py
python scripts/check_public_repo.py --all
```

The every-byte replay case is deterministic fault injection, not fuzzing. A
future parser fuzz campaign must have a fixed time budget and separately
approved corpus-retention policy; it does not belong in every pull request.

## Source-candidate preparation failure

Before a tag exists, a package-policy failure means the source tree is not a
candidate. Do not edit only the reported manifest or lock entry in isolation.
Reconcile the shared version, explicit member inventory, non-publishable
package declarations, exact local requirements and paths, member inheritance,
lockfile, expected quality invocation, and candidate notes, then rerun the
ordinary and controlled checklist. The checker reports only a stable category
and repository-relative path; it never repairs files or grants tag/publication
authority.

`scripts/source_candidate.py prepare` is a local release-preparation boundary,
not a repair tool. Correct the stable reported category and start again with
two absent targets. Never overwrite, append to, or manually bless a failed
archive. `tag_moved`, `head_moved`, `repository_changed`, or
`git_version_changed` means the captured release basis is no longer stable;
return to the reviewed commit and repeat the complete preparation. Inventory,
blob, PAX, metadata, public-content, checksum, corruption, or trailing-data
failure means neither file is a candidate even if an external tar program can
list or extract it. A missing local object or oversized Git metadata response
fails closed; obtain a complete reviewed clone through a separately authorized
workflow rather than weakening the tool or allowing an implicit fetch.

On ordinary failure the tool removes only files it created during that
invocation. `cleanup_uncertain` means it could not prove that cleanup. Inspect
the caller-owned output directory without publishing either target, remove the
partial files through the operator's normal safe procedure, and retry with new
absent paths. The tool does not create or repair a tag, change a version,
upload an asset, authenticate a producer, or authorize a release.

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
JSON-RPC, scene, compilation, and receipt values. The opening exchange must be
either exact MCP `2025-11-25` initialization or an exact self-contained MCP
`2026-07-28` discovery/direct request. Every later modern request repeats its
protocol version and client capabilities; one connection cannot switch eras.
Identified invalid opening requests receive a small JSON-RPC error where
possible, while malformed, over-limit, deeply nested, truncated, invalid-
direction, or failed input terminates with a stable payload-redacted transport
category. Later missing, malformed, unsupported, mixed-era, or unsupported-
method requests return bounded JSON-RPC errors before semantic dispatch.
Invalid typed tool arguments are rejected before the lazy `LocalService` is
created.

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

An observation request is validated before lazy service creation, submitted
only through `LocalService`, and polled every 2 ms under a fixed 15 second
deadline. A successful canonical `COGOBS01` envelope atomically becomes the
only listed resource and replaces any prior URI. `observation_rejected`,
`observation_failed`, `observation_too_large`, and `output_unavailable`
preserve that prior resource without poisoning the service.
`observation_timeout`, polling `service_failed`, and
`invalid_service_output` also preserve and leave the prior resource readable,
but no further service-backed call is trusted; discard the child after reading
only if recovery of that already completed payload is required.

Each response is fully encoded and checked against the byte and nesting limits
before its first write. Only one request is admitted through complete response
flush, so pipelined resource reads cannot accumulate payload clones or encoded
responses. A later inherited-stream failure may still leave a
physical prefix; never retry or attempt JSON-line resynchronization. This
adapter has no general operation deadline or preemptive cancellation; only the
observation poll has its fixed deadline. The parent must enforce any stronger
timeout, kill/reap, authorization, confidentiality,
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
