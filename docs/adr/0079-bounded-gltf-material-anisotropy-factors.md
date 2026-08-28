# ADR 0079: Admit bounded glTF material anisotropy factors

- Status: Accepted
- Date: 2026-08-28
- Task: CF079
- Refines: [ADR 0078](0078-bounded-gltf-material-sheen-textures.md)

## Context

The ratified
[`KHR_materials_anisotropy`](https://github.com/KhronosGroup/glTF/blob/main/extensions/2.0/Khronos/KHR_materials_anisotropy/README.md)
adds directional roughness to metallic-roughness materials. Its numeric surface
is a finite unit `anisotropyStrength`, default zero, and a finite-radian
`anisotropyRotation`, default zero. The extension also defines one linear
texture whose red/green channels encode direction and whose blue channel
multiplies strength. Admitting that texture would expand Cogniform's fixed
eleven-role material resource surface and is a separate bounded decision.

The ratified model requires a tangent space supplied by source `NORMAL` plus
`TANGENT`, or by the base normal-texture tangent-generation path. Cogniform's
existing direct isotropic GGX path intentionally predates the correlated
visibility used by the ratified anisotropic model, so merely evaluating the
anisotropic equations at strength zero would not preserve accepted output.

## Decision

Recognize strictly declared `KHR_materials_anisotropy` only on metallic-
roughness materials that are neither unlit nor legacy specular-glossiness.
Retain finite unit strength and finite rotation after exact validation, and
precompute finite cosine/sine once during import. Validate optional
`anisotropyTexture`, its declared texture transform, root texture/sampler/
image/PNG authority, and selected primitive coordinates before classifying an
otherwise valid texture-bearing or wider payload as unsupported. Do not admit
an anisotropy texture role or raise a root collection limit.

Require the selected primitive to provide valid source normals and tangents,
unless its supported base normal texture already invokes bounded tangent
generation. A clearcoat normal alone does not establish the base tangent
space. Missing or invalid authority rejects before proxy classification.

For nonzero strength, rotate the retained tangent/bitangent basis and evaluate
the ratified anisotropic GGX distribution and correlated visibility for direct
directional and point lighting. With perceptual roughness floored at `0.05`,
use `alpha = roughness^2`, tangent roughness
`mix(alpha, 1, strength^2)`, and bitangent roughness `alpha`. Degenerate
interpolated tangent bases fall back to the accepted isotropic path without
creating a non-finite value. Exact zero strength bypasses all new basis and
anisotropic BRDF work and preserves the prior response bit-for-bit.

Apply anisotropy only to the existing base specular lobe. Preserve diffuse,
IOR/specular composition, sheen and clearcoat layering, emission, no-light
compatibility, unlit behavior, scene-material overrides, built-ins, fallbacks,
proxies, and non-color observations. Append `[strength, cos, sin, 0]` after
the exact accepted 912-byte uniform prefix, producing 928 bytes. Preserve the
72-byte vertex ABI, eleven texture roles, twenty-three bind entries, 36
samplers, and two pipelines.

Do not add the anisotropy texture, new tangent algorithms, ambient or image-
based lighting, shadows, transmission, volume, iridescence, dispersion,
dynamic resources, remote authority, deployment, release, or publication
authority.

## Consequences

- Factor/rotation-only ratified anisotropy renders deterministically without
  expanding renderer resource topology.
- Exact zero strength preserves every accepted isotropic pixel even though the
  ratified nonzero path uses a different visibility model.
- Texture-bearing anisotropy remains an explicit proxy candidate only after
  its complete supported authority validates; malformed or dangling authority
  fails closed.
- Tangent-space requirements are explicit and reuse the already bounded base
  normal-texture generation path.

## Status

Accepted and implemented by CF079. Importer tests cover defaults, exact
strength/rotation retention, selected and unused materials, malformed and
forbidden combinations, required tangent space, base-normal generation,
texture-info/resource/coordinate validation before proxy classification, and
wider-payload precedence. CPU vectors pin zero identity, anisotropic
distribution, visibility, and rotation. The uniform test pins the exact
912-byte prefix and appended row. Controlled Vulkan evidence covers
directional and point lights, rotation, roughness, tangent handedness,
combined existing layers, scene override, no-light behavior, exact zero
identity, and unchanged non-color observations.
