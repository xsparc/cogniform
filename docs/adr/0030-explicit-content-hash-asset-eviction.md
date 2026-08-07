# ADR 0030: Explicit content-hash asset eviction

- Status: Accepted
- Date: 2026-08-07
- Task: CF030
- Refines: [ADR 0008](0008-content-addressed-assets-and-pure-built-in-procedures.md)
- Follows: [ADR 0029](0029-bounded-embedded-png-base-color-textures.md)

## Context

Asset admission, decoded CPU state, renderer upload work, resident meshes, and
shared textures all have fixed capacities. Before CF030, those bounds prevented
unbounded growth but callers could reclaim capacity only by dropping the whole
store or renderer. Long-lived local services therefore could not deliberately
replace an unused, rejected, queued, or resident asset while preserving the
authoritative world and unrelated assets.

Automatic LRU or reference-counted eviction would require hidden policy, world
reference scans, and synchronization between the world, service, and renderer.
Per-mesh eviction would leave ambiguous ownership for a source-wide shared
texture. A whole-source operation matches the existing content-addressed
identity and keeps policy with the trusted local caller.

## Decision

`AssetStore::evict` removes all CPU-domain state for one `ContentHash`: its
lifecycle record, any queued source, decoded meshes, and shared decoded
texture. `AssetStoreEviction` reports the previous state plus exact removed
queue, source-byte, decoded-byte, and mesh counts. Unrelated queued imports keep
FIFO order.

`HeadlessRenderer::evict_asset` removes every pending upload and resident mesh
for the same content hash. It releases the source's unique pending or resident
texture exactly once and reports all removed counts and bytes through
`RendererAssetEviction`. Unrelated upload jobs preserve FIFO order. Dropping
renderer ownership is immediate, but a GPU backend may retain physical
resources until already-submitted command buffers finish; an owned
`PendingFrame` remains readable after eviction.

`CogniformEngine` forwards only the renderer operation. `LocalService::evict_asset`
performs renderer eviction and then store eviction, returning both exact
effects in `LocalAssetEvictionOutcome`. The operation is synchronous,
caller-driven, allocation-free apart from collection maintenance, performs no
decode/upload/frame/file/network work, and is idempotent: a hash absent from
both domains returns an all-zero already-absent outcome.

Eviction never scans or mutates the authoritative world. Existing
`AssetMeshComponent` values, scene revision, logical hash, replay bytes, frame
frontier, recovery values, and separately persisted exact-hash source files
remain unchanged. A later draw follows the existing authored primitive
fallback or returns `AssetUnavailable`. Supplying the same exact source and
explicitly driving import and upload restores availability without a world
mutation.

## Consequences

- Callers can reclaim every bounded asset capacity without replacing the
  service or disturbing unrelated work.
- Rejected records can be removed and retried, so an operator must not
  spin-retry adversarial bytes; retry rate and policy remain outside the local
  library boundary.
- Logical references may deliberately outlive residency, matching restoration
  semantics. Eviction is not entity deletion or an asset-reference edit.
- There is no per-mesh, automatic, LRU, reference-counted, background, or
  pressure-triggered policy. No catalog, storage-file deletion, automatic
  rehydration, or device recreation is introduced.
- The additive Rust API changes no canonical schema or dependency and the
  unpublished `0.0.0` workspace receives no version or release action.
