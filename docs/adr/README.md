# Architecture Decision Records

Architecture decision records capture choices that constrain future Cogniform
work. Accepted records are not silently rewritten; a later record supersedes an
earlier one and links the history.

| ADR | Status | Decision |
|---|---|---|
| [0001](0001-rust-workspace-and-domain-boundaries.md) | Accepted | Establish the Rust workspace and exclusive domain boundaries |
| [0002](0002-bounded-canonical-json-contracts.md) | Accepted | Use bounded canonical JSON contracts with exact-pinned vendored Serde |
| [0003](0003-atomic-world-preflight-and-stable-identity.md) | Accepted | Preflight atomic patches and keep stable identity outside ECS handles |
| [0004](0004-stable-hierarchy-canonical-hash-and-replay-chain.md) | Accepted | Keep hierarchy stable-ID based and replay canonical state through a bounded hash chain |
| [0005](0005-bounded-headless-wgpu-baseline.md) | Accepted | Use an exact-pinned, bounded DX12/Vulkan wgpu core for offscreen reference rendering |
| [0006](0006-coalesced-extraction-and-bounded-observations.md) | Accepted | Drain coalesced render changes into revision-linked frames and bounded observations |
| [0007](0007-pure-imagination-compiler-and-bounded-local-gateway.md) | Accepted | Compile bounded primitive imaginations purely and admit local commands with explicit queue semantics |
| [0008](0008-content-addressed-assets-and-pure-built-in-procedures.md) | Accepted | Separate content-addressed CPU import, logical asset references, bounded GPU upload, and pure built-in procedures |
| [0009](0009-recorded-engine-and-local-typed-service.md) | Accepted | Record engine mutations and expose a bounded local typed service with a canonical unattended scenario |
| [0010](0010-source-first-release-profile.md) | Accepted | Prepare a narrow source-first candidate profile without publishing crates, binaries, or a release |
| [0011](0011-quantized-world-space-normal-observations.md) | Accepted | Add bounded quantized flat world-space normal observations without changing asset contracts |
| [0012](0012-complete-in-memory-service-restoration.md) | Accepted | Restore a fresh bounded local service from complete verified replay and frame-continuity state |
| [0013](0013-versioned-recovery-point-envelope.md) | Accepted | Bind replay and frame-continuity state in one bounded integrity-protected portable envelope |
| [0014](0014-exact-revision-historical-recovery-forks.md) | Accepted | Create fresh-service historical forks from exact retained revisions without source mutation or pre-capture frame reuse |
| [0015](0015-service-owned-asset-resolution-and-rehydration.md) | Accepted | Compose bounded asset resolution into the local service with explicit post-recovery rehydration |
| [0016](0016-service-procedure-composition-through-ordinary-patches.md) | Accepted | Compose pure built-in procedures into the local service through ordinary patch admission |
| [0017](0017-quiescent-live-revert-through-fresh-replacement.md) | Accepted | Revert a quiescent local service through an exact fresh replacement before swap |
| [0018](0018-immutable-bounded-local-recovery-files.md) | Accepted | Persist complete recovery envelopes through explicit create-new bounded local files |
| [0019](0019-immutable-exact-hash-asset-source-files.md) | Accepted | Persist exact-hash asset sources through separate explicit create-new bounded local files |
| [0020](0020-bounded-imported-vertex-normals.md) | Accepted | Import bounded optional vertex normals and preserve flat position-only fallback output |
| [0021](0021-centered-built-in-plane-rendering.md) | Accepted | Render a centered built-in XY plane with explicit winding, dimensions, and fallback semantics |
| [0022](0022-fixed-built-in-uv-sphere-rendering.md) | Accepted | Render a fixed centered unit-diameter UV sphere with bounded topology and radial normals |
| [0023](0023-bounded-directional-diffuse-lighting.md) | Accepted | Render up to four stable-ordered directional lights with bounded diffuse shading and exact unlit compatibility |
| [0024](0024-bounded-point-diffuse-lighting.md) | Accepted | Render up to four stable-ordered point lights with bounded inverse-square diffuse shading |
| [0025](0025-outward-built-in-cuboid-winding.md) | Accepted | Correct the fixed built-in cuboid to outward counter-clockwise winding and exterior normals |
| [0026](0026-bounded-direct-metallic-roughness-response.md) | Accepted | Honor metallic and roughness through one bounded direct-light microfacet response |
| [0027](0027-imported-glb-metallic-roughness-materials.md) | Accepted | Retain bounded GLB material factors through immutable upload and renderer residency |
| [0028](0028-bounded-primary-texture-coordinates.md) | Accepted | Retain one bounded primary GLB texture-coordinate set without changing rendered output |
| [0029](0029-bounded-embedded-png-base-color-textures.md) | Accepted | Decode and explicitly upload one bounded embedded PNG base-color texture |
| [0030](0030-explicit-content-hash-asset-eviction.md) | Accepted | Reclaim all CPU/GPU asset state for one content hash through an explicit caller-driven operation |
| [0031](0031-monotonic-pending-work-age-status.md) | Accepted | Report monotonic oldest-pending age for every caller-driven local-service queue |
| [0032](0032-offline-recovery-file-inspection.md) | Accepted | Inspect one immutable recovery file through the exact CPU restoration preflight without GPU or payload exposure |
| [0033](0033-versioned-recovery-inspection-json.md) | Accepted | Emit a deterministic versioned recovery-inspection JSON report at the CLI boundary |
| [0034](0034-versioned-controlled-measurement-json.md) | Accepted | Emit versioned informational controlled-measurement JSON at the CLI boundary |
| [0035](0035-versioned-canonical-scenario-json.md) | Accepted | Emit a deterministic versioned canonical-scenario proof at the CLI boundary |

New records use four sections: context, decision, consequences, and status.
