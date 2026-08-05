# ADR 0025: Outward built-in cuboid winding

- Status: Accepted
- Date: 2026-08-06
- Task: CF025
- Refines: the built-in cuboid normal source in
  [ADR 0020](0020-bounded-imported-vertex-normals.md) and the canonical response
  recorded by [ADR 0024](0024-bounded-point-diffuse-lighting.md)

## Context

The fixed cuboid introduced with the headless renderer remained visible because
the baseline pipeline does not cull back faces. Its 12 triangles nevertheless
wound toward the box interior. Once winding-derived normals became both a
machine-readable observation and a diffuse-lighting input, every cuboid face
reported the opposite of its exterior direction. CF024 exposed the defect in
the canonical scenario: a Point source in front of the table produced black
because the visible face carried an inward normal.

Three corrections were considered:

1. retain the geometry and document inward normals;
2. flip cuboid normals in the shader or according to the viewed side; or
3. reverse the fixed cuboid triangles at their source.

The first option would preserve an invalid exterior-surface contract. The
second would add shape- or view-dependent policy to the shared normal path and
could alter planes or imported meshes. Correcting the one faulty built-in
position list preserves the existing generic vertex, transform, shader, and
observation boundaries.

## Decision

The built-in cuboid is a centered unit box whose coordinates remain exactly
`-0.5` or `0.5`. Each of its six faces contains two non-degenerate triangles
that wind counter-clockwise when viewed from outside. Their synthesized flat
normals are the exact axis-aligned directions positive and negative X, Y, and
Z, with two triangles per direction.

The immutable payload remains 36 expanded vertices in the existing 24-byte
position-plus-normal layout, exactly 864 bytes, created once during renderer
initialization. Face diagonals, positions, extents, model transforms, geometry
selection, draw calls, pipeline, attachments, and the no-culling policy do not
change. Position-only imported triangles continue to follow their own source
winding; Cogniform does not rewrite asset orientation.

Cuboid normal observations and Lambert response intentionally adopt the
corrected exterior directions. The fixed reference projection sees the near
negative-Z face and therefore reports negative Z. The canonical camera sees
the table's positive-Z exterior; its existing Point source now produces a
positive color, measured as `[175, 93, 33, 255]` on the validated Vulkan
profile and checked with the existing two-unit-per-channel RGBA8 tolerance.

## Consequences

- Machine clients receive exterior rather than interior directions for every
  built-in cuboid face.
- Lit cuboids can change color where callers previously observed the inverted
  Lambert result. Unlit color, depth, coverage, identity, background, and
  logical scene/replay state remain unchanged.
- The correction is observable to callers that depended on the faulty normal
  direction. The workspace remains unpublished at `0.0.0`, so CF025 records
  the compatibility change without a version, tag, or release action.
- Plane, sphere, imported-normal, position-only asset, culling, two-sided
  lighting, material, protocol, persistence, transport, and dependency
  contracts do not change.

## Status

Accepted and implemented by CF025. CPU tests pin the exact topology, layout,
face counts, non-degeneracy, axis directions, and outward winding. Controlled
adapter tests pin the reference near-face direction and positive canonical
Point-light response while retaining the wider renderer and engine regressions.
