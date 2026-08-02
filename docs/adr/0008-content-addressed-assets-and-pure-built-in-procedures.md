# ADR 0008: Content-addressed assets and pure built-in procedures

- Status: Accepted
- Date: 2026-08-02
- Task: CF007

## Context

Cogniform needs to place imported geometry in authoritative scenes without
moving untrusted source bytes, parser state, or GPU handles into the world
domain. Import work must be separately schedulable from world mutation and
frame submission. Source identity, decoded CPU residency, renderer upload
pressure, and GPU residency all need explicit bounds. The first importer must
also be honest about its supported format instead of accepting a broad glTF
surface that has not been hardened.

Built-in scene procedures have a related boundary problem. They should reduce
repetitive intent to ordinary atomic patches, but they must not gain privileged
world access, ambient randomness, filesystem or network access, or a second
mutation path.

Three asset layouts were considered:

1. decode assets synchronously while applying a scene patch;
2. let the renderer accept GLB source bytes and decode them before drawing; or
3. keep a content-addressed CPU asset store separate from the world and pass
   immutable, bounded upload jobs to the renderer.

The first two options put attacker-controlled parsing or allocation on a
critical domain. The third keeps source verification, CPU decoding, world
references, and GPU upload independently bounded and caller-driven.

## Decision

`cogniform-protocol` owns a canonical lowercase SHA-256 `ContentHash` and the
logical `AssetMeshComponent { content_hash, mesh_index }`. The authoritative
world stores that value like any other component, includes it in snapshots and
the version-one logical hash, and extracts only the immutable reference. It
does not retain asset bytes, parser objects, or backend resources.

`cogniform-assets` owns exact source-hash admission, retained lifecycle records,
strict GLB decoding, decoded CPU meshes, and immutable renderer upload jobs.
Admission checks the caller-supplied hash over the exact bytes and reserves
both record and pending-byte capacity before retaining source. `process_next`
decodes at most one queued source and then releases those source bytes. Records
remain in one of `Queued`, `Ready`, `ProxyReady`, or `Rejected`; diagnostics use
stable classifications and static locations rather than parser input or
unbounded error strings.

The initial format is GLB 2.0 with exactly one JSON chunk followed by one BIN
chunk. It permits one embedded buffer, triangle-list meshes with one primitive
and a `POSITION` accessor, optional unsigned 16-bit or 32-bit indices, bounded
interleaving, and optional PBR base-color factors. The complete subset and
default limits are documented in [the GLB asset guide](../assets/glb-subset.md).
There is no decompression path. Images, textures, scenes, nodes, animations,
skins, cameras, external buffers, extra vertex attributes, and every extension
are unsupported.

Unsupported but otherwise valid features produce structured diagnostics. The
default policy rejects them. An embedder may explicitly select a conspicuous
magenta unit-cube proxy only for unsupported extension, feature, accessor, or
primitive-mode classifications. Malformed framing or JSON, invalid ranges or
indices, non-finite vertices, and exceeded limits always reject and can never
be converted into a proxy.

The renderer admits immutable expanded-triangle upload jobs only after
reserving pending count, pending bytes, final mesh count, and final resident
bytes. `process_next_asset_upload` performs at most one renderer-domain GPU
allocation and is never called by frame submission. A scene uses a resident
asset mesh when available. An explicit primitive component may serve as the
scene-authored fallback while an asset is unavailable; otherwise frame
preparation returns `AssetUnavailable`.

`cogniform-procedural` owns pure built-in procedure execution. The first
procedure emits a row-major cuboid grid. Its parameters, procedure ID, seed,
transaction ID, idempotency key, base revision, delivery semantics, patch
budget, and procedure limits are all explicit inputs. Stable entity IDs are
derived with domain-separated SHA-256 from procedure identity, seed, row-major
index, and a bounded collision attempt. Output is an ordinary validated
`ScenePatch`; only the authoritative world can apply it.

## Consequences

- The same source bytes always have the same asset identity, and a mismatched
  claimed identity fails before a record or queue slot is consumed.
- Parsing is caller-driven CPU work, not world or render work. GPU allocation
  is a second caller-driven step owned by the renderer.
- The baseline expands indexed triangles on the CPU and retains immutable
  decoded meshes. It does not optimize, evict, stream, or deduplicate vertices.
- Asset records and GPU meshes remain resident until their owning store or
  renderer is dropped. Explicit eviction requires a later lifecycle decision.
- Imported geometry uses positions and base color only. Normals, lighting,
  texture sampling, material metallic/roughness rendering, mesh transforms,
  and scene traversal are deferred.
- A repeated procedure request produces exactly the same ordered entity IDs and
  canonical patch bytes. Collision with entities already present in a world is
  still rejected by ordinary atomic patch preflight.
- Procedures are compiled library functions, not Wasm or native plugins. A
  sandboxed third-party procedure model requires a later ADR and approved task.
- The workspace remains unpublished at `0.0.0`. No release compatibility
  promise, remote asset fetch, decompression, or external service is introduced.
- The existing single standard GitHub Actions job is unchanged. Parser corpus,
  limit, procedure, and renderer admission tests run offline; the GPU fixture
  remains an explicit controlled-adapter test.
