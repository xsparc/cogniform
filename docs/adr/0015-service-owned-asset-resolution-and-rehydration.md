# ADR 0015: Service-owned asset resolution and explicit rehydration

- Status: Accepted
- Date: 2026-08-04
- Task: CF015

## Context

CF007 established exact content-addressed source admission, bounded caller-driven
GLB decoding, immutable upload jobs, logical world references, and
renderer-owned GPU residency. CF008 then composed the recorded world, renderer,
observations, and command gateway into `LocalService`, but embedders still had
to keep an unrelated `AssetStore` and reach a lower-level renderer API to make
an `AssetMeshComponent` drawable.

Complete and historical recovery deliberately reconstruct logical state from
replay. Source bytes, decoded CPU meshes, pending work, and GPU resources are
not replay events, so silently retaining or reconstructing them would blur the
recovery and ownership boundaries.

## Decision

`LocalServiceConfig` includes an explicit `AssetStoreConfig`, and each
`LocalService` owns one bounded `AssetStore`. The service exposes exact-hash
source admission, one-item import processing, immutable record lookup, ready
mesh upload admission, one-item GPU upload processing, and aggregate CPU/GPU
asset status. No patch, frame, observation, initialization, or restoration call
implicitly decodes or uploads an asset.

`CogniformEngine` forwards only an immutable `AssetUploadJob` into the
renderer-owned queue and returns typed admission, outcome, and aggregate
residency values. It does not expose mutable renderer state, a device, queue,
buffer, source bytes, or an asset-store handle.

Fresh construction and recovery both create an empty asset store and an empty
renderer residency set. Replay continues to preserve the logical
`AssetMeshComponent` content hash and mesh index. Observation submission for a
referenced mesh without residency fails with the existing typed
`AssetUnavailable` error. A caller re-establishes availability by supplying
bytes matching the retained hash and explicitly driving import and upload;
this changes no world revision, logical hash, or replay bytes.

## Consequences

- The local service can resolve checked GLB bytes through rendering without an
  external store or mutable renderer access.
- CPU decode and GPU upload remain bounded, caller-scheduled, and absent from
  world/render critical paths.
- Hash mismatch consumes no asset capacity, and renderer capacity is reserved
  before upload allocation.
- Logical recovery cannot substitute different bytes for a retained reference;
  rehydration must pass exact content-hash verification.
- Recovery does not include source bytes, decoded meshes, upload queues, or GPU
  residency. Filesystem/network fetching, durable asset caches, automatic
  startup, eviction, retry, device recreation, and in-place revert remain
  separate approved work.
