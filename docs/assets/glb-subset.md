# Content-addressed GLB assets

Status: immutable asset records, the approved GLB subset, world references,
and bounded renderer uploads are implemented by CF007. CF015 composes those
steps into the local typed service without making them implicit. CF019 adds an
independent immutable bounded file for retaining one exact source across a
restart; import and upload remain explicit.

## Ownership and lifecycle

An asset is identified by the SHA-256 digest of its exact source bytes. The
canonical `ContentHash` representation is 64 lowercase hexadecimal characters.
Callers provide both the expected digest and the bytes to `AssetStore::enqueue`;
a mismatch fails before the store consumes record or queue capacity.

Processing is deliberately split into explicit steps:

```text
exact GLB bytes + expected hash
  -> verify and reserve bounded source capacity
  -> Queued record
  -> caller invokes AssetStore::process_next
  -> Ready, ProxyReady, or Rejected record
  -> immutable AssetUploadJob for (content hash, mesh index)
  -> reserve renderer upload and final residency capacity
  -> caller invokes HeadlessRenderer::process_next_asset_upload
  -> immutable GPU-resident mesh
```

Neither admission nor world mutation decodes an asset. Frame submission never
decodes source bytes or processes an upload. An owner should schedule
`AssetStore::process_next` on a CPU service worker and call
`process_next_asset_upload` only on the renderer domain. This library baseline
does not create workers or make those calls implicitly.

`LocalService` is now the standard in-process owner. Its
`enqueue_asset_source`, `process_next_asset_import`, `asset_record`,
`enqueue_asset_upload`, and `process_next_asset_upload` methods preserve the
same split lifecycle, while `asset_status` returns only aggregate store and
renderer counters. The engine forwards immutable upload jobs and never exposes
mutable renderer state or backend handles. The lower-level store and renderer
APIs remain available for embedders that own those domains directly.

Records are retained as `Queued`, `Ready`, `ProxyReady`, or `Rejected`. The
original source is retained only while queued. Ready and proxy records retain
expanded triangle positions for upload. There is no eviction API in this
baseline; dropping the store or renderer releases its respective residency.

The authoritative world stores only `AssetMeshComponent`, containing the
content hash and zero-based mesh index. That component participates in logical
snapshots, hashing, replay, and render extraction. It contains no source bytes,
CPU mesh ownership, ECS handle, or GPU handle.

Fresh and restored local services start with empty source/import state and GPU
residency. Replay still restores the logical content hash and mesh index. A
frame that has no resident mesh and no explicit primitive fallback returns
`AssetUnavailable`; the caller must supply exact matching bytes and explicitly
drive import and upload. Rehydration does not mutate the world or append replay.

`cogniform-storage::AssetFileStore` can create and later load one separate
exact-hash source file under a caller-selected bound. It does not retain the
source inside `AssetStore`, associate it with a recovery point, discover a path
from a hash, decode the GLB, or schedule import/upload. See the
[asset-file guide](../persistence/asset-files.md).

## Approved GLB subset

The importer accepts only the following baseline:

- GLB version 2 with an exact declared file length;
- exactly two four-byte-aligned chunks: JSON first, then BIN;
- one embedded buffer with no URI;
- one or more meshes, with exactly one primitive per mesh;
- triangle-list mode, either explicit mode `4` or the glTF default;
- exactly one vertex attribute, `POSITION`;
- finite non-normalized f32 `VEC3` positions;
- optional non-normalized scalar u16 or u32 indices;
- tightly packed or valid component-aligned buffer-view strides up to 252
  bytes; and
- an optional material `pbrMetallicRoughness.baseColorFactor`. Metallic and
  roughness factors are range-validated but are not rendered yet.

Indexed geometry is expanded into a triangle vertex stream. Every referenced
range and index is checked before use. A primitive without indices must contain
a multiple of three positions; an indexed primitive must contain a multiple of
three indices.

The strict schema rejects unknown fields after recognized unsupported feature
declarations are classified. External buffers, data URIs, additional GLB
chunks, sparse accessors, normalized accessors, imported normals, UVs, tangents,
colors, morph targets, multiple primitives, non-triangle modes, images,
samplers, textures, nodes, scenes, cameras, animations, skins, and extensions
are not supported. There is no compressed geometry or texture decompression
path.

## Failure and proxy policy

Import diagnostics contain a stable code, a static schema/import location, and
an optional collection index. They never contain source text, filenames,
parser payloads, or unbounded backend messages.

`UnsupportedAssetPolicy::Reject` is the default. With the explicit
`ProxyCuboid` policy, only these otherwise-valid classifications may become a
magenta unit-cube `ProxyReady` record:

- unsupported extension;
- unsupported feature;
- unsupported accessor; or
- unsupported primitive mode.

Invalid GLB framing or lengths, malformed or type-invalid JSON, invalid buffer
ranges or indices, non-finite positions, and collection or byte-limit failures
always produce `Rejected`. A proxy therefore never masks malformed or
over-limit input.

At draw time, a resident mesh uses its imported base color unless the world
entity has an explicit material, which overrides it. If the referenced mesh is
not resident, an explicit primitive component is used as the author-chosen
fallback. Without that component, preparation fails with `AssetUnavailable`.

## Default bounds

`AssetLimits::default` applies these limits before or during CPU processing:

| Resource | Default |
|---|---:|
| Source bytes per asset | 16 MiB |
| Aggregate queued source bytes | 32 MiB |
| JSON chunk bytes | 1 MiB |
| BIN chunk bytes | 16 MiB |
| Retained asset records | 256 |
| Pending imports | 64 |
| Meshes per asset | 64 |
| Buffer views, accessors, or materials per asset | 256 each |
| Primitives per mesh | 1 |
| Expanded vertices per mesh | 262,144 |
| Source indices per mesh | 786,432 |
| Decoded bytes per asset | 16 MiB |
| Aggregate resident CPU mesh bytes | 64 MiB |

`RendererConfig::new` independently applies these GPU-side reservations:

| Resource | Default |
|---|---:|
| Pending upload jobs | 64 |
| Aggregate pending upload bytes | 32 MiB |
| Expanded vertices per mesh | 262,144 |
| Vertex bytes per mesh | 16 MiB |
| Resident meshes, including reservations | 256 |
| Resident vertex bytes, including reservations | 64 MiB |

Embedders may choose different non-zero values. Renderer configuration is
rejected if a maximum-size mesh cannot fit in both the pending-byte and
resident-byte limits.

## Validation

The default offline suite verifies exact hash admission, every truncated prefix
of the checked fixture, malformed extension declarations, proxy eligibility,
range and capacity failure, procedure replay, world extraction, and renderer
upload reservation:

```text
cargo test -p cogniform-assets -p cogniform-procedural --locked --offline
cargo test --workspace --all-features --locked --offline
```

The checked text fixture under `tests/assets/triangle.glb.hex` can be exercised
on an approved DX12 or Vulkan adapter:

```text
cargo test -p cogniform-renderer --test asset_fixture --locked --offline -- --ignored --exact approved_glb_fixture_renders_with_identity_color_depth_and_winding_normal
cargo test --release -p cogniform-engine --test service_assets --locked --offline -- --ignored --exact local_service_imports_renders_and_explicitly_rehydrates_one_glb_asset
cargo test --release -p cogniform-storage --test asset_file --locked --offline -- --ignored --exact persisted_recovery_and_asset_sources_restore_renderable_state
```

The controlled tests create no window, perform no network call, and upload no
artifact. They verify exact entity identity plus tolerant imported color and
depth probes, then prove that restored asset references require explicit
exact-hash CPU/GPU rehydration without another logical mutation. The CF019 case
persists recovery and asset source in separate files, drops the source service,
restores the logical reference, observes its exact typed absence, and then
loads/imports/uploads the expected bytes without changing revision, hash, or
replay.
