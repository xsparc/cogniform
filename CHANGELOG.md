# Changelog

All notable project changes will be recorded here. Cogniform has no published
release yet; the current workspace version remains `0.0.0`.

## Unreleased

### Added

- bounded canonical protocol values, limits, patches, receipts, queries, and
  observation metadata;
- atomic authoritative world state with stable identity, hierarchy, transforms,
  snapshots, render extraction, and canonical logical hashing;
- integrity-chained accepted-event recording, verified-prefix recovery, and
  exact fresh-world replay;
- bounded headless renderer paths compiled for Vulkan/DX12 with primitive and
  strict GLB geometry, color/depth/entity-ID readback, quantized world-space
  normal observations from flat fallback or imported vertex directions,
  structured visibility, and guarded
  background GPU retirement;
- deterministic primitive imagination compilation and pure seeded built-in
  cuboid-grid procedures;
- local-service execution of pure bounded procedures through ordinary patch
  admission, idempotency, processing, query, replay, and restoration;
- bounded content-addressed GLB admission, CPU decode, renderer upload, and
  explicit unsupported/proxy policy;
- optional finite same-count GLB vertex normals, deterministic winding fallback,
  exact 24-byte position/normal accounting, interleaved GPU upload, and
  inverse-transpose rendering under non-uniform scale;
- fixed centered XY plane rendering with counter-clockwise positive-Z winding,
  all-axis model scaling, exact primitive fallback selection, stable identity,
  and bounded color/depth/normal readback;
- service-owned asset admission, explicit single-item CPU/GPU processing,
  aggregate residency status, and exact-hash post-recovery rehydration;
- local typed service and unattended room/table/light/camera scenario;
- complete verified in-memory local-service restoration with retained replay,
  logical state, idempotency, renderer revision, and frame continuity;
- deterministic bounded version-one recovery-point envelopes that bind replay
  bytes and frame continuity with typed validation and SHA-256 corruption
  detection before payload allocation;
- exact-revision replay prefixes and fresh-service historical recovery forks
  that preserve the source service and carry its current next frame identity;
- quiescent in-place historical local-service revert through a fully restored
  replacement, with typed blockers, explicit cache/asset clearing, and frame,
  replay, idempotency, and branch-continuation evidence;
- explicit create-new local recovery files with pre-write envelope validation,
  write/sync failure cleanup evidence, bounded regular-file loading, path-
  redacted errors, and complete persisted restoration continuation;
- separate immutable exact-hash asset-source files with pre-I/O size and
  identity checks, shared create-new/sync/cleanup guarantees, bounded
  regular-file loading, and explicit restart rehydration evidence;
- public-repository safeguards, threat model, failure/recovery matrix,
  controlled compatibility/performance baseline, and source-first candidate
  checklist.

### Changed

- `AssetVertex` now requires a public unit-normal field and
  `AssetUploadJob::byte_len` accounts 24 rather than 12 bytes per expanded
  vertex. This is a source-breaking Rust API and capacity-planning change in
  the still-unpublished `0.0.0` workspace; no version or release action was
  taken.

### Known limitations

- no supported release, stable crates.io API, remote transport, authentication,
  automatic persistence/startup, snapshot retention, shared memory, model
  integration, deployment, or production SLA;
- the validated full-runtime profile is currently Windows 11 x86_64 with a
  Vulkan discrete GPU; other runtime platforms/backends remain unverified;
- renderer materials use flat base color; normal output is quantized and the
  imported subset has no normal maps or tangent space; the GLB
  subset excludes textures, external buffers, compression, scene traversal,
  and most vertex attributes. Built-in geometry supports cuboids and fixed
  centered XY planes; spheres, subdivisions, thickness, UVs, and two-sided
  normal policy remain unsupported;
- recovery points and recovery files do not include queued commands,
  observations, asset bytes, or residency; exact-hash asset sources can be
  stored only as separate caller-mapped files. Files are create-new only and
  provide no automatic startup, overwrite, directory-sync guarantee,
  encryption, authentication, or key management; there is no asset
  discovery/catalog, retention/eviction, bundle, or automatic rehydration.
  In-place revert requires drained transient work, clears asset residency, and
  supplies no automatic rollback or freshness policy;
  future frame identity across concurrently live branches is
  caller-coordinated; device-loss recreation, crash-atomic latest pointers,
  binary packaging, signing, provenance, and automated release publication are
  not implemented.
