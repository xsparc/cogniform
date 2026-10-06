# ADR 0084: Compile bounded glTF scenes into ordinary stable entities

- Status: Accepted
- Date: 2026-10-06
- Task: CF085

## Context

Cogniform's strict GLB importer retains immutable meshes and materials, while
the authoritative world stores only stable entity identities, hierarchy,
transforms, and content-hash-plus-mesh references. The importer currently
classifies every glTF `scene`, `node`, and root `scene` selection as an
unsupported feature. Callers can instantiate one retained mesh at several
ordinary world entities, but there is no bounded way to preserve an authored
scene hierarchy or map source nodes to stable public identities.

The accepted F01 direction calls for one bounded default glTF scene with node
hierarchy, positive finite transforms, repeated mesh references, and
deterministic source-node identity. This is consequential because an imported
scene could otherwise introduce a second mutation path, renderer-owned
identities, unbounded graph work, or logical state that cannot be replayed.

The current Khronos glTF 2.0.1 specification defines a scene as root nodes to
render, requires the complete node hierarchy to be a set of disjoint strict
trees, composes local transforms in `T * R * S` order, and permits the same
mesh to be referenced by several nodes with different transforms
([glTF 2.0 specification](https://registry.khronos.org/glTF/specs/2.0/glTF-2.0.html)).
It also makes the determinant of a node's global transform control triangle
winding. The official
[Multiple Scenes](https://github.com/KhronosGroup/glTF-Sample-Assets/tree/main/Models/MultipleScenes)
and
[Negative Scale Test](https://github.com/KhronosGroup/glTF-Sample-Assets/tree/main/Models/NegativeScaleTest)
assets provide focused selection and winding evidence.

Three ownership models were considered:

1. let the renderer retain and traverse an asset scene graph, synthesizing
   virtual identities for observations;
2. add one opaque asset-scene component to the world and resolve its hidden
   children during extraction; or
3. retain an immutable bounded scene blueprint with the decoded asset, then
   compile one explicit instance into an ordinary atomic `ScenePatch` whose
   entities, transforms, hierarchy, and mesh references are all public world
   state.

The first two models would let logical children exist outside the world's
stable-ID index, patch preflight, canonical hash, replay, ownership, and query
contracts. They would also make renderer or asset residency determine public
identity. The third model keeps parsing in the asset domain and all mutation
in the existing world gateway.

## Decision

Use a pure default-scene-to-patch compiler. Asset decoding may retain one
immutable scene blueprint beside the existing decoded meshes, but neither the
blueprint nor renderer residency is authoritative scene state. A caller must
explicitly request compilation, inspect the resulting source-node identity
map if needed, and submit the resulting ordinary patch through the existing
gateway. There is no implicit apply, import-time mutation, renderer traversal,
or alternate replay event.

`cogniform-assets` owns and exposes the immutable blueprint because it already
owns exact-hash decoded asset meaning and lifecycle. `cogniform-procedural`
owns pure patch compilation and gains a one-way dependency on the public
blueprint type; it already owns deterministic built-ins, patch-budget checks,
and stable-ID derivation. Keeping the compiler in `cogniform-assets` was
rejected because ingestion would then own transaction and delivery semantics;
a new compiler package was rejected as disproportionate for one bounded
operation. `cogniform-world` remains unaware of asset graphs, and
`cogniform-engine` later retrieves the blueprint and composes the operation
without becoming the source of identity or graph semantics.

The first supported scene subset has these boundaries:

- a root `scene` index is required and selects exactly one default scene for
  instantiation; every declared scene and node is structurally validated, but
  only the selected scene's reachable closure is retained for compilation;
- defaults admit at most 16 scene records, 256 node records, 256 reachable
  nodes or mesh instances, hierarchy depth 64, and 256 UTF-8 bytes for each
  optional scene or node name. The strict-tree invariant bounds aggregate
  child edges to fewer than the node count. Embedders may choose smaller
  positive limits;
- root references and child references must be in range and unique where glTF
  requires uniqueness. The complete node collection must be acyclic and each
  node must have at most one parent before any blueprint is retained. Traversal
  uses an explicit bounded worklist rather than source-controlled recursion;
- a node may contain children, one already-supported mesh index, an optional
  bounded name, and finite translation/rotation/scale values. Omitted TRS
  fields use the glTF defaults. Rotation must be a unit quaternion under a
  fixture-pinned tolerance and is deterministically normalized once in f64
  before its f32 patch value is created. Scale must be finite and strictly
  positive on every axis so it maps exactly to the current public
  `LocalTransform` contract;
- node matrices, zero or negative scale, cameras, skins, morph weights,
  animations, lights, node extensions, and node extras remain unsupported.
  Matrix decomposition and signed-determinant winding cannot silently enter
  the positive-TRS implementation;
- scene and node names are untrusted presentation metadata, not identity or
  authority. A bounded valid string may be accepted but is discarded by the
  first subset; and
- every referenced mesh must already satisfy the complete accepted mesh,
  primitive, material, texture, and extension subset. One node may reference
  one mesh, and many nodes may reference the same mesh. The existing
  one-primitive-per-mesh boundary remains unchanged.

Every retained scene record, node record, child index, root index, transform,
and mesh reference contributes to exact checked decoded-byte accounting. The
complete asset, including its meshes, textures, and blueprint, must fit both
`max_asset_decoded_bytes` and aggregate `max_resident_cpu_bytes` before the
record becomes `Ready`. A limit or arithmetic failure rejects the complete
asset without partial blueprint adoption.

One explicit non-zero scene-instance identity namespaces each compilation.
The complete request also carries transaction and idempotency identities, an
exact base revision, delivery semantics, an ordinary patch budget, and active
runtime limits. It has no world, renderer, filesystem, network, clock, or
entropy access and always emits `RequireExactBase` conflict policy. For every
reachable source node, the compiler derives a `StableEntityId` from a domain
separator, the exact content hash, selected scene index, instance identity,
source node index, and a bounded collision-attempt counter. The digest
algorithm and byte order are versioned and fixture-pinned before the compiler
is exposed. Derivation resolves only impossible zero values or within-output
collisions; it never reads the world or probes around a live entity collision.
A collision with existing world state rejects the complete ordinary patch,
and the caller may choose another instance identity.

Compilation is deterministic and bounded before output allocation. It emits:

1. one `Create` operation per reachable node in ascending source-node index,
   containing a `LocalTransform` and, when present, the existing
   `AssetMeshComponent { content_hash, mesh_index }`; then
2. one `Reparent` operation for every non-root node in ascending child
   source-node index.

The artifact also returns `(source_node_index, stable_entity_id)` pairs in
ascending source-node order. Patch operation, component, decoded-byte, and
entity limits must admit the complete output before it is returned. Ordinary
preflight then proves base revision, idempotency, identity availability,
hierarchy, and atomic application. Partial scene creation is never a success
mode.

Compilation requires a `Ready` exact-hash asset with a retained supported
default-scene blueprint. `ProxyReady`, queued, rejected, evicted, or legacy
mesh-only assets have no compilable scene. Upload remains independent: every
referenced mesh must be explicitly prepared and uploaded before a dependent
frame can render, and an unavailable mesh produces the existing typed render
failure without changing the world.

Once applied, imported nodes are ordinary authoritative entities. Their
stable IDs, local transforms, hierarchy, and mesh components participate in
the existing canonical logical hash, replay, queries, extraction, ownership,
and observations. Content-hash-wide eviction still leaves logical entities,
revision, and replay untouched. Fresh recovery restores those logical
references and requires the same exact source plus explicit import and upload
to render again.

F01 is delivered in dependency order rather than as one cross-domain patch:

1. decode, validate, bound, account for, and expose the immutable selected
   scene blueprint without changing world or renderer behavior;
2. compile one blueprint instance into an ordinary bounded patch and stable
   source-node identity map without applying it; and
3. compose explicit local-service compilation/application and demonstrate
   repeated-mesh hierarchy rendering, identity observations, replay,
   eviction, and exact-hash rehydration.

## Consequences

- Authored hierarchy becomes ordinary reviewable world state instead of
  hidden renderer or asset state.
- The same source and instance identity produce the same ordered patch and
  source-node identity map for the declared engine build and schema.
- Repeated nodes reuse immutable mesh and texture residency while retaining
  separate stable entity identities and entity-ID observation values.
- Parsing and graph validation remain caller-driven CPU asset work; patch
  compilation remains pure; mutation and GPU upload keep their existing
  explicit scheduling boundaries.
- A valid glTF with no selected default scene, a matrix transform, signed or
  zero scale, or another excluded node feature remains outside the first
  subset. It is not partially flattened or silently approximated.
- The current implementation continues to reject nodes and scenes until the
  staged code slices are delivered. This record does not change a protocol,
  dependency, runtime, renderer, persistence format, release, or deployment.
- Material variants, imported visibility, GPU instancing extensions,
  animation, spatial selection, semantic metadata, camera import, multiple
  primitive meshes, and interactivity remain separate approved directions
  with their own prerequisites and acceptance evidence.
