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
  strict GLB geometry, color/depth/entity-ID readback, structured visibility,
  and guarded background GPU retirement;
- deterministic primitive imagination compilation and pure seeded built-in
  cuboid-grid procedures;
- bounded content-addressed GLB admission, CPU decode, renderer upload, and
  explicit unsupported/proxy policy;
- local typed service and unattended room/table/light/camera scenario;
- public-repository safeguards, threat model, failure/recovery matrix,
  controlled compatibility/performance baseline, and source-first candidate
  checklist.

### Known limitations

- no supported release, stable crates.io API, remote transport, authentication,
  persistence composition, shared memory, model integration, deployment, or
  production SLA;
- the validated full-runtime profile is currently Windows 11 x86_64 with a
  Vulkan discrete GPU; other runtime platforms/backends remain unverified;
- renderer materials use flat base color, normal output is deferred, and the
  GLB subset excludes textures, external buffers, compression, scene traversal,
  and most vertex attributes;
- device-loss restart, durable recovery, binary packaging, signing, provenance,
  and automated release publication are not implemented.
