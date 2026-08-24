# ADR 0072: Apply one bounded glTF material IOR

- Status: Accepted
- Date: 2026-08-24
- Task: CF072

## Context

Core glTF metallic-roughness shading assumes an index of refraction of `1.5`,
which gives dielectric normal-incidence reflectance `0.04`. The ratified
[`KHR_materials_ior`](https://github.com/KhronosGroup/glTF/blob/main/extensions/2.0/Khronos/KHR_materials_ior/README.md)
extension lets a material author change that value. Its
[schema](https://github.com/KhronosGroup/glTF/blob/main/extensions/2.0/Khronos/KHR_materials_ior/schema/material.KHR_materials_ior.schema.json)
accepts exactly zero or values at least one and defaults to `1.5`. Zero is a
compatibility mode whose Fresnel term is always one. The extension forbids
coexistence with `KHR_materials_unlit` and
`KHR_materials_pbrSpecularGlossiness`.

Cogniform already implements one bounded direct metallic-roughness BRDF with
a fixed dielectric reflectance of `0.04`. Supporting the scalar must not add
refraction, transmission, image-based lighting, texture authority, or another
GPU resource.

## Decision

Recognize `KHR_materials_ior` only through the existing unique top-level
declaration contract. Its per-material payload must be an object. The optional
`ior` member must decode to a finite f32 and be either exact zero or at least
one; omission defaults exactly to `1.5`. Reject undeclared, null, wrong-type,
negative, zero-to-one, non-finite, unlit-combined, and specular-glossiness-
combined uses without proxy substitution. Validate the supported member before
classifying a well-formed wider payload as unsupported. Apply the same checks
to selected and unused material records.

Retain authored IOR in `AssetMaterial`. Derive dielectric F0 as exact one for
IOR zero and otherwise as `((ior - 1) / (ior + 1))^2`, using f64
intermediates before validating the finite unit f32 result. Preserve the
public constructor with defaults `1.5` and `0.04`.

For selected imported metallic-roughness materials only, replace the shader's
fixed dielectric `0.04` Fresnel base with retained F0. Preserve metallic
interpolation, roughness, directional and point lighting, output clamps, and
all emissive behavior. Explicit scene materials, built-ins, missing-asset
fallbacks, proxies, and omitted extension data continue to use `0.04`.

Append one aligned `[F0, 0, 0, 0]` optical row after the accepted 624-byte
uniform. The resulting uniform is exactly 640 bytes and preserves every byte
of the prior prefix. Keep the 72-byte vertex ABI, nine-entry bind group, four
texture roles, 36-sampler table, two pipelines, attachments, and resource
counts unchanged.

Do not add `KHR_materials_specular`, refraction, transmission, volume,
dispersion, clearcoat, sheen, anisotropy, iridescence, ambient or image-based
lighting, new texture or image roles, animation, node/scene traversal,
protocol or world state, persistence, dependencies, deployment, or
release/publication authority.

## Consequences

- Omitted and explicit `1.5` IOR reproduce the prior direct response; IOR one
  gives dielectric F0 zero, and IOR zero gives angle-independent F0 one.
- Metallic-one response is independent of IOR because base-color reflectance
  retains full authority at that endpoint.
- Material metadata and one private uniform row change without affecting asset
  byte accounting, lifecycle, revision/hash/replay, or observation formats.
- Later specular-factor work can compose with the retained F0 without
  reinterpreting authored IOR.

## Status

Accepted and implemented by CF072. Importer tests cover defaults, zero, one,
ordinary and maximum values, malformed payloads, both forbidden coexistences,
unused materials, wider payloads, proxy precedence, exact derived F0, and
unchanged accounting. Renderer tests protect the exact 624-byte prefix and
640-byte total. A controlled optimized Windows/Vulkan comparison covers
directional and point response, zero/default/water/diamond values,
metallic-one invariance, scene override, normal mapping, emissive composition,
and unchanged non-color observations.
