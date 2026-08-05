# ADR 0024: Bounded point diffuse lighting

- Status: Accepted
- Date: 2026-08-05
- Task: CF024
- Refines: [ADR 0023](0023-bounded-directional-diffuse-lighting.md)

## Context

The version-one light component and render extraction already retain point
lights, but ADR 0023 deliberately left them visually inactive while it fixed
the directional convention and base diffuse path. Completing the public light
kind now requires an explicit position source, attenuation rule, zero-distance
behavior, independent capacity, mixed-light combination, and fixed GPU layout.

A configurable range, radius, cutoff, HDR response, or clustered light buffer
would introduce new public policy or resource surfaces. They are unnecessary
for a bounded first point-light contract.

## Decision

A point source position is the finite GPU-f32 translation of its extracted
world transform. The renderer processes definitions in stable entity-ID order
and accepts at most four point definitions per scene, independently of the
four-directional-definition limit. Zero-intensity definitions still count
toward capacity but do not enter the active GPU array. A fifth definition or
an active position outside finite GPU-f32 range fails before GPU submission.

For each active point light, a fragment computes
`to_light = point_position - world_position` and
`distance_squared = dot(to_light, to_light)`. An exact zero distance contributes
zero so direction normalization cannot become non-finite. Otherwise the
surface-to-light direction is normalized and attenuation is
`min(intensity / max(distance_squared, 1e-6), 1)`. The RGB contribution is
`color * attenuation * max(dot(world_normal, surface_to_light), 0)`. If finite
positions still overflow the derived f32 squared distance, its zero inverse
distance also contributes zero before direction multiplication.

Point and directional contributions share one component-wise clamped sum in
the unit interval. Material base RGB is multiplied by the result and alpha is
unchanged. The exact unlit factor of one now applies only when neither light
kind has an active definition. With no active point light, directional output
remains exactly the ADR 0023 path.

The per-draw uniform grows from 304 to exactly 448 bytes. Its first 304 bytes
remain the ADR 0023 model, camera, material, identity, directional count, and
four directional slots. An appended point count and four zero-padded 32-byte
position/color-intensity slots complete the layout. The existing bind group,
pipeline, observation formats, protocol schema, and logical state remain
unchanged.

## Consequences

- Both public light kinds now produce deterministic bounded diffuse output.
- Point work, definition count, and per-draw storage are fixed; capacity does
  not fluctuate when intensity changes.
- The attenuation floor is only numerical protection. There is no range,
  cutoff, radius, or culling contract, and attenuation is capped at one.
- Exact-zero coincidence is dark rather than undefined or arbitrarily
  directional.
- A finite source whose derived f32 squared distance overflows is also dark,
  matching zero inverse-square attenuation without creating a NaN direction.
- Ambient, emissive, metallic/roughness response, specular/PBR/IBL, shadows,
  HDR/tone mapping, gamma conversion, textures, normal maps, spot lights,
  lighting configuration, culling, and clustering remain separate decisions.
- Protocol, hashing, replay, recovery, assets, and observation payloads do not
  change.

## Status

Accepted and implemented by CF024. CPU tests cover stable ordering, independent
capacity including inactive definitions, finite active positions, and the
exact appended uniform layout. Controlled adapter tests cover unit-distance,
far-distance inverse-square, mixed Directional/Point summation, back-facing,
and finite-input distance-overflow output while preserving identity, depth,
normals, and background. The
canonical point-light scenario records its
existing cuboid winding-normal response without changing scene geometry.
