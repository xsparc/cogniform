# ADR 0023: Bounded directional diffuse lighting

- Status: Accepted
- Date: 2026-08-05
- Task: CF023
- Refines: the unlit material path in [ADR 0005](0005-bounded-headless-wgpu-baseline.md)

## Context

The public scene contract already carries directional and point lights, and
render extraction already retains both kinds, but the renderer has so far
emitted material base color without lighting. Making directional lights
visible requires one explicit axis convention, combination rule, capacity,
uniform layout, and compatibility rule for scenes that have no active
directional light.

An unbounded light collection, a configurable GPU layout, or a wider PBR model
would add policy and resource surfaces that are unnecessary for this first
coherent lighting slice. Point attenuation also needs different position and
range decisions and remains independent.

## Decision

A directional light emits along its transformed local negative-Z axis. Its
transformed local positive-Z axis is therefore the normalized direction from a
shaded surface toward the source. An active light with a degenerate positive-Z
axis is rejected before GPU submission.

The renderer accepts at most four directional-light definitions in one scene,
ordered by stable entity ID. The definition count includes zero-intensity
lights so capacity does not depend on transient activation. Zero-intensity
definitions do not enter the GPU active-light array. A fifth definition is a
typed capacity error before command encoding. Point-light records remain
retained by extraction but visually inactive.

When at least one directional light is active, the RGB factor for a fragment
is the component-wise clamped sum of
`color * intensity * max(dot(world_normal, surface_to_light), 0)`. The factor
and sum are each bounded to the unit interval. Material base RGB is multiplied
by that factor and alpha is unchanged. When no directional light is active,
the factor is exactly one, preserving the existing unlit base-color output,
including point-only scenes.

Every draw uses one fixed 304-byte uniform: model, view-projection, material
color, compact entity identity, active directional-light count, and four
32-byte direction/color-intensity slots. Unused slots are zeroed. This extends
the existing bind group and pipeline rather than adding a light buffer,
pipeline variant, dependency, or observation format.

## Consequences

- Directional lights have a deterministic visible baseline across built-in
  and resident asset geometry through the existing world-normal path.
- Per-frame directional work and per-draw uniform storage are fixed and
  rejected explicitly when definitions exceed four.
- Zero-active and point-only scenes preserve the prior exact color path.
- Point attenuation, ambient light, emissive response, metallic/roughness,
  specular/PBR/IBL, shadows, HDR/tone mapping, gamma conversion, textures,
  normal maps, light configuration, culling, and clustering remain separate
  decisions.
- Protocol schema, logical hashing, replay, recovery, assets, and observation
  payload formats do not change.

## Status

Accepted and implemented by CF023. CPU tests cover stable ordering, normalized
direction, point and zero-intensity inactivity, the fifth-definition error,
degenerate active directions, and the exact padded uniform layout. Controlled
adapter tests cover front-facing half-intensity and back-facing black output
while preserving stable identity, depth, normals, background, and every prior
unlit renderer regression.
