# ADR 0071: Scale imported glTF emission with one bounded strength

- Status: Accepted
- Date: 2026-08-24
- Task: CF071

## Context

Core glTF bounds `emissiveFactor` to the unit interval, so brighter authored
surface emission requires the ratified `KHR_materials_emissive_strength`
extension. Cogniform already retains core emissive factors and one embedded
sRGB emissive texture, adds their product after the existing surface response,
and clamps the final linear RGB result to one. It did not recognize the
extension, and therefore proxied otherwise supported assets that use it.

The scalar must cross strict extension preflight, immutable material metadata,
scene-material precedence, the private draw ABI, and shader arithmetic without
adding a light, HDR output, exposure, bloom, another resource, or another
pipeline. The ratified extension also forbids coexistence with
`KHR_materials_unlit` on one material.

## Decision

Recognize `KHR_materials_emissive_strength` only through the existing unique
top-level declaration contract. Its per-material payload must be an object.
The optional `emissiveStrength` value must decode to a finite non-negative f32
and defaults exactly to one. Reject undeclared, null, wrong-type, negative, or
non-finite values without proxy substitution. Validate the supported field
before classifying a well-formed wider payload as unsupported. Reject a
material that carries both an emissive-strength member and a
`KHR_materials_unlit` member, including a wider unlit payload.

Retain the scalar separately in `AssetMaterial`; preserve the public
constructor and give it a default of one. For selected imported metallic-
roughness materials only, multiply the linear emissive factor and sRGB-decoded
emissive texture RGB by the retained strength before the existing final unit
clamp. An explicit scene `MaterialComponent` selects zero imported emission
and a neutral strength of one.

Carry the scalar in the previous padding lane of the camera-position `vec4`.
Keep the private draw uniform exactly 624 bytes, the vertex ABI 72 bytes, the
nine-entry bind group, four texture roles, 36-sampler table, and two pipelines
unchanged.

Do not add cross-surface illumination, HDR output, exposure, tone mapping,
bloom, ambient or image-based lighting, occlusion, new image or texture roles,
alpha blending, node/scene traversal, protocol or world state, persistence,
dependencies, deployment, or release/publication authority.

## Consequences

- Omitted extension data is byte-for-byte compatible at strength one; zero
  strength makes imported emission neutral, and large finite values saturate
  only through the existing final clamp.
- The camera-position padding lane is no longer required to be zero, but the
  fixed uniform size and every GPU resource count remain unchanged.
- Extension declaration, payload, wider-feature precedence, and unlit
  exclusion are enforced for selected and unused material records before any
  proxy can be adopted.
- Logical world state, revision/hash/replay, observations, asset byte
  accounting, eviction, recovery, and exact-hash rehydration are unchanged.

## Status

Accepted and implemented by CF071. Importer tests cover omission, empty
payload, zero, finite override, maximum finite f32, malformed and undeclared
forms, unlit exclusion, wider payloads, and proxy precedence. Renderer tests
protect the 624-byte uniform and scene-material suppression. A controlled
optimized Windows/Vulkan comparison proves factor-only and sRGB-textured
scaling below the clamp, zero/default behavior, saturation, and unchanged
alpha, depth, stable identity, and geometric-normal observations.
