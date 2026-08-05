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
  strict GLB geometry, color/depth/entity-ID readback, quantized flat
  world-space normal observations, structured visibility, and guarded
  background GPU retirement;
- deterministic primitive imagination compilation and pure seeded built-in
  cuboid-grid procedures;
- local-service execution of pure bounded procedures through ordinary patch
  admission, idempotency, processing, query, replay, and restoration;
- bounded content-addressed GLB admission, CPU decode, renderer upload, and
  explicit unsupported/proxy policy;
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
- public-repository safeguards, threat model, failure/recovery matrix,
  controlled compatibility/performance baseline, and source-first candidate
  checklist.

### Known limitations

- no supported release, stable crates.io API, remote transport, authentication,
  persistence composition, shared memory, model integration, deployment, or
  production SLA;
- the validated full-runtime profile is currently Windows 11 x86_64 with a
  Vulkan discrete GPU; other runtime platforms/backends remain unverified;
- renderer materials use flat base color; normal output is flat and quantized,
  with no smooth imported normals, normal maps, or tangent space; the GLB
  subset excludes textures, external buffers, compression, scene traversal,
  and most vertex attributes;
- recovery points do not include queued commands, observations, asset bytes or
  residency, storage, or automatic startup; their envelopes are not encrypted
  or authenticated; in-place revert requires drained transient work, clears
  asset residency, and supplies no automatic rollback or freshness policy;
  future frame identity across concurrently live branches is
  caller-coordinated; device-loss recreation, durable recovery,
  binary packaging, signing, provenance, and automated release publication are
  not implemented.
