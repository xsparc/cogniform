# Source release-candidate checklist

Cogniform has no published or supported release. CF009 prepares the evidence
for a future source-first `0.1.0-rc.1`; it does not authorize a version change,
tag, archive, GitHub release, crates.io publication, or deployment.

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
| Headless render | Pass on validated profile | No surface/window; outward-wound Vulkan cuboid, centered plane, fixed sphere, bounded directional/point direct metallic-roughness response including imported numeric GLB materials, one sampled embedded PNG base-color texture, scene overrides, readback pressure, renderer-drop retirement, and canonical scenario evidence |
| Machine outputs | Pass on validated profile | Exact unlit and tolerant scene/imported/overridden direct-material color and depth, exact stable entity ID, structured visibility, and quantized world-space normals from outward built-ins, source-wound assets, sphere radial directions, or approved imported directions |
| Observation payload envelope | Pass for declared CPU profile | Fixed version-one all-kind layouts and exact fixture, canonical values and limits, metadata binding, truncation/trailing/corruption rejection, and engine source-compatible re-export without I/O or transport construction |
| Local stream frame | Pass for declared CPU profile | Fixed version-one header and exact fixture, independent header-first complete/control/bulk limits, canonical observation-envelope composition, short/interrupted I/O, back-to-back framing, and corruption/truncation rejection over caller-owned streams without an endpoint or session |
| Local-session messages | Pass for declared CPU profile | Exact schema-v1 client/server LF fixtures, every-variant round trips, outer-only correlation, effective byte/nesting bounds, direction/version/unknown/canonical/substitution/nested validation, exact-revision observation admission, and no executor or endpoint |
| Local-session executor | Pass for declared CPU profile | One hello, field-wise peer/local/service limits, deterministic one-command advancement, exact terminal command/observation correlation, one-time pending, two-frame output cap, redacted failures, quiescent close, and a passing controlled service adapter without endpoint authority |
| Revision causality | Pass | Receipt, extraction, renderer revision, frame, camera, observation, staleness, and visibility agree |
| Overload | Pass | Fixed capacities and tested `MustApply`, `LatestWins`, `BestEffort`, readback, asset, and replay behavior |
| Pending-work age | Pass on validated profile | Empty and retained command/observation/import/upload status, duplicate retention, supersession reset, rejection/drop neutrality, processing/eviction/delivery cleanup, saturation, and restoration/revert compatibility |
| Asset safety | Pass for documented GLB subset | Exact hash, strict framing/ranges/counts, finite non-zero same-count normals, finite same-count primary coordinates with full-source validation, unit-bounded numeric materials, bounded static embedded RGB/RGBA PNG decode, independent texture accounting, exact decoded/GPU bytes, truncation corpus, typed unsupported/proxy policy |
| Service asset resolution | Pass on validated profile | Explicit one-item import/upload renders an exact stable ID and optional shared texture; recovery retains logical references and exact-hash rehydration resumes rendering without replay mutation |
| Asset lifecycle | Pass on validated profile | Explicit content-hash-wide eviction releases exact queued, decoded, upload, mesh, and unique-texture capacity; preserves unrelated FIFO, submitted frames, world/replay/frame state, and persisted sources; and supports exact rehydration |
| Service procedure composition | Pass on validated profile | A bounded 2x3 built-in procedure follows ordinary queue, idempotency, query, replay/hash, and restored world-idempotency behavior |
| End to end | Pass on validated profile | Room/table/light/camera create and atomic restyle, exact query, three observations, same replay hash |
| Repository and dependency hygiene | Pass | Redacted Git-object scan, secret scanning/push protection, pinned vendor/lock/action, cargo-deny |

Inside the local single-user source profile, the correctness matrix passes and
the threat model has no unresolved Critical or High residual. Medium residuals,
unsupported platforms, and operational gaps are named in the linked records.
This does not make the current `0.0.0` workspace a supported release.

## Required evidence before proposing a candidate tag

- [ ] Start from a clean, protected `main` commit whose pull-request checks and
      reviewed tree are recorded.
- [ ] Confirm no open private security advisory is a Critical or High blocker
      for the declared profile.
- [ ] Reproduce the ordinary offline format, Clippy, workspace-test, rustdoc,
      public-tree, and dependency-policy checks.
- [ ] Reproduce all ignored engine/renderer conformance tests on at least one
      matrix entry named in the compatibility baseline.
- [ ] Re-run the canonical scenario's human and schema-version-one JSON modes
      on that matrix entry and confirm they prove the same successful run
      contract without publishing the adapter or run evidence.
- [ ] Re-run `measure-world` human and schema-version-one JSON output in release
      mode and append rather than overwrite
      the dated baseline if hardware, fixture, or result materially changes.
- [ ] Re-run the CPU-only asset-source inspection black-box contract and
      confirm exact human and schema-version-one hash/byte output, file
      immutability, bounded failure, empty failure stdout, and path/payload
      redaction.
- [ ] Re-run the CPU-only observation-envelope contract and confirm all-kind
      fixtures, canonical validation, metadata binding, corruption rejection,
      and configured limits without constructing a listener or GPU adapter.
- [ ] Re-run the CPU-only local-frame contract and confirm the exact header,
      header-first independent limits, short/interrupted I/O, back-to-back
      framing, nested observation integrity, and payload-redacted failures
      without constructing an endpoint, session, service, or GPU adapter.
- [ ] Review `CHANGELOG.md`, the threat model, failure/recovery guide, support
      matrix, recovery-envelope, recovery-file, and asset-file
      formats/limitations, offline inspection profile/versioned output, known
      limitations, and license from the exact candidate tree.
- [ ] Change the shared workspace version from `0.0.0` to the explicitly
      approved candidate version without changing `publish = false`.
- [ ] Build the source archive from the annotated candidate tag, not a dirty
      working tree, and verify it includes `Cargo.lock`, `vendor/`, docs, tests,
      `LICENSE`, and no ignored local orchestration state.
- [ ] Run the public-repository scan against the archive file list and content,
      then compute and independently verify its SHA-256 checksum.
- [ ] Obtain maintainer approval for the exact tag, archive hash, release notes,
      support statement, and residual risks.

## Publication procedure

Publication is deliberately manual and requires a new explicit authorization.
After every checklist item passes, the maintainer may create an annotated tag,
produce the source archive and checksum, and publish a prerelease entry that
links this evidence. Do not upload binaries, containers, symbols, runtime logs,
benchmark artifacts, observations, replay streams, or private test data.

The GitHub release must be marked prerelease and state:

- source-only, early local evaluation;
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
  provide no endpoint, listener, service loop, session identity,
  authentication, authorization, confidentiality, freshness, replay
  protection, rate limit, timeout/cancellation policy, shared-memory lease,
  retention policy, writer atomicity after an I/O failure, or automatic
  delivery;
- local-session messages define only bounded in-memory control values and frame
  adaptation; the separate local executor sequences one caller-driven
  lifecycle and executes one supplied local service. Neither owns stdin/stdout,
  recovers partial writes, authenticates or authorizes a peer, enforces remote
  freshness/replay/rate/tenancy policy, schedules itself, or creates an
  endpoint;
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
- normal output is quantized and accepts the documented imported smoothing
  subset, with no normal maps or tangent-space contract; unvalidated
  platforms/backends remain
  unsupported; and
- built-in rendering supports outward-wound centered unit cuboids, fixed
  centered XY planes, and fixed centered unit-diameter spheres; one documented
  embedded PNG base-color texture can sample imported primary coordinates;
  configurable subdivisions, plane thickness, generated coordinate mappings,
  and two-sided normal policy are not implemented;
  and
- lighting supports independently bounded sets of at most four stable-ordered
  directional and four point definitions with direct GGX/Smith/Schlick
  metallic-roughness response, exact unlit compatibility, imported numeric GLB
  factors, and explicit scene override precedence; configurable point
  range/radius/cutoff, spot lights, ambient/image-based lighting, shadows,
  emissive and alpha material modes, HDR/tone mapping, configurable gamma
  conversion, additional image formats/samplers/texture roles, and lighting
  configuration are not implemented; and
- performance figures are one-machine informational measurements.

If a candidate is defective, close or supersede its release entry as
appropriate and issue a new incremented candidate after review. Do not move a
published tag or replace an archive under the same version.

## Current disposition

The implementation and evidence are suitable for a separately reviewed source
release-candidate preparation task. No tag or release is created by CF009.

See [ADR 0010](../adr/0010-source-first-release-profile.md), the
[validation baseline](../operations/validation-baseline.md), the
[failure guide](../operations/failure-and-recovery.md), the
[recovery-file guide](../persistence/recovery-files.md), the
[asset-file guide](../persistence/asset-files.md), and the
[MVP threat model](../threat-model/mvp.md).
