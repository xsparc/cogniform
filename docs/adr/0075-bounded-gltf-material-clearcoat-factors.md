# ADR 0075: Layer bounded glTF material clearcoat factors

- Status: Accepted
- Date: 2026-08-26
- Task: CF075
- Refines: [ADR 0074](0074-bounded-gltf-material-specular-textures.md)

## Context

The ratified
[`KHR_materials_clearcoat`](https://github.com/KhronosGroup/glTF/blob/main/extensions/2.0/Khronos/KHR_materials_clearcoat/README.md)
extension adds a thin dielectric layer over the complete material beneath it.
Its numeric `clearcoatFactor` and `clearcoatRoughnessFactor` are finite unit
values with exact defaults of zero under the ratified
[`material clearcoat schema`](https://github.com/KhronosGroup/glTF/blob/main/extensions/2.0/Khronos/KHR_materials_clearcoat/schema/material.KHR_materials_clearcoat.schema.json).
The coat uses a fixed index of refraction
of 1.5, a normal-incidence reflectance of 0.04, and a view-normal Schlick
weight. Emission is below the coat and is attenuated by the same weight.

The extension also defines intensity, roughness, and normal textures. Those
roles require another bounded-resource and tangent-space decision. CF075
therefore admits only the numeric factors while validating the complete
texture-info and root-reference authority before applying the existing
unsupported/proxy policy. The official
[`ClearCoatTest`](https://github.com/KhronosGroup/glTF-Sample-Assets/tree/main/Models/ClearCoatTest)
is used as a behavior reference without copying its asset bytes.

## Decision

Recognize `KHR_materials_clearcoat` only through the strict declaration
contract. Decode `clearcoatFactor` and `clearcoatRoughnessFactor` as finite unit
`f32` values with exact zero defaults. Reject undeclared, null, malformed,
non-finite, negative, above-one, `KHR_materials_unlit`, and
`KHR_materials_pbrSpecularGlossiness` coexistence before adoption. Validate all
three optional texture infos, including finite clearcoat-normal scale,
declared texture transforms, texture indices, root texture/image/sampler
records, and source bytes before returning the explicit unsupported outcome.
Do not silently ignore valid texture authority.

Retain the two factors in `AssetMaterial` without changing decoded or upload
accounting. Explicit scene materials, built-ins, proxies, and unlit materials
retain exact zero clearcoat.

For imported metallic-roughness materials, use the face-corrected geometric
normal for the coat independently of the base normal map. Compute the coat
weight as:

```text
clearcoatFactor * (0.04 + 0.96 * (1 - abs(dot(view, coatNormal)))^5)
```

For each active directional or point light, mix the complete CF074 base
response with a white coat response produced by the existing bounded GGX
distribution and visibility functions at the authored coat roughness. The
coat's fixed 1.5 IOR is independent of the base IOR and specular controls. When
there is no active light, attenuate the compatibility base once by one minus
the coat weight; attenuate emission once by the same term in every lit-material
path. A coat lobe is generated only for active direct light. Branch on exact
zero intensity before layer arithmetic so omitted and explicit-zero clearcoat
preserve the accepted CF074 output path.

Append one aligned row `[factor, roughness, 0, 0]` after the complete accepted
720-byte uniform, producing an exact 736-byte layout. Preserve the first 720
bytes, 72-byte vertex ABI, thirteen-entry bind group, six texture roles,
36-entry sampler table, exactly two pipelines, attachments, capability
preflight, and resource accounting.

Do not add clearcoat textures, coat normal scale behavior, new image formats,
new GPU resources, ambient or image-based lighting, refraction, transmission,
volume, dispersion, sheen, deployment, release, or publication authority.

## Consequences

- Factor-only clearcoat composes with base color, metalness, roughness, normal
  mapping, emissive strength, IOR, and specular factors/textures under both
  direct-light kinds.
- The coat stays geometrically smooth when only the base material has a normal
  map, matching the ratified layer separation.
- Valid textured clearcoat remains explicit unsupported input rather than
  losing authored texture authority.
- The material ABI grows by one fixed row without a new dependency, texture,
  sampler, binding, pipeline, protocol, persistence, or lifecycle role.

## Status

Accepted and implemented by CF075. Importer tests cover defaults, unit bounds,
declarations, forbidden coexistence, unused malformed peers, wider payloads,
all deferred texture-info shapes, and root-reference precedence. Uniform and
CPU reference tests pin the 720-byte prefix, appended row, fixed coat Fresnel,
roughness response, geometric-normal separation, no-light attenuation,
emission attenuation, and exact zero identity. Controlled Windows/Vulkan
evidence covers directional, point, combined-light, no-light, emission, base
normal mapping, IOR/specular composition, scene override, and unchanged
non-color observations.
