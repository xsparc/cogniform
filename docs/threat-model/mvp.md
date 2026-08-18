# MVP threat model

Status: reviewed for the local source-first candidate profile on 2026-08-02
and extended through CF053 bounded terminal MCP cancellation on 2026-08-13.

This model covers the in-process, single-user Cogniform MVP. It does not claim
that the engine is an authentication, authorization, multi-tenant, remote, or
production security boundary. Exposing it to untrusted remote clients without a
separate transport and identity design violates the assumptions below.

## Assets and security objectives

| Asset | Objective |
|---|---|
| Authoritative world | Only complete validated patches change state; stable IDs and revisions remain correct |
| Accepted-event log, recovery point, and recovery file | Newly accepted patches stay complete, ordered, bounded, integrity checked, and replayable; complete or exact-revision replay bytes remain associated with non-reused frame-continuity state; explicit files never overwrite an existing target; diagnostics reveal no path or payload |
| Observation causality | Request schema and exact expected revision reject before capacity/renderer work; payload, camera, frame, revision, and stable identity agree; optional binary payloads and local frames remain bounded and integrity-bound to canonical metadata |
| Compilation outcomes | Source imagination/revision, normalized patch, decisions, and unresolved constraints remain bounded, canonical, ordered, unique, and role-consistent before adapter use |
| Asset state and source files | Source identity is exact; malformed or oversized input cannot become decoded or GPU-resident state; recovered references and caller-mapped source files cannot substitute different bytes |
| Host resources | CPU, memory, queues, GPU allocations, and waits stay within declared bounds and explicit release is exactly accounted |
| Process and GPU | Backend failures return controlled errors and do not grant access to world mutation |
| Local session streams | Inherited binary input/output remains bounded, direction-correct, causally correlated, explicitly flushed, and fail-closed without payload-bearing diagnostics or whole-frame retry |
| MCP stdio stream and retained resource | Newline JSON-RPC remains byte- and nesting-bounded before decode, output is bounded before each write, one active plus one decoded pending message bound the pipeline, exact matching cancellation terminates without a response or later dispatch, tool effects retain exact revision and idempotency roles, the sole observation resource remains causal and canonical, and diagnostics disclose no request or result payload |
| Repository and release | No credentials, private workflow state, paid calls, or unreviewed artifacts enter public history |

Availability is bounded rather than guaranteed. Confidentiality applies to
project and operator data, but Cogniform does not encrypt runtime scene or
replay values. A caller that places sensitive text in a scene also places it in
snapshots and canonical replay entries.

## Trust boundaries

1. **Caller to protocol and pure preparation.** Patches, imagination,
   procedures, queries, IDs, labels, limits, and observation requests are
   untrusted typed or JSON-derived data.
2. **Compilation values to result schema.** A nested optional patch, decision
   and unresolved collections, scene text, and canonical JSON cross explicit
   version, byte/nesting, logical/text/count, role, order, uniqueness, outcome,
   and revision checks without acquiring compiler execution or I/O authority.
3. **Asset file/bytes to storage and decoder.** A caller-selected path, claimed
   SHA-256 identity, and GLB/embedded PNG bytes cross bounded file checks before
   separately crossing into strict, explicitly scheduled parsers.
4. **World to renderer.** The renderer receives immutable compact extraction,
   never mutable ECS state or an authorization decision.
5. **Renderer to observation worker.** GPU readback crosses an asynchronous
   fixed-capacity lease and returns owned validated payloads.
6. **Owned observation and metadata to payload codec.** Caller-owned values
   cross an explicit in-memory canonical-layout, bounds, and integrity boundary
   without acquiring I/O or transport authority.
7. **Caller-owned byte stream to local frame codec.** A fixed header, declared
   control and bulk lengths, and body bytes cross independent header-first
   bounds and integrity checks without creating an endpoint or session.
8. **Control bytes to local-session schema.** Direction-specific client/server
   JSON crosses byte/nesting, version, unknown-field, canonical-byte, nested
   protocol/compilation-value, role, and effective-limit checks without
   execution or I/O.
9. **Session values to one local-service executor.** Validated instructions
   cross explicit hello/lifecycle, peer/local/service limit, correlation,
   typed patch/imagination command-order, exact replay, observation-delivery,
   output, and quiescent-close checks
   without acquiring endpoint or automatic scheduling authority.
10. **Inherited standard streams to the CLI session driver.** A parent-owned
   redirected binary stream crosses exact argument/terminal preflight, bounded
   frame/message decoding, half-duplex scheduling, per-frame output/flush, a
   fixed live-operation deadline, and fail-closed shutdown. This is one local
   endpoint policy, not peer authentication or remote transport security.
11. **Inherited standard streams to the MCP adapter.** Parent-owned redirected
   newline JSON-RPC crosses exact protocol-version initialization, incremental
   byte and nesting preflight, one-active/one-pending admission with a matching
   cancellation-only control bypass, fixed tool dispatch, serialized lazy-
   service access, bounded encode-before-write output, exact-URI latest-value
   resource retention, and fail-closed shutdown. Tool annotations are
   interoperability hints, not authorization policy.
12. **Recovery file/envelope and replay bytes to recovery.** Caller-selected
   paths and portable bytes are untrusted until regular-file/size/growth checks,
   envelope header/version/bounds/length/frame/digest, and replay
   protocol/revision/scene-hash/predecessor/entry-hash chains have been
   independently verified.
13. **Repository to public hosting.** Tracked content, commit metadata, workflow
   definitions, dependencies, and future release artifacts become public.

Exact-hash asset files use the same final-file type, size, growth, create-new,
cleanup, and path-redaction boundary, followed by complete identity validation.
The compiler, compilation-result value crate, and built-in procedures have no
ambient filesystem, network, time, world, renderer, or entropy authority. The
result crate performs no compilation or session execution. The current local
service creates no
socket, listener, shared-memory segment, automatic persistent file, or model
call. The local frame adapter touches only caller-supplied `std::io` values and
does not select a path, address, endpoint, peer, or session. The separate
local-session codec opens no I/O and executes no decoded request. The separate
executor owns one supplied service and advances only when its caller invokes a
method; it opens no endpoint and starts no thread, timer, or runtime loop. The
CLI's fixed `serve-stdio` composition locks one inherited redirected stream
pair, drives the executor half-duplex, flushes each frame, and enforces a
completion-poll deadline. It creates no pipe, listener, socket, child process,
daemon, identity boundary, or remote-security policy. The separate
`serve-mcp-stdio` composition owns one inherited redirected stream pair and a
current-thread async runtime, pins each connection to exact `2025-11-25` or
exact `2026-07-28`, exposes only four fixed tools plus one latest-value
observation resource, and creates its one `LocalService` lazily after typed
argument validation. Modern requests repeat independently validated protocol
and client-capability metadata; discovery grants no inherited identity or
extension authority. It creates no
listener, socket, credential store, model call,
background service, identity boundary, or remote-security policy. The
storage adapter touches only an explicit caller-selected path when directly
invoked.
The offline inspection command composes that adapter with the exact CPU
restoration preflight, retains no reconstructed world, and emits only aggregate
counts, revision/frame values, and hashes. Its optional machine-readable view
is versioned and encoded only at the CLI boundary after complete validation.
The controlled CPU measurement's optional machine-readable view is likewise
versioned and encoded only at the CLI boundary after all samples finish. It
contains fixed fixture/profile/sample metadata and timings, but no hardware
identity, system metadata, threshold, upload, or background collection.
The canonical scenario's optional machine-readable view is also CLI-only and
is encoded after the complete adapter-backed scenario succeeds. It deliberately
contains the already-human-visible adapter summary plus exact run evidence, so
it is opt-in local output rather than automatic telemetry.
The asset-source inspection command accepts one explicit expected hash and
path, reuses the bounded storage load, drops the verified bytes, and emits only
that hash and the byte count. It never invokes a decoder, service, importer,
renderer, upload, rehydration, or automatic path discovery.
Its optional machine-readable view is CLI-only, schema-versioned, and serialized
in memory after complete verification; the default human report is unchanged.

## Threat and control matrix

Residual ratings assume the declared local single-user boundary.

| Threat | Inherent risk | Controls and evidence | Residual |
|---|---|---|---|
| Oversized or deeply nested messages exhaust CPU or memory | High | Pre-decode byte/nesting caps, bounded collections and budgets, fail-before-mutation tests | Low |
| A malformed, substituted, reordered, or semantically inconsistent compilation result reaches an adapter | High | Exact schema version and canonical LF bytes; unknown/duplicate/code rejection; independent encoded/logical/text/count/patch bounds; code-specific field roles; strict order/uniqueness; patch/outcome/revision invariants; bounded re-encoding; no invalid codec result; no invalid completed compiler result; CF044 installs local-session bounds before compilation/application; mandatory validation under each later adapter's own limits | Low inside the CF044 local single-client mapping; canonical bytes provide no authentication, authorization, confidentiality, or remote-transport approval |
| A local-session imagination changes version, exceeds negotiated result bounds, misbinds identity/receipt roles, or replays through duplicate compilation/mutation | High | Version-locked hello; field-wise compilation-limit negotiation; nested canonical result validation; imagination/key/transaction/revision/operation/status role checks; one typed command FIFO; exact-once correlation release; gateway-retained replay returned at admission without `process_next` | Low inside the single-client local inherited-stream boundary; no peer authentication, confidentiality, or remote replay policy |
| Malformed, substituted, or adversarial GLB/PNG allocates excessively or reaches GPU state | High | Service-owned exact-hash admission, source/decoded/count limits, strict geometry/image subset, exact 48-byte expanded-vertex reservation, finite non-zero same-count normal and tangent with exact handedness, full-source finite same-count primary-coordinate validation, unit-bounded numeric material plus finite normal-scale validation, zero-to-three typed texture roles with unique-image CPU accounting, PNG dimension/pixel/working/decoded bounds, atomic content-hash-and-role GPU texture count/byte reservation, exact-pinned vendored decoder, explicit one-item processing, empty recovery residency, truncation corpus, unsafe proxy exclusions | Medium |
| Stale, conflicting, or partially invalid patch mutates part of the world | High | Exact base revision, complete preflight plan, atomic commit, invariant/property tests | Low |
| Idempotency-key reuse duplicates or substitutes work | High | Retained canonical command fingerprint, transaction identity, conflict error, exact replayed receipt | Low |
| Adversarial procedure dimensions or text allocate unbounded output or bypass mutation controls | High | Pure built-in implementation, entity/patch/decoded/text preflight under active runtime limits, ordinary gateway admission and atomic patch processing, controlled restoration test | Low |
| Queue or readback pressure creates hidden unbounded work | High | Fixed command, idempotency, asset, renderer, and observation capacities with typed rejection/drop/supersession; optional aggregate oldest-pending ages expose stalled caller-driven work; per-renderer retirement guard keeps final driver destruction off the caller's bounded read path | Low |
| Queue diagnostics disclose queued content or create a hidden telemetry channel | High | Status/debug expose only optional saturating elapsed microseconds and existing aggregate counts; monotonic instants stay process-local and out of payloads, identifiers, system time, durable state, logs, exporters, and background workers | Low inside the local caller boundary |
| A caller churns eviction and reimport to amplify decode, upload, or GPU-retirement work | High | Explicit trusted-local content-hash API only; one-item bounded import/upload; exact release outcomes; no background eviction, retry, or automatic rehydration; submitted work remains safely retired | Low inside the local caller boundary; remote exposure would require rate and authorization controls |
| Adversarial light or material definitions create unbounded per-frame GPU work, invalid directions, non-finite positions, or singular direct-light math | High | Independent four-definition directional/point caps, stable preparation, zero-padded fixed uniform, finite/unit-bounded scene and imported values, finite selected-camera conversion, roughness and BRDF denominator floors, degenerate active-direction and out-of-range active-position rejection, exact-zero and derived-distance-overflow handling, and pre-submit tests | Low |
| Renderer-local IDs escape as authoritative identity | High | Frame-local compact mapping, stable IDs in public observations, exact center-pixel tests | Low |
| Observation from an old camera or revision is accepted as current | High | Exact requested revision validated before capacity and renderer submission; camera/frame/revision completion checks, explicit staleness, source-ahead rejection, and canonical scenario proof | Low |
| Observation payload bytes are truncated, extended, corrupted, noncanonical, over-limit, or paired with different causal metadata | High | Independent complete-envelope and visibility bounds; exact kind/count/length and fixed big-endian layouts; canonical finite floats, presence, identity, and ordering; SHA-256 binding over header, canonical metadata, and payload; decode allocation only after framing, metadata, and integrity checks; all-prefix and every-byte mutation tests | Low for in-memory corruption and substitution; writer authenticity and confidentiality remain caller-owned |
| A local frame declares excessive sections, is truncated, corrupted, noncanonical, or crosses an untrusted stream boundary | High | Fixed version/kind/header; non-zero correlation identity; independent complete/control/bulk limits checked before body allocation; exact short/interrupted I/O; canonical observation metadata and nested envelope integrity; all-prefix, every-byte, substitution, pre-body-limit, and payload-redaction tests | Low inside caller-owned bounded local I/O; endpoint identity, authentication, confidentiality, freshness, replay protection, rate limits, cancellation, and partial-write recovery remain caller-owned |
| Local control bytes substitute a direction, version, nested value, correlation, or over-limit shape | High | Separate client/server roots; outer-only correlation; pre-decode byte/nesting caps; unknown-field and canonical-byte rejection; nested core validation; self-consistent hello limits; stable redacted failures; exact fixtures and malformed/substitution tests | Low for in-memory local decoding; lifecycle state, execution authorization, endpoint identity, confidentiality, freshness, replay/rate policy, and partial-write handling remain outside the codec |
| Valid local-session work exhausts correlations, misroutes terminal results, bypasses lifecycle, or emits an over-limit completion | High | One hello and terminal close; peer/local/service limit intersection; fixed live-correlation cap; ordered key/ID maps; exact queued/replayed/dropped/superseded/completed/error release; one command plus one observation poll and at most two outputs per call; completed-frame preflight; stable redacted failure mapping; model tests | Low inside the trusted caller-driven boundary; authorization, endpoint identity, confidentiality, freshness/replay/rate policy, deadlines, cancellation, and partial-write handling remain outside the executor |
| A stdio peer sends malformed or incomplete frames, abandons a live session, stalls completion, or causes partial output | High | Exact args and terminal checks before adapter selection; clean EOF only before the first frame; negotiated bounds; half-duplex no-read-while-live driver; positive-cadence 15-second completion deadline; encode-before-write and per-frame flush; stable redacted fatal categories; no whole-frame retry or resynchronization; fake-stream and controlled child-process tests | Low inside a parent-owned local single-user process boundary; physical output prefixes, blocking synchronous calls, peer authorization, confidentiality, freshness/replay/rate policy, process supervision, and remote exposure remain caller-owned |
| An MCP peer sends malformed, oversized, deeply nested, version-substituted, mixed-era, stale, conflicting, or replayed tool traffic, pipelines bulk resource reads, requests an unknown resource, substitutes observation causality, cancels or abandons a call, or causes partial output | High | Exact legacy initialization or modern discovery/direct opening; one pinned connection era; independent exact protocol and capability validation on every modern request; no advertised extensions or inherited discovery authority; Tasks, MRTR input-required results, subscriptions, Apps, prompts, sampling, model calls, and all other unsupported methods excluded; complete modern result discrimination, informational server identity, and zero-TTL private discovery/list/read cache roles; incremental newline bounds before decode; outer JSON nesting preflight; direction rejection; exactly four fixed tools; one active request plus at most one decoded pending message; exact-ID cancellation delivered only before response writing, response suppression and terminal no-later-dispatch semantics; wrong/missing/late cancellation remains post-flush; cooperative observation polling with prior-resource preservation and service poison; typed core validation before lazy service creation; serialized access; direct patches and observations admitted only through `LocalService`; exact revision, transaction, idempotency, operation, compilation, receipt, observation metadata, dimension, and staleness roles; pre-mutation patch rejection; retained replay without a second process call; one exact-URI 4 MiB canonical envelope resource with bounded base64 output, atomic replacement, and failure preservation; bounded encode-before-write and flush; stable payload-redacted categories; official-client, raw-wire, cancellation, stalled-writer, and controlled child-process tests in both eras | Low inside a parent-owned local single-user process boundary; the trusted parent can intentionally submit any core-valid patch, server identity is informational rather than authentication, cancellation provides no rollback/effect receipt/general deadline, and physical output prefixes, peer authorization, confidentiality, freshness/rate policy, kill/reap supervision, and remote exposure remain caller-owned |
| Replay bytes are truncated, reordered, or modified | High | Append-only SHA-256 chain, verified-prefix inspection, complete-service fail-closed restoration, exact replay checks, every-byte corruption injection | Low |
| Recovery replay bytes and frame marker are separated or accidentally changed | High | Single bounded versioned envelope, exact-length parsing, domain-separated SHA-256 digest, every-byte corruption rejection before replay allocation | Low for accidental corruption; authenticity remains caller-owned |
| A recovery path or file causes overwrite, disclosure, unbounded allocation, or partial-state adoption | High | Separate opt-in crate; encode-before-I/O; create-new only; final symlink/non-file rejection; metadata/platform allocation bound; fixed-buffer read and growth probe; complete digest validation; path-redacted errors; injected write/sync cleanup | Medium because parent-path trust, permissions, confidentiality, authenticity, freshness, and crash durability remain caller-owned |
| Offline recovery diagnostics expose a path/payload, accept only a verified prefix, mutate the file, require GPU availability, or emit partial machine output on failure | High | Exact one-path CLI; bounded read-only storage load; shared complete restoration preflight before stdout; aggregate-only human/schema-v1 JSON result; path/payload-redacted success and errors; empty JSON failure stdout; ordinary no-adapter black-box tests | Low inside the trusted local fixed-profile boundary |
| Measurement diagnostics create a hidden performance gate or disclose a host fingerprint | High | Explicit local invocation; fixed fixture and sample counts; schema-v1 integer timings marked `informational_only`; no hardware identity, system metadata, threshold, automatic upload, exporter, or background sampling; complete preparation before stdout | Low inside the trusted local boundary; operators still control timing disclosure |
| Scenario diagnostics expose adapter identity or correlatable run evidence, emit a partial proof, or imply portable support | High | Explicit `--json`; complete scenario and in-memory schema-v1 serialization before stdout; fixed profile/scenario; exact cross-mode tests; no path, payload, timing, upload, exporter, background sampling, or added support claim; invalid arguments reject before adapter selection | Low inside the trusted local boundary; operators still control adapter and run-evidence disclosure |
| An asset path or source file causes overwrite, disclosure, unbounded allocation, substitution, or unsafe implicit rehydration | High | Separate opt-in adapter; source size/hash checks before I/O; create-new only; bounded regular-file load and growth probe; complete expected-hash validation before return; path-redacted errors; explicit later import/upload; injected cleanup and controlled restart evidence | Medium because path mapping, parent trust, permissions, confidentiality, writer authenticity, freshness, retention, and crash durability remain caller-owned |
| Offline asset-source diagnostics disclose a path or payload, trust substituted bytes, emit partial machine output, mutate the file, or trigger decode, service, or GPU side effects | High | Exact one-hash/one-path CLI; strict expected-hash parsing before I/O; shared bounded read-only storage load and complete identity check; aggregate human/schema-v1 JSON output serialized before one stdout write; dropped bytes; path/payload-redacted errors; empty failure stdout; ordinary CPU black-box tests | Low inside the trusted local boundary; operators still control path access and hash disclosure |
| A historical fork reuses a frame identity issued before capture or mutates the live source | High | Exact contiguous replay prefixes are copied with the source's current next frame identity; controlled tests preserve source status/hash/bytes and prove query/observe/append continuation | Low for pre-capture reuse; future cross-branch identity and freshness remain caller-owned |
| A stale, unintended, or busy live revert silently loses authoritative or transient state | High | Local caller-only API, explicit older revision, exact quiescence blockers, fresh replacement before swap, no event on failure, explicit removed-tail/cache/asset receipt, controlled continuation test | Low inside the local trusted-caller boundary; authorization and freshness remain caller-owned |
| Scene text or replay data discloses caller secrets | High | No automatic logging, upload, persistence, or release; explicit storage errors/debug omit path and content; operator warning and public-repo scan | Medium |
| Native code or a procedure escapes its authority | High | Unsafe Rust forbidden; no native plugins or user shaders; procedures are pure compiled functions emitting ordinary patches | Low |
| GPU driver/device failure corrupts authoritative world state | High | World commits precede only immutable extraction; renderer cannot mutate world; errors are typed; fresh-service restoration is documented | Medium |
| Dependency or workflow supply chain introduces unreviewed code | High | Exact lockfile, checked-in vendor sources, pinned action digest, read-only workflow permissions, cargo-deny policy | Medium |
| Credential or private path enters the public repository | High | Local and CI Git-object scan, redacted findings, GitHub secret scanning/push protection, staged scan procedure | Low |
| A release archive is built from the wrong or moving source, altered by ambient attributes or replacement refs, populated through an implicit fetch, contains substituted/private/unsafe content, exceeds reviewed bounds, or is replaced after hashing | High | Exact direct annotated-tag object and clean-HEAD binding; pre/post ref, HEAD, cleanliness, and Git-version checks; inherited/system/global configuration neutralization; no replacement objects or lazy fetch; fixed prefix/umask; create-new external outputs; bounded Git metadata plus 256 MiB/20,000-member limits; raw sole-PAX/type/path/metadata/termination checks; exact Git inventory/blob identities; actual non-vendor public-content scan; exact SHA-256 sidecar; fail-closed cleanup | Low for accidental substitution inside the declared local preparation boundary; maintainer authentication, tag-host protection, signing, provenance, output-directory access, confidentiality, and publication policy remain separate |
| A partial candidate-version edit leaves members, local requirements, publish policy, or lock resolution inconsistent | High | One shared candidate version; inherited member versions; `publish = false`; exact path-bound local requirements; source-less first-party lock entries; bounded deterministic policy check with disposable drift cases in the ordinary quality job | Low for reviewed repository drift; tag identity, producer authentication, release-host settings, archive approval, and publication remain separate gates |
| Local API or inherited-stdio session is deployed as a remote or multi-tenant security boundary | Critical | Explicitly unsupported; no listener, peer identity, authentication, authorization, confidentiality, freshness/replay, rate, or tenancy surface exists | Not applicable inside scope; Critical if assumption is violated |

No Critical or High residual risk remains inside the declared profile. Medium
residuals are accepted for an early local source candidate and must be revisited
before automatic/mutable persistence, broader platform support, plugins,
transport, or production use.

## Abuse cases and required operator behavior

- Treat scene labels, asset bytes, replay streams, screenshots, and observations
  as potentially sensitive caller data. Do not publish them as diagnostics by
  default.
- Treat an observation-payload digest as corruption detection only. Do not use
  it to authenticate a writer, authorize a request, establish freshness, or
  protect confidentiality.
- Treat a local-frame digest and correlation identity as framing and corruption
  evidence only. Supply a bounded caller-owned stream, retain the configured
  complete/control/bulk caps, and define endpoint identity, authentication,
  authorization, confidentiality, freshness, replay, rate, timeout,
  cancellation, and partial-write recovery outside the codec before exposing
  it beyond the trusted local single-user boundary.
- Treat a source-candidate SHA-256 and Git blob identities as corruption and
  substitution evidence, not producer authentication or provenance. Keep both
  outputs in the caller-owned directory named for the reviewed run, publish
  neither after any failure or `cleanup_uncertain` result, and require a
  separately authorized immutable-tag/release policy before external use.
- Treat `0.1.0-rc.1` as an unpublished source-tree identity, not proof of
  origin, support, API stability, tag immutability, archive approval, or
  publication. Keep every package non-publishable and require the package
  policy check before any separately authorized tag step.
- Treat local-session control messages as validated instructions, not authorized
  actions. The local executor enforces lifecycle and maps each supported variant
  explicitly. The fixed stdio command adds only inherited-stream ownership,
  half-duplex scheduling, a completion deadline, flush, and shutdown policy;
  it does not add peer identity, authorization, confidentiality,
  freshness/replay, rate/tenancy control, preemptive cancellation, or remote
  safety.
- Launch `serve-stdio` only with parent-owned redirected binary stdin/stdout.
  Keep stdout separate from diagnostics, protect observation and scene bytes as
  sensitive, and discard the child/stream after any failure. A partial output
  prefix may remain; never retry a whole frame or attempt resynchronization.
- Launch `serve-mcp-stdio` only with parent-owned redirected stdin/stdout and an
  MCP client locked for the connection to exact `2025-11-25` or exact
  `2026-07-28`. In the modern era, repeat the exact protocol and client
  capabilities on every request; do not treat discovery or `serverInfo` as
  authentication or inherited authority. Treat tool annotations as descriptive
  hints, keep stdout protocol-pure, protect scene/result and observation
  resource values as sensitive,
  and discard the child/stream after any transport or service failure. The
  adapter supplies no authentication, authorization, confidentiality,
  freshness, rate/tenancy policy, general operation deadline, arbitrary
  preemption, or remote safety. Matching active-request cancellation is
  process-terminal rather than reusable and provides no rollback or effect
  receipt; its encode-before-write policy cannot retract a physical
  prefix after an inherited-stream write failure.
  Only the observation poll has a fixed 15 second deadline. Resource URIs are
  references inside the same child session, not bearer authorization or
  freshness tokens; read only the currently listed URI and validate the
  canonical envelope after base64 decoding.
  Send a matching cancellation as the next message, then reap and replace the
  child. The parent must impose any stronger process timeout and kill/reap
  fallback for blocked synchronous work.
- Do not pass credentials, private endpoints, or production records as scene
  text. Cogniform validates structure and bounds; it is not a secret sanitizer.
- Stop admitting commands when capacity errors repeat. Retrying without a
  consumer or scheduling change is an availability attack on the same process.
- Treat pending-work ages as local aggregate diagnostics. Sample them at a
  bounded operator-selected cadence; do not infer payload identity, persist
  them as causal truth, or expose them as unauthenticated remote telemetry.
- Treat controlled measurement output as locally sensitive and informational.
  Do not publish timing distributions by default, infer a portable hardware
  identity, or turn a noisy local result into a release or merge threshold.
  Scripts should require JSON `schema_version` 1 and `unit` `nanoseconds`.
- Treat canonical scenario output as locally sensitive conformance evidence.
  Do not publish the adapter name/backend or correlate its stable IDs, hashes,
  colors, counters, and pixel coverage by default. Scripts should require JSON
  `schema_version` 1 and `scenario` `canonical-mvp-v1`; a pass applies only to
  the named adapter/profile and is not a portable performance or support claim.
- Treat procedure requests as untrusted bounded data. Do not load external
  procedure code or grant a procedure filesystem, network, clock, renderer, or
  mutable-world access under this threat model.
- On renderer/device failure, stop the affected service instance. If a complete
  recovery point was captured, retain the point or explicitly create a new file
  only in an operator-approved location and restore a fresh service. The file
  is plaintext and unauthenticated; protect it according to the scene's
  sensitivity and trust only an approved writer. In-place or automatic device
  recreation is not implemented.
- Accept recovery-file paths only from the trusted local operator. Review parent
  ACLs/permissions, never rely on the digest for writer authentication or
  freshness, and inspect `PartialFileCleanup::Retained` before reusing a failed
  target. Do not expose path selection as an unauthenticated remote input or
  treat file `sync_all` as a portable directory/power-loss guarantee.
- Use `inspect-recovery` only as a trusted local, fixed-profile diagnostic.
  Keep its aggregate output private when hashes or revision/frame counts are
  sensitive, and do not treat a passing CPU preflight as authentication,
  freshness selection, asset availability, or GPU/service readiness. Scripts
  should require JSON `schema_version` 1 instead of parsing human output.
- Treat asset-file paths and their expected hash mapping as trusted local
  configuration. Do not scan an untrusted directory as a catalog, infer writer
  authenticity from SHA-256, or automatically import every stored source.
  Validate through the ordinary importer and explicitly schedule only the
  sources required by approved retained logical references.
- Use `inspect-asset` only for a trusted local expected-hash-to-path mapping.
  Keep the reported hash private when it could correlate sensitive content,
  and do not treat a pass as format validation, authorization, freshness,
  recovery association, import approval, or GPU readiness. Scripts should
  require JSON `schema_version` 1 instead of parsing human output.
- On replay tail failure, inspect only the verified prefix and preserve the
  rejected bytes for private diagnosis. Never adopt that prefix as successful
  `LocalService` recovery, skip an entry, or continue after the bad suffix.
- Treat a historical recovery point as a new branch owned by the caller. Do not
  infer freshness from its envelope digest. If both services remain live, their independent counters may
  issue equal future frame numbers; add branch identity or coordinate frame
  allocation outside Cogniform.
- Authorize and choose a live-revert target outside Cogniform. Drain transient
  work, review the explicit removed-tail/cache/asset receipt, and rehydrate
  retained asset references after success. Do not expose this unauthenticated
  local method as a remote rollback endpoint or treat it as a retention policy.
- Rehydrate a recovered asset reference only from caller-approved bytes whose
  exact hash matches the retained key. `AssetUnavailable` is not permission for
  Cogniform to discover or download content, and substituting another hash
  changes the requested logical asset rather than repairing residency.
- Authorize explicit asset eviction as a local capacity-policy decision.
  Inspect its exact outcome, retain approved source files independently, and do
  not spin an evict/reimport loop or expose eviction as an unauthenticated
  remote endpoint.
- On a repository secret finding, revoke or rotate first and coordinate history
  remediation privately. A later deletion does not remove public exposure.

## Deferred changes that require a new review

Remote transport, authentication, tenancy, automatic or mutable persistence,
recovery discovery/profile negotiation, diagnostic schemas beyond the
versioned recovery, controlled-measurement, canonical-scenario, and
asset-source-inspection CLI reports, asset catalogs,
automatic eviction/rehydration, shared memory, third-party
Wasm, model execution, arbitrary shaders, binary
releases, telemetry export, and production deployment each add a trust
boundary. None may inherit this local threat assessment without an approved
design and updated abuse/failure tests.

See the [failure and recovery guide](../operations/failure-and-recovery.md),
[validation baseline](../operations/validation-baseline.md), and
[security policy](../../SECURITY.md).
