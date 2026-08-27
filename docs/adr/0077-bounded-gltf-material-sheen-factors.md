# ADR 0077: Admit bounded glTF material sheen factors

- Status: Accepted
- Date: 2026-08-27
- Task: CF077
- Refines: [ADR 0076](0076-bounded-gltf-material-clearcoat-textures.md)

## Context

The ratified
[`KHR_materials_sheen`](https://github.com/KhronosGroup/glTF/blob/main/extensions/2.0/Khronos/KHR_materials_sheen/README.md)
adds a colored retroreflective lobe to metallic-roughness materials. Its
numeric surface consists of an exact three-channel finite unit
`sheenColorFactor`, default `[0,0,0]`, and a finite unit
`sheenRoughnessFactor`, default zero. The extension also defines color and
roughness textures, but admitting those roles would grow Cogniform's fixed
nine-role material resource surface.

The Khronos sample renderer uses the Charlie distribution and a fitted sheen
visibility term. That non-normative implementation is not energy conserving:
a deterministic midpoint hemisphere grid reaches approximate directional
albedo `1.35537` at roughness `0.05` and view-normal cosine `0.02`. Cogniform
needs a bounded result without adding a runtime directional-albedo lookup
texture, sampler, binding, or device-dependent integration step.

## Decision

Recognize strictly declared `KHR_materials_sheen` only on metallic-roughness
materials that are neither unlit nor legacy specular-glossiness. Retain the
two numeric factors only after exact shape, finiteness, and unit-interval
validation. Validate both optional sheen texture infos, their declared
texture transforms, root texture/sampler/image/PNG resources, and selected
primitive coordinate sets before classifying otherwise valid texture-bearing
or wider payloads as unsupported. Do not admit a sheen texture role or raise
any root collection limit.

Use the Khronos Charlie distribution and fitted visibility term for direct
directional and point lighting through the selected base shading normal. Floor
perceptual roughness at `1e-6`. Clamp the scalar sheen BRDF pointwise to
`1 / PI` before color multiplication, then compute
`base * (1 - max(sheenColor)) + sheenColor * boundedSheen`. The pointwise cap
is an intentional conservative deviation from the non-normative sample
renderer: because the cosine-weighted hemisphere integral of `1 / PI` is one,
and every color channel is at most the maximum color channel, the combined
base-plus-sheen response cannot gain energy when the base response is itself
bounded. Exact zero sheen color bypasses all sheen math and preserves the
accepted base path bit-for-bit.

Apply the existing clearcoat layer above the completed base-plus-sheen
response. Do not apply sheen to the no-active-light compatibility path,
emission, unlit materials, explicit scene materials, built-ins, unresolved
fallbacks, or proxies. Append one factor row containing sheen RGB and
roughness after the exact accepted 832-byte uniform prefix, producing 848
bytes. Preserve the 72-byte vertex ABI, nine texture roles, nineteen bind
entries, 36 samplers, and two pipelines.

Do not add sheen textures, directional-albedo lookup resources, occlusion,
anisotropy, iridescence, ambient or image-based lighting, transmission,
volume, dispersion, new image formats, remote authority, deployment, release,
or publication authority.

## Consequences

- Factor-only ratified sheen renders deterministically without growing GPU
  resource topology.
- Well-formed sheen textures remain explicit proxy candidates only after all
  supported authority and referenced resources validate; malformed or dangling
  authority still fails closed.
- The `1 / PI` cap trades some grazing-lobe intensity for a simple analytic
  bound and removes any need for a runtime directional-albedo LUT.
- Clearcoat continues to own the outer layer, while exact zero color, no-light
  compatibility, emission, observations, and scene overrides remain unchanged.

## Status

Accepted and implemented by CF077. Importer tests cover defaults, exact unit
factors, selected and unused materials, malformed and forbidden combinations,
texture-info/resource/coordinate validation before proxy classification, and
wider-payload precedence. Independent numeric tests pin Charlie and fitted-
visibility vectors, reproduce the uncapped energy regression, prove the
pointwise analytic bound, sweep a bounded hemisphere grid, and preserve exact
zero identity. The uniform test pins the exact 832-byte prefix and appended
row. Controlled Vulkan evidence covers directional, point, combined-light,
color, roughness, clearcoat, scene-override, no-light, emission, and unchanged
non-color behavior.
