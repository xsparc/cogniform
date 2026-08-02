# MVP threat model

Status: reviewed for the local source-first candidate profile on 2026-08-02.

This model covers the in-process, single-user Cogniform MVP. It does not claim
that the engine is an authentication, authorization, multi-tenant, remote, or
production security boundary. Exposing it to untrusted remote clients without a
separate transport and identity design violates the assumptions below.

## Assets and security objectives

| Asset | Objective |
|---|---|
| Authoritative world | Only complete validated patches change state; stable IDs and revisions remain correct |
| Accepted-event log | Newly accepted patches are complete, ordered, bounded, integrity checked, and replayable |
| Observation causality | Payload, camera, frame, revision, and stable identity agree |
| Asset state | Source identity is exact; malformed or oversized input cannot become decoded or GPU-resident state |
| Host resources | CPU, memory, queues, GPU allocations, and waits stay within declared bounds |
| Process and GPU | Backend failures return controlled errors and do not grant access to world mutation |
| Repository and release | No credentials, private workflow state, paid calls, or unreviewed artifacts enter public history |

Availability is bounded rather than guaranteed. Confidentiality applies to
project and operator data, but Cogniform does not encrypt runtime scene or
replay values. A caller that places sensitive text in a scene also places it in
snapshots and canonical replay entries.

## Trust boundaries

1. **Caller to protocol.** Patches, imagination, queries, IDs, labels, limits,
   and observation requests are untrusted typed or JSON-derived data.
2. **Asset bytes to decoder.** A claimed SHA-256 identity and GLB bytes cross
   into a strict, separately scheduled parser.
3. **World to renderer.** The renderer receives immutable compact extraction,
   never mutable ECS state or an authorization decision.
4. **Renderer to observation worker.** GPU readback crosses an asynchronous
   fixed-capacity lease and returns owned validated payloads.
5. **Replay bytes to recovery.** Portable bytes are untrusted until the header,
   length, protocol, revision, scene-hash, predecessor, and entry-hash chains
   have been verified.
6. **Repository to public hosting.** Tracked content, commit metadata, workflow
   definitions, dependencies, and future release artifacts become public.

The compiler and built-in procedures have no ambient filesystem, network, time,
world, renderer, or entropy authority. The current local service creates no
socket, listener, shared-memory segment, persistent file, or model call.

## Threat and control matrix

Residual ratings assume the declared local single-user boundary.

| Threat | Inherent risk | Controls and evidence | Residual |
|---|---|---|---|
| Oversized or deeply nested messages exhaust CPU or memory | High | Pre-decode byte/nesting caps, bounded collections and budgets, fail-before-mutation tests | Low |
| Malformed or adversarial GLB allocates excessively or reaches GPU state | High | Exact content hash, source/decoded/count limits, strict subset, explicit processing, truncation corpus, unsafe proxy exclusions | Medium |
| Stale, conflicting, or partially invalid patch mutates part of the world | High | Exact base revision, complete preflight plan, atomic commit, invariant/property tests | Low |
| Idempotency-key reuse duplicates or substitutes work | High | Retained canonical command fingerprint, transaction identity, conflict error, exact replayed receipt | Low |
| Queue or readback pressure creates hidden unbounded work | High | Fixed command, idempotency, asset, renderer, and observation capacities with typed rejection/drop/supersession; per-renderer retirement guard keeps final driver destruction off the caller's bounded read path | Low |
| Renderer-local IDs escape as authoritative identity | High | Frame-local compact mapping, stable IDs in public observations, exact center-pixel tests | Low |
| Observation from an old camera or revision is accepted as current | High | Camera/frame/revision metadata, explicit staleness, source-ahead rejection, canonical scenario proof | Low |
| Replay bytes are truncated, reordered, or modified | High | Append-only SHA-256 chain, verified-prefix loader, exact replay checks, every-byte corruption injection | Low |
| Scene text or replay data discloses caller secrets | High | No automatic logging, upload, persistence, or release; debug output is aggregate; operator warning and public-repo scan | Medium |
| Native code or a procedure escapes its authority | High | Unsafe Rust forbidden; no native plugins or user shaders; procedures are pure compiled functions emitting ordinary patches | Low |
| GPU driver/device failure corrupts authoritative world state | High | World commits precede only immutable extraction; renderer cannot mutate world; errors are typed; process restart is documented | Medium |
| Dependency or workflow supply chain introduces unreviewed code | High | Exact lockfile, checked-in vendor sources, pinned action digest, read-only workflow permissions, cargo-deny policy | Medium |
| Credential or private path enters the public repository | High | Local and CI Git-object scan, redacted findings, GitHub secret scanning/push protection, staged scan procedure | Low |
| Local API is deployed as a remote or multi-tenant security boundary | Critical | Explicitly unsupported; no listener/auth/session surface exists | Not applicable inside scope; Critical if assumption is violated |

No Critical or High residual risk remains inside the declared profile. Medium
residuals are accepted for an early local source candidate and must be revisited
before broader platform support, persistence, plugins, transport, or production
use.

## Abuse cases and required operator behavior

- Treat scene labels, asset bytes, replay streams, screenshots, and observations
  as potentially sensitive caller data. Do not publish them as diagnostics by
  default.
- Do not pass credentials, private endpoints, or production records as scene
  text. Cogniform validates structure and bounds; it is not a secret sanitizer.
- Stop admitting commands when capacity errors repeat. Retrying without a
  consumer or scheduling change is an availability attack on the same process.
- On renderer/device failure, stop the affected service instance, retain replay
  bytes only in an operator-approved location, and restart from a verified
  source. In-process device recovery is not implemented.
- On replay tail failure, use only the verified prefix and preserve the rejected
  bytes for private diagnosis. Never skip an entry or continue after the bad
  suffix.
- On a repository secret finding, revoke or rotate first and coordinate history
  remediation privately. A later deletion does not remove public exposure.

## Deferred changes that require a new review

Remote transport, authentication, tenancy, persistent replay loading, shared
memory, third-party Wasm, model execution, arbitrary shaders, binary releases,
telemetry export, and production deployment each add a trust boundary. None may
inherit this local threat assessment without an approved design and updated
abuse/failure tests.

See the [failure and recovery guide](../operations/failure-and-recovery.md),
[validation baseline](../operations/validation-baseline.md), and
[security policy](../../SECURITY.md).
