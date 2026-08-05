# ADR 0021: Centered built-in plane rendering

- Status: Accepted
- Date: 2026-08-05
- Task: CF021
- Refines: the deferred primitive-mesh path in [ADR 0006](0006-coalesced-extraction-and-bounded-observations.md)

## Context

The public protocol has always included cuboid, plane, and sphere shapes, but
the first bounded extracted-scene renderer implemented only cuboids. Plane and
sphere records were retained and rejected at frame preparation with a stable
unsupported-primitive error. The renderer now has a reviewed position-plus-
normal vertex layout, model inverse-transpose normal handling, fixed draw
budgets, and controlled color, depth, identity, and normal probes.

A plane is the smallest remaining built-in geometry path. Its local orientation,
winding, dimension behavior, asset-fallback behavior, and allocation boundary
need to be explicit before callers can rely on it. Sphere tessellation adds
separate resolution, topology, and capacity choices and should not be implied
by enabling planes.

## Decision

`PrimitiveShape::Plane` is one centered unit square in the local XY plane. Its
six expanded vertices form two counter-clockwise triangles when viewed from
positive Z, and every source vertex carries the unit normal `[0, 0, 1]`. The
plane uses the existing 24-byte position-plus-normal layout and one immutable
six-vertex buffer created with the renderer. It has no thickness, subdivision,
index buffer, UV, tangent, or separate pipeline.

The existing positive XYZ primitive dimensions scale the three model columns.
X and Y therefore set the visible extents; local positions remain at Z = 0,
while Z remains part of the complete model and inverse-transpose normal
contract. World and hierarchy transforms continue to compose before rendering.
The baseline pipeline does not cull either triangle side, but both sides retain
the same transformed source normal; two-sided lighting or normal flipping is
not introduced.

Scene preparation selects the exact built-in geometry named by the primitive.
When an entity also names an asset mesh, a resident asset still takes
precedence. If that asset is unavailable, the explicit primitive is the proxy:
a plane falls back to plane geometry rather than cuboid geometry. A sphere
remains `UnsupportedPrimitive` when it is the direct geometry or the required
fallback.

## Consequences

- Plane entities now render through the same bounded submission, targets,
  readback pool, stable-ID mapping, and observation envelopes as cuboids and
  approved assets.
- Renderer initialization owns one additional fixed 144-byte vertex payload;
  frame preparation adds no plane tessellation, upload job, or unbounded work.
- Existing cuboid and resident-asset behavior remains unchanged. Callers that
  previously expected a plane submission to fail now receive rendered output;
  the protocol type and serialized schema do not change.
- Sphere topology, UVs, tangents, textures, material lighting, collision,
  thickness, and configurable plane subdivisions remain separate decisions.

## Status

Accepted and implemented by CF021. CPU tests cover exact layout, winding,
geometry selection, all-axis dimension scaling, fallback precedence, and the
retained sphere error. A controlled Vulkan-profile test covers plane color,
depth, stable identity, background, and world-space normal output.
