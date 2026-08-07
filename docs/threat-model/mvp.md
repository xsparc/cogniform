# MVP threat model

Status: reviewed for the local source-first candidate profile on 2026-08-02
and extended through CF037 versioned asset-source inspection JSON on 2026-08-08.

This model covers the in-process, single-user Cogniform MVP. It does not claim
that the engine is an authentication, authorization, multi-tenant, remote, or
production security boundary. Exposing it to untrusted remote clients without a
separate transport and identity design violates the assumptions below.

## Assets and security objectives

| Asset | Objective |
|---|---|
| Authoritative world | Only complete validated patches change state; stable IDs and revisions remain correct |
| Accepted-event log, recovery point, and recovery file | Newly accepted patches stay complete, ordered, bounded, integrity checked, and replayable; complete or exact-revision replay bytes remain associated with non-reused frame-continuity state; explicit files never overwrite an existing target; diagnostics reveal no path or payload |
| Observation causality | Payload, camera, frame, revision, and stable identity agree |
| Asset state and source files | Source identity is exact; malformed or oversized input cannot become decoded or GPU-resident state; recovered references and caller-mapped source files cannot substitute different bytes |
| Host resources | CPU, memory, queues, GPU allocations, and waits stay within declared bounds and explicit release is exactly accounted |
| Process and GPU | Backend failures return controlled errors and do not grant access to world mutation |
| Repository and release | No credentials, private workflow state, paid calls, or unreviewed artifacts enter public history |

Availability is bounded rather than guaranteed. Confidentiality applies to
project and operator data, but Cogniform does not encrypt runtime scene or
replay values. A caller that places sensitive text in a scene also places it in
snapshots and canonical replay entries.

## Trust boundaries

1. **Caller to protocol and pure preparation.** Patches, imagination,
   procedures, queries, IDs, labels, limits, and observation requests are
   untrusted typed or JSON-derived data.
2. **Asset file/bytes to storage and decoder.** A caller-selected path, claimed
   SHA-256 identity, and GLB/embedded PNG bytes cross bounded file checks before
   separately crossing into strict, explicitly scheduled parsers.
3. **World to renderer.** The renderer receives immutable compact extraction,
   never mutable ECS state or an authorization decision.
4. **Renderer to observation worker.** GPU readback crosses an asynchronous
   fixed-capacity lease and returns owned validated payloads.
5. **Recovery file/envelope and replay bytes to recovery.** Caller-selected
   paths and portable bytes are untrusted until regular-file/size/growth checks,
   envelope header/version/bounds/length/frame/digest, and replay
   protocol/revision/scene-hash/predecessor/entry-hash chains have been
   independently verified.
6. **Repository to public hosting.** Tracked content, commit metadata, workflow
   definitions, dependencies, and future release artifacts become public.

Exact-hash asset files use the same final-file type, size, growth, create-new,
cleanup, and path-redaction boundary, followed by complete identity validation.
The compiler and built-in procedures have no ambient filesystem, network, time,
world, renderer, or entropy authority. The current local service creates no
socket, listener, shared-memory segment, automatic persistent file, or model
call. The separate storage adapter touches only an explicit caller-selected
path when directly invoked.
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
| Malformed, substituted, or adversarial GLB/PNG allocates excessively or reaches GPU state | High | Service-owned exact-hash admission, source/decoded/count limits, strict geometry/image subset, exact 32-byte expanded-vertex reservation, finite non-zero same-count normal, full-source finite same-count primary-coordinate, unit-bounded numeric material validation, PNG dimension/pixel/working/decoded bounds, separately reserved unique GPU texture count/bytes, exact-pinned vendored decoder, explicit one-item processing, empty recovery residency, truncation corpus, unsafe proxy exclusions | Medium |
| Stale, conflicting, or partially invalid patch mutates part of the world | High | Exact base revision, complete preflight plan, atomic commit, invariant/property tests | Low |
| Idempotency-key reuse duplicates or substitutes work | High | Retained canonical command fingerprint, transaction identity, conflict error, exact replayed receipt | Low |
| Adversarial procedure dimensions or text allocate unbounded output or bypass mutation controls | High | Pure built-in implementation, entity/patch/decoded/text preflight under active runtime limits, ordinary gateway admission and atomic patch processing, controlled restoration test | Low |
| Queue or readback pressure creates hidden unbounded work | High | Fixed command, idempotency, asset, renderer, and observation capacities with typed rejection/drop/supersession; optional aggregate oldest-pending ages expose stalled caller-driven work; per-renderer retirement guard keeps final driver destruction off the caller's bounded read path | Low |
| Queue diagnostics disclose queued content or create a hidden telemetry channel | High | Status/debug expose only optional saturating elapsed microseconds and existing aggregate counts; monotonic instants stay process-local and out of payloads, identifiers, system time, durable state, logs, exporters, and background workers | Low inside the local caller boundary |
| A caller churns eviction and reimport to amplify decode, upload, or GPU-retirement work | High | Explicit trusted-local content-hash API only; one-item bounded import/upload; exact release outcomes; no background eviction, retry, or automatic rehydration; submitted work remains safely retired | Low inside the local caller boundary; remote exposure would require rate and authorization controls |
| Adversarial light or material definitions create unbounded per-frame GPU work, invalid directions, non-finite positions, or singular direct-light math | High | Independent four-definition directional/point caps, stable preparation, zero-padded fixed uniform, finite/unit-bounded scene and imported values, finite selected-camera conversion, roughness and BRDF denominator floors, degenerate active-direction and out-of-range active-position rejection, exact-zero and derived-distance-overflow handling, and pre-submit tests | Low |
| Renderer-local IDs escape as authoritative identity | High | Frame-local compact mapping, stable IDs in public observations, exact center-pixel tests | Low |
| Observation from an old camera or revision is accepted as current | High | Camera/frame/revision metadata, explicit staleness, source-ahead rejection, canonical scenario proof | Low |
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
| Local API is deployed as a remote or multi-tenant security boundary | Critical | Explicitly unsupported; no listener/auth/session surface exists | Not applicable inside scope; Critical if assumption is violated |

No Critical or High residual risk remains inside the declared profile. Medium
residuals are accepted for an early local source candidate and must be revisited
before automatic/mutable persistence, broader platform support, plugins,
transport, or production use.

## Abuse cases and required operator behavior

- Treat scene labels, asset bytes, replay streams, screenshots, and observations
  as potentially sensitive caller data. Do not publish them as diagnostics by
  default.
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
