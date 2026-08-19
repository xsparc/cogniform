# Source release-candidate checklist

Cogniform has no published or supported release. CF009 defines the evidence
for source-first `0.1.0-rc.1`, CF050 supplies the local archive
preparation/verification prerequisite, and CF051 gives the unpublished
workspace that candidate identity while every package remains non-publishable.
CF052 defines the future immutable publication, consumer-verification, and
latest-candidate support contract. No slice authorizes a repository-setting
change, tag, real archive, release draft, asset upload, crates.io publication,
release publication, or deployment.

## MVP acceptance evidence

| Capability | Status | Evidence |
|---|---|---|
| Stable identity | Pass | Stable-ID serialization, deletion/reuse, compact renderer mapping, and exact entity-ID probes |
| Atomic transaction | Pass | Full preflight, late-operation rejection, randomized model, unchanged revision/hash on failure |
| Idempotency | Pass | World and gateway replay the retained result; content conflicts reject without mutation |
| Hierarchy and transforms | Pass | Cycle/depth/dangling rejection and stable parent-before-child sparse propagation |
| Deterministic replay | Pass | Canonical logical hash, chained entries, verified-prefix inspection, every-byte corruption injection |
| Recovery envelope | Pass | Deterministic bounded v1 encoding, exact round trip, typed malformed-input rejection, and every-byte corruption injection |
| Immutable recovery file | Pass on validated profile | Encode-before-I/O create-new storage, non-overwrite, bounded regular-file load, corruption/growth rejection, injected write/sync cleanup, and persisted restoration continuation |
| Offline recovery inspection | Pass for declared CPU profile | Exact complete restoration preflight without adapter selection, unchanged human output plus deterministic CLI schema-v1 JSON, read-only file evidence, semantic/frame rejection, and path/payload redaction |
| Controlled CPU measurement | Pass for declared CPU profile | Fixed fixture/sample metadata, unchanged human structure, fixed-layout schema-v1 integer-nanosecond distributions, complete-before-output behavior, and explicit informational-only status |
| Canonical scenario report | Pass on validated profile | Unchanged 19-line human proof plus fixed-layout schema-v1 adapter/revision/observation/identity/pixel/replay evidence, complete-before-output behavior, and controlled cross-mode comparison |
| Immutable asset source file | Pass on validated profile | Pre-I/O source-size/hash validation, create-new non-overwrite, bounded regular-file load, substitution/growth rejection, injected cleanup, and separate persisted rehydration continuation |
| Offline asset-source inspection | Pass for declared CPU profile | Strict expected-hash input, bounded complete read-only identity validation, unchanged human output plus fixed-layout schema-v1 JSON, unchanged file evidence, empty failure stdout, and path/payload redaction without decoder, service, or GPU construction |
| Fresh-service restoration | Pass on validated profile | Complete replay and frame state restore revision/hash/query/idempotency and continue observation and append causality |
| Historical recovery fork | Pass on validated profile | An exact retained revision restores into a separate fresh service, preserves the source, resumes from the source frame frontier, and continues query/observe/append causality |
| Quiescent live revert | Pass on validated profile | A fully restored historical replacement swaps only after success, rejects transient blockers without mutation, clears named cache/asset state, preserves frame/prefix idempotency, and continues a new branch |
| Headless render | Pass on validated profile | No surface/window; outward-wound Vulkan cuboid, centered plane, fixed sphere, bounded directional/point direct metallic-roughness response including imported numeric GLB materials, core emissive factors, deterministic OPAQUE/MASK coverage, sampled embedded PNG base-color, packed metallic-roughness, source-tangent normal, and sRGB emissive roles, scene overrides, geometric-normal observation preservation, readback pressure, renderer-drop retirement, and canonical scenario evidence |
| Machine outputs | Pass on validated profile | Exact unlit and tolerant scene/imported/overridden direct-material color and depth, exact stable entity ID, structured visibility, and quantized world-space normals from outward built-ins, source-wound assets, sphere radial directions, or approved imported directions |
| Observation payload envelope | Pass for declared CPU profile | Fixed version-one all-kind layouts and exact fixture, canonical values and limits, metadata binding, truncation/trailing/corruption rejection, and engine source-compatible re-export without I/O or transport construction |
| Local stream frame | Pass for declared CPU profile | Fixed version-one header and exact fixture, independent header-first complete/control/bulk limits, canonical observation-envelope composition, short/interrupted I/O, back-to-back framing, and corruption/truncation rejection over caller-owned streams without an endpoint or session |
| Local-session messages | Pass for declared CPU profile | Exact unchanged schema-v1 client/server LF fixtures plus exact schema-v2 hello/imagination/completion/replay fixtures; outer-only correlation; negotiated frame/result bytes and nesting; direction/version/unknown/canonical/substitution/nested identity, revision, receipt, and role validation; no executor or endpoint authority; exhaustive message construction and `LocalSessionValidationKind` matching remain pre-stable in the unpublished source-candidate API |
| Local-session executor | Pass for declared CPU profile | One version-locked hello; field-wise peer/local/service frame and compilation limits installed before semantic work; every patch/imagination admission; both cross-kind supersession directions; deterministic one-command advancement; exact terminal command/observation correlation; compiled/unresolved/replay/service-invalid outcomes; one-time pending; two-frame output cap; redacted failures; and quiescent close |
| Local stdio session | Pass for declared CPU and controlled Windows profile | Exact pre-adapter argument/terminal/first-frame behavior, half-duplex no-read-while-live scheduling, asymmetric version-two result-limit adoption, positive-cadence fixed completion deadline, partial-output/flush/fatal handling, stable diagnostics, and passing controlled child-process v1 patch plus v2 imagination/patch/query/observation/replay/close exchanges |
| MCP stdio adapter | Pass for declared CPU and controlled Windows profile | Byte-compatible exact `2025-11-25` initialization beside exact `2026-07-28` discovery or direct requests; one pinned connection era and independent modern protocol/capability validation; a 508-byte workflow instruction; no advertised extensions or inherited discovery authority; complete modern result discrimination, informational server identity, and zero-TTL private discovery/list/read cache roles; explicit Tasks and unsupported-method rejection; exactly four ordered tools with closed mutually exclusive success/error schemas and stable vocabularies, plus resources without subscriptions/list-change; incremental input byte/nesting bounds; one active request plus one decoded pending message, with exact matching cancellation delivered before response writing and terminal no-response/no-later-dispatch behavior; wrong/missing/late cancellation remains response-through-flush; bounded encode-before-write output; typed validation before lazy service creation; serialized exact-revision query, one-command imagination, direct atomic patch application, and cooperative bounded observation; exact retained replay without duplicate compile/mutation; one exact-URI latest-value canonical resource with atomic replacement and cancellation/failure preservation; stable invalid/stale/conflict/busy/output roles; official-client, raw-wire, cancellation, stalled-writer, and controlled child-process evidence in both eras; no listener or remote authority |
| Compilation result contract | Pass for declared CPU profile | Exact compiled/unresolved schema-v1 LF fixtures, bounded canonical round trips, malformed/substituted/order/duplicate/outcome/revision rejection, nested patch validation, compiler pre-return encoded/nesting enforcement, source-path compiler re-exports, and unchanged normalization/IDs/patch bytes without session or I/O authority; exhaustive result/config construction and `CompileError` matching remain pre-stable in the unpublished source-candidate API |
| Revision causality | Pass | Receipt, extraction, renderer revision, frame, camera, observation, staleness, and visibility agree |
| Overload | Pass | Fixed capacities and tested `MustApply`, `LatestWins`, `BestEffort`, readback, asset, and replay behavior |
| Pending-work age | Pass on validated profile | Empty and retained command/observation/import/upload status, duplicate retention, supersession reset, rejection/drop neutrality, processing/eviction/delivery cleanup, saturation, and restoration/revert compatibility |
| Asset safety | Pass for documented GLB subset | Exact hash, strict framing/ranges/counts, finite non-zero same-count normals/tangents with exact handedness, finite same-count primary coordinates with full-source validation, bounded numeric materials, three-channel emissive factor, finite non-negative alpha cutoff, and normal scale, bounded static embedded RGB/RGBA PNG decode across four typed roles, shared/distinct role accounting, exact decoded/GPU bytes, truncation corpus, typed unsupported/proxy policy |
| Service asset resolution | Pass on validated profile | Explicit one-item import/upload renders an exact stable ID and optional base-color, metallic-roughness, normal, and emissive role textures; recovery retains logical references and exact-hash rehydration resumes rendering without replay mutation |
| Asset lifecycle | Pass on validated profile | Explicit content-hash-wide eviction releases exact queued, decoded, upload, mesh, and content-hash-and-role texture capacity; preserves unrelated FIFO, submitted frames, world/replay/frame state, and persisted sources; and supports exact rehydration |
| Service procedure composition | Pass on validated profile | A bounded 2x3 built-in procedure follows ordinary queue, idempotency, query, replay/hash, and restored world-idempotency behavior |
| End to end | Pass on validated profile | Room/table/light/camera create and atomic restyle, exact query, three observations, same replay hash |
| Repository and dependency hygiene | Pass | Redacted Git-object scan, secret scanning/push protection, pinned vendor/lock/action, cargo-deny |
| Source-candidate archive | Pass for the local tooling profile | Direct annotated-tag and clean-HEAD binding, repeated exact Git tar bytes, fixed metadata and bounds, raw inventory/blob/PAX/termination verification, exact SHA-256 sidecar, actual public-content scanning, negative corruption/attribute/path/type/cleanup matrix, and no extraction or publication |
| Package candidate identity | Pass for the declared source profile | All sixteen members inherit `0.1.0-rc.1` and remain non-publishable; all fifteen local crate requirements are exact and path-bound; source-less lock entries match; bounded disposable drift and live-inventory checks run in ordinary quality |

Inside the local single-user source profile, the correctness matrix passes and
the threat model has no unresolved Critical or High residual. Medium residuals,
unsupported platforms, and operational gaps are named in the linked records.
This does not make the `0.1.0-rc.1` source-candidate workspace a supported or
published release.

## Required evidence and preparation before publication

Checked items below were reproduced through CF059 and are recorded in the
validation baseline; the package/source identity matrix originated on CF051
and the executable MCP evidence was revalidated on CF054. The CF055 through
CF059 controlled GPU paths were reproduced separately on the same validated adapter. Pull-request quality
must reproduce the portable subset, and the exact clean merged/tagged commit
must remain unchanged through the still-open identity, archive, and publication
gates.

- [ ] Start from a clean, protected `main` commit whose pull-request checks and
      reviewed tree are recorded.
- [ ] Confirm no open private security advisory is a Critical or High blocker
      for the declared profile.
- [x] Reproduce the ordinary offline format, Clippy, workspace-test, rustdoc,
      public-tree, and dependency-policy checks.
- [x] Reproduce all ignored engine/renderer conformance tests on at least one
      matrix entry named in the compatibility baseline.
- [x] Re-run the canonical scenario's human and schema-version-one JSON modes
      on that matrix entry and confirm they prove the same successful run
      contract without publishing the adapter or run evidence.
- [x] Re-run `measure-world` human and schema-version-one JSON output in release
      mode and append rather than overwrite
      the dated baseline if hardware, fixture, or result materially changes.
- [x] Re-run the CPU-only asset-source inspection black-box contract and
      confirm exact human and schema-version-one hash/byte output, file
      immutability, bounded failure, empty failure stdout, and path/payload
      redaction.
- [x] Re-run the CPU-only observation-envelope contract and confirm all-kind
      fixtures, canonical validation, metadata binding, corruption rejection,
      and configured limits without constructing a listener or GPU adapter.
- [x] Re-run the CPU-only local-frame contract and confirm the exact header,
      header-first independent limits, short/interrupted I/O, back-to-back
      framing, nested observation integrity, and payload-redacted failures
      without constructing an endpoint, session, service, or GPU adapter.
- [x] Re-run the CPU stdio fake-stream/CLI contract and the controlled ignored
      child-process session on an approved adapter; confirm exact arguments,
      immediate clean EOF, hello limit adoption, half-duplex terminal delivery,
      deadline/fault behavior, applied/query/observation causality, orderly
      close, and no trailing output.
- [x] Re-run the CPU-only compilation-result and compiler contracts; confirm
      exact compiled/unresolved fixtures, byte/nesting/logical/text/count/patch
      bounds, canonical equality, code-field roles, order/uniqueness,
      outcome/revision binding, source-path compiler re-exports, unchanged
      normalization/IDs/patch bytes, and the unpublished pre-stable exhaustive
      result/config construction and `CompileError` matching breaks without
      constructing a session, service, endpoint, or GPU adapter.
- [x] Re-run the CPU-only local-session/executor version-two contracts and the
      controlled ignored child-process flow on an approved adapter; confirm
      unchanged version-one fixtures, explicit compilation-limit negotiation,
      imagination admission, compiled/unresolved completion, exact replay
      without duplicate processing, query/observation causality, and orderly
      close.
- [x] Re-run the ordinary MCP adapter/CLI contracts and the controlled ignored
      child-process flows on an approved adapter; confirm byte-compatible exact
      `2025-11-25` negotiation and exact `2026-07-28` discovery/direct requests,
      one pinned era, independent modern metadata validation, exact 508-byte
      workflow instructions, four-tool order,
      deterministic closed success/error schema metadata and stable vocabularies,
      authoritative typed core validation, pessimistic annotations, and byte
      and nesting equality,
      validation before lazy service creation, exact-revision query, one-command
      imagination and direct camera-patch application, retained replay without
      duplicate mutation, stale/conflicting rejection, bounded stdout, stable
      redacted failures, exact-revision observation, canonical base64 resource
      link/list/read, atomic latest-value replacement and failure preservation,
      one-request-through-flush pipelined-read backpressure, complete modern
      result/server/cache roles, and clean EOF. Re-review the exact-pinned
      SDK/runtime features, excluded Tasks and unsupported modern methods,
      `uuid`/`getrandom` reachability, vendored build scripts, and
      advisory policy from the candidate tree.
- [x] Review `CHANGELOG.md`, the threat model, failure/recovery guide, support
      matrix, recovery-envelope, recovery-file, and asset-file
      formats/limitations, offline inspection profile/versioned output, known
      limitations, and license from the exact candidate tree.
- [x] Confirm the exact reviewed tree passes the package-policy gate: shared
      `0.1.0-rc.1`, sixteen inherited non-publishable members, fifteen exact
      path-bound local requirements, and matching source-less lock entries.
- [ ] After squash merge and under separate explicit authorization, create the
      annotated maintainer tag on the exact clean protected `main` commit.
- [ ] From that exact clean tagged commit, prepare the candidate into one
      existing caller-owned directory outside the worktree and Git directory:

      ```text
      python scripts/source_candidate.py prepare --repository . --tag refs/tags/v0.1.0-rc.1 --archive <outside-repository-dir>/cogniform-0.1.0-rc.1.tar --checksum <outside-repository-dir>/cogniform-0.1.0-rc.1.tar.sha256
      ```

      Confirm the schema-version-one report names the reviewed tag object,
      peeled commit, Git implementation/object format, archive byte/member
      counts, and SHA-256. Use a complete local object store: the command
      refuses lazy object fetching rather than opening a network connection.
      The command itself never creates or moves a tag.
- [ ] Independently reopen the unchanged files under the same exact clean tag
      and repository state:

      ```text
      python scripts/source_candidate.py verify --repository . --tag refs/tags/v0.1.0-rc.1 --archive <outside-repository-dir>/cogniform-0.1.0-rc.1.tar --checksum <outside-repository-dir>/cogniform-0.1.0-rc.1.tar.sha256
      ```

      Require an identical report. This proves the fixed prefix, sole commit
      PAX value, canonical termination, exact directory/file inventory and Git
      blob identities, fixed metadata, mandatory offline-build content,
      reusable public path/content rules, sidecar bytes, and hard bounds
      without extraction.
- [ ] Obtain maintainer approval for the exact tag, archive hash, release notes,
      support statement, and residual risks.

## Publication procedure

Publication is deliberately manual. The following six gates require distinct
explicit maintainer authorizations and recorded completion; authority for one
gate never authorizes a later gate:

1. enable GitHub release immutability for the repository and verify that it
   applies to future releases;
2. create the annotated `v0.1.0-rc.1` tag on the exact reviewed protected
   `main` commit;
3. prepare and independently verify the source archive and checksum under that
   unchanged tag;
4. create a prerelease draft against the pre-existing tag;
5. upload exactly `cogniform-0.1.0-rc.1.tar` and
   `cogniform-0.1.0-rc.1.tar.sha256` to the draft without `--clobber` or any
   replacement operation; and
6. review the complete draft, then publish it under a final separate
   authorization while release immutability remains enabled.

GitHub documents that release immutability applies only to future releases, so
the setting must be enabled before the draft is created. A mismatched or
incomplete draft is discarded rather than repaired through asset replacement.
Do not substitute GitHub's generated compressed source download for either
named asset, and do not upload binaries, containers, symbols, runtime logs,
benchmark artifacts, observations, replay streams, or private test data.

The GitHub release must be marked prerelease and state:

- source-only, early local evaluation;
- only the latest published candidate is eligible for security fixes, support
  ends immediately on replacement or withdrawal, and there is no minimum
  lifetime, SLA, bounty, or backport promise;
- validated Windows/Vulkan profile and build-only Ubuntu evidence;
- no remote service, authentication, automatic checkpoint/startup, mutable
  snapshot catalog, background telemetry exporter, production SLA, or
  semver-stable crates.io API;
- recovery envelopes detect corruption but provide no encryption,
  authentication, freshness, or rollback protection;
- local recovery files are explicit create-new plaintext artifacts with no
  overwrite, latest-pointer, retention, directory-sync, power-loss,
  authentication, or remote-storage guarantee;
- offline recovery inspection uses one fixed local profile and proves only the
  complete CPU restoration preflight; it does not authenticate the writer,
  choose the freshest file, initialize a GPU/service, or discover assets; its
  optional compact JSON report is CLI schema version one and aggregate hashes
  and counts can still be sensitive;
- the controlled CPU measurement's optional compact JSON report is CLI schema
  version one, contains no hardware identity, and is informational only; timing
  values can still expose local performance characteristics and are not a
  release or merge threshold;
- the canonical scenario's optional compact JSON proof is CLI schema version
  one and can disclose adapter identity plus correlatable stable IDs, hashes,
  colors, counters, and pixel evidence; it remains local and opt-in, makes no
  portable performance or additional-adapter claim, and is not uploaded by
  default;
- exact-hash asset sources are separate create-new plaintext artifacts with no
  recovery manifest, content discovery, catalog, automatic retention/eviction,
  automatic rehydration, writer authentication, or remote-storage guarantee;
- offline asset-source inspection accepts one caller-supplied expected hash and
  path and proves only bounded byte identity. Its optional compact JSON report
  is CLI schema version one; both modes can disclose a correlatable hash and do
  not validate format, renderability, authenticity, freshness, authorization,
  recovery association, or GPU readiness, nor schedule import, upload, or
  rehydration;
- observation payload envelopes are optional in-memory corruption-detection
  values, not authenticated or encrypted messages;
- local stream frames add a fixed header and enforce independent complete,
  control, and bulk limits before body allocation over caller-owned I/O, but
  provide no endpoint, listener, service loop, or session identity by
  themselves and add no
  authentication, authorization, confidentiality, freshness, replay
  protection, rate limit, timeout/cancellation policy, shared-memory lease,
  retention policy, writer atomicity after an I/O failure, or automatic
  delivery;
- local-session messages define only bounded in-memory control values and frame
  adaptation; the separate local executor sequences one caller-driven
  lifecycle and executes one supplied local service. Neither owns stdin/stdout,
  schedules itself, or creates an endpoint;
- compilation results are bounded schema-version-one in-memory or canonical LF
  values that bind an imagination and exact scene revision to one normalized
  patch or unresolved issues. The value crate does not compile, execute,
  mutate, authenticate, persist, or transport them. Local-session schema
  version two can carry the values under explicit negotiated limits through
  the existing executor and stdio profile; version one cannot;
- `cogniform-cli serve-stdio` supplies one fixed inherited redirected-stream
  owner and one 64x64 local service. It is half-duplex, flushes every frame,
  supports patch and version-two imagination commands, polls live completion
  every 2 ms under a 15-second deadline, and terminates
  on truncation, corruption, incomplete session EOF, fatal service/executor
  state, deadline, write, or flush failure. It does not recover or retry a
  partial write, preempt a blocked synchronous operation, create a pipe,
  listener, socket, or process, authenticate or authorize a peer, add
  confidentiality/freshness/replay/rate/tenancy policy, supervise/restart the
  child, or support configurable, multi-client, full-duplex, or remote use;
- `cogniform-cli serve-mcp-stdio` supplies one fixed inherited redirected-stream
  owner, byte-compatible exact MCP `2025-11-25` plus exact MCP `2026-07-28`
  discovery/direct-request lifecycles, four fixed tools, one in-memory latest-
  value observation resource, and one lazily initialized 64x64 local service.
  A connection pins one era; every modern request repeats exact protocol and
  client-capability metadata. Discovery and informational server identity grant
  no extension or authentication authority. It
  serializes tool calls and bounds newline input plus encoded output. A matching
  active request cancellation is observable only before response writing and
  terminates the child without a response or later dispatch; it supplies no
  rollback, effect receipt, reusable cancellation, general deadline, listener,
  peer identity, authentication, authorization, confidentiality, freshness,
  rate/tenancy policy, restart supervision, multi-client, or remote use;
- historical recovery supports caller-coordinated fresh forks and quiescent
  live replacement, but provides no automatic rollback, authorization,
  freshness, branch manager, or global frame namespace across concurrent
  branches;
- asset bytes, decoded meshes, and GPU residency are not recovery state;
  callers may map retained hashes to separate approved files but must rehydrate
  exact matching content explicitly; no filesystem/network resolver, durable
  mutable cache, automatic eviction policy, or automatic asset startup exists;
- only compiled pure built-in procedures are supported; no external procedure
  loading, user-code execution, native plugin, or Wasm host is included;
- normal output is quantized and retains documented geometric smoothing while
  direct lighting may use one source-tangent normal map and one linear packed
  metallic-roughness map; generated tangents,
  other normal-texture coordinate sets, and unvalidated
  platforms/backends remain
  unsupported; and
- built-in rendering supports outward-wound centered unit cuboids, fixed
  centered XY planes, and fixed centered unit-diameter spheres; one documented
  embedded PNG base-color, metallic-roughness, normal, and emissive roles can sample
  imported primary coordinates;
  configurable subdivisions, plane thickness, generated coordinate mappings,
  and two-sided normal policy are not implemented;
  and
- lighting supports independently bounded sets of at most four stable-ordered
  directional and four point definitions with direct GGX/Smith/Schlick
  metallic-roughness response, exact unlit compatibility, imported numeric GLB
  factors, bounded surface-only core emissive factors, and explicit scene
  override precedence; configurable point
  range/radius/cutoff, spot lights, ambient/image-based lighting, shadows,
  emissive strength/cross-surface illumination and alpha blending/sorting,
  HDR/tone mapping, configurable gamma conversion,
  occlusion textures, additional image
  formats/samplers/material texture roles, and lighting
  configuration are not implemented; and
- performance figures are one-machine informational measurements.

If a candidate is defective, close or supersede its release entry as
appropriate and issue a new incremented candidate after review. Published tags
and assets remain immutable; do not move, reuse, delete, or replace them to
deliver a fix. After publication, reproduce every command in the
[consumer verification procedure](support.md#consumer-verification) against a
fresh empty directory and record the exact tag, commit, two asset names, and
SHA-256 in the release evidence.

## Current disposition

The deterministic local preparation/verification prerequisite and unpublished
`0.1.0-rc.1` workspace identity are implemented, and the future immutable
publication and support contract is documented. A taggable candidate still
requires complete checklist reproduction and exact reviewed-tree approval.
Repository-setting, annotated-tag, archive, draft, upload, and publication
actions each retain separate authority. CF009-CF052 create no tag or release.

See [ADR 0010](../adr/0010-source-first-release-profile.md),
[ADR 0050](../adr/0050-deterministic-source-candidate-archive.md),
[ADR 0051](../adr/0051-version-source-candidate-without-publication.md),
[ADR 0052](../adr/0052-immutable-source-release-and-support-contract.md), the
[release integrity and support policy](support.md), the
[validation baseline](../operations/validation-baseline.md), the
[failure guide](../operations/failure-and-recovery.md), the
[recovery-file guide](../persistence/recovery-files.md), the
[asset-file guide](../persistence/asset-files.md), and the
[MVP threat model](../threat-model/mvp.md).
