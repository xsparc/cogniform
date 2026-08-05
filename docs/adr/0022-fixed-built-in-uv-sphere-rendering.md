# ADR 0022: Fixed built-in UV-sphere rendering

- Status: Accepted
- Date: 2026-08-05
- Task: CF022
- Refines: the deferred sphere path in [ADR 0021](0021-centered-built-in-plane-rendering.md)

## Context

The public protocol includes cuboid, plane, and sphere shapes. CF021 made the
first two exact renderer capabilities while retaining sphere records and
rejecting their direct or fallback submission. Supporting the final public
built-in shape requires an explicit local-space convention, bounded topology,
winding, normal source, allocation boundary, and dimension interpretation.

Configurable subdivisions or an indexed mesh would introduce new capacity and
pipeline policy. Generating triangles during frame preparation would also make
render work depend on scene contents. Neither is needed for a stable baseline.

## Decision

`PrimitiveShape::Sphere` is one centered unit-diameter UV sphere with a
positive-Z polar axis. Its existing positive XYZ dimensions are bounding
diameters, consistent with scaling the centered unit cuboid and plane. World
and hierarchy transforms continue to compose before rendering.

The mesh has 16 longitude sectors and 8 latitude bands. Bottom and top fans
and the intervening quads expand to 224 non-degenerate outward-facing,
counter-clockwise triangles and 672 vertices. Each vertex stores its local
position and unit radial normal in the existing 24-byte layout. This creates
one exact 16,128-byte immutable payload during renderer initialization. Frame
preparation performs no tessellation or sphere upload.

The fixed directions use finite `f32` trigonometry. Sphere pixels therefore
follow the existing tolerant visual observation contract; they do not enter
logical scene hashing, replay, protocol serialization, or stable identity.
There is no UV attribute despite the topology name.

Scene preparation selects the exact public built-in shape. A resident asset
still takes precedence over its primitive fallback; when the asset is
unavailable, a sphere fallback selects this sphere mesh. The public
`RendererError::UnsupportedPrimitive` variant remains for source compatibility
and future enum evolution, although every current `PrimitiveShape` now has
built-in geometry.

## Consequences

- Cuboids, planes, and spheres share the same bounded submission, model,
  targets, stable-ID mapping, and observation path.
- Renderer initialization owns one additional fixed 16,128-byte payload;
  scene and frame work remain independent of tessellation complexity.
- Non-uniform dimensions produce an ellipsoid whose smooth normals use the
  existing inverse-transpose transform.
- Configurable tessellation, index buffers, UV attributes, tangents, textures,
  normal maps, LOD, collision, lighting, culling, and batching remain separate
  decisions.
- The protocol schema, asset format, recovery state, replay, and observation
  payload formats do not change.

## Status

Accepted and implemented by CF022. CPU tests cover exact topology, bytes,
radius, winding, radial normals, direct and fallback selection, all-axis
dimension scaling, and resident-asset precedence. A controlled Vulkan-profile
test covers sphere color, curved depth, stable identity, background, and
smooth world-space normal output.
