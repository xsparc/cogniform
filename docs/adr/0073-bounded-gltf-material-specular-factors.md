# ADR 0073: Apply bounded glTF material specular factors

- Status: Accepted
- Date: 2026-08-25
- Task: CF073
- Refines: [ADR 0072](0072-bounded-gltf-material-ior.md)

## Context

The ratified
[`KHR_materials_specular`](https://github.com/KhronosGroup/glTF/blob/main/extensions/2.0/Khronos/KHR_materials_specular/README.md)
extension lets a metallic-roughness material scale and tint its dielectric
Fresnel response. Its
[schema](https://github.com/KhronosGroup/glTF/blob/main/extensions/2.0/Khronos/KHR_materials_specular/schema/material.KHR_materials_specular.schema.json)
defines unit `specularFactor` with default one and a three-channel finite
non-negative `specularColorFactor` with default `[1,1,1]` and no upper bound.
The extension excludes unlit and legacy specular-glossiness materials.

CF072 retained bounded IOR-derived dielectric F0. Applying the two numeric
specular factors can therefore complete the bounded direct-light composition
without adding a texture role. The extension also defines two texture infos;
silently ignoring either would render a materially different asset as if it
were supported.

## Decision

Recognize `KHR_materials_specular` only through the strict unique top-level
declaration contract. Require an object payload. Decode optional
`specularFactor` as a finite unit f32 with exact default one and optional
`specularColorFactor` as exactly three finite non-negative f32 values with
exact default `[1,1,1]`; values above one remain valid. Reject undeclared,
null, malformed, non-finite, negative, out-of-range-strength, unlit-combined,
and `KHR_materials_pbrSpecularGlossiness`-combined data without proxy
substitution. Apply these checks to selected and unused materials before
classifying a well-formed wider payload as unsupported.

Fully decode both extension texture infos, including required texture index,
optional coordinate selector, declared `KHR_texture_transform`, root texture,
sampler, source, image, PNG, and selected primitive coordinate references.
Malformed or dangling authority rejects without proxy. A well-formed
`specularTexture` or `specularColorTexture` remains an explicit unsupported-
extension/proxy candidate because CF073 does not add either texture role.

Retain both numeric factors in `AssetMaterial`. For non-default factors,
compute dielectric normal reflectance as
`min(IOR_F0 * specularColorFactor, 1) * specularFactor`, clamping each color
channel before strength multiplication. Use `specularFactor` as dielectric
grazing reflectance, RGB Schlick Fresnel for the direct microfacet lobe, and
one minus the maximum dielectric Fresnel channel for scalar diffuse energy.
Metallic Fresnel continues to use base color and grazing one, so metallic-one
response is independent of both factors. Strength zero gives pure dielectric
diffuse. Exact default factors preserve the accepted CF072 direct-response
path, including authored IOR behavior.

Explicit scene materials, built-ins, missing-asset fallbacks, proxies, unlit
materials, and omitted extension data use neutral factors. Append one aligned
`[color.r,color.g,color.b,strength]` row after the accepted 640-byte uniform,
for an exact 656-byte total and unchanged first 640 bytes. Preserve the
72-byte vertex ABI, nine-entry bind group, four texture roles, 36-sampler
table, two pipelines, attachments, resource counts, observations, lifecycle,
revision, logical hash, replay, protocol, persistence, dependencies, workflow,
and release authority.

Do not add the two specular textures, new image formats, texture roles,
samplers, bindings, refraction, transmission, volume, dispersion, clearcoat,
sheen, anisotropy, iridescence, ambient or image-based lighting, deployment,
release, or publication authority.

## Consequences

- Zero strength, tinted dielectric response, above-one color, IOR composition,
  scalar diffuse energy, and metallic-one independence are deterministic.
- Texture-bearing materials cannot lose authored authority through silent
  partial support; valid texture payloads follow explicit proxy policy until
  CF074.
- Material metadata and one private uniform row change without affecting asset
  byte accounting or renderer resource topology.

## Status

Accepted and implemented by CF073. Importer tests cover declarations,
defaults, numeric domains, selected and unused materials, forbidden
coexistence, wider payloads, and texture-info/resource/coordinate precedence.
CPU vectors protect clamp order, f90, zero strength, scalar energy, IOR
composition, and metallic-one independence. A controlled Windows/Vulkan case
covers exact default identity, directional, point, combined-light, zero,
tinted, high-color, IOR, metallic-one, scene-override, emissive-composition,
and unchanged non-color observations.
