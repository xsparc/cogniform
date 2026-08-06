# ADR 0026: Bounded direct metallic-roughness response

- Status: Accepted
- Date: 2026-08-06
- Task: CF026
- Refines: [ADR 0024](0024-bounded-point-diffuse-lighting.md)

## Context

The public `MaterialComponent` has always carried a base color, metallic value,
and perceptual roughness, but the headless renderer used only base color.
CF023 and CF024 established bounded directional and point definitions, while
CF025 corrected built-in cuboid exterior normals. The remaining numeric
material inputs can therefore become observable without adding another scene
contract, light source, geometry path, or resource collection.

A texture system, image-based lighting, shadows, or an ad hoc shininess model
would widen unrelated trust and compatibility surfaces. Continuing to ignore
two required public fields would leave materially different scene states with
the same lit observation.

## Decision

Active directional and point lights use one direct Cook-Torrance
metallic-roughness response in linear color space. The normal-distribution
term is GGX, visibility uses the Schlick-GGX approximation to Smith geometry,
and Fresnel uses Schlick's approximation. Normal-incidence reflectance is
`mix(vec3(0.04), base_color, metallic)`. The energy split is
`(1 - Fresnel) * (1 - metallic)` for Lambert diffuse plus the microfacet
specular term, multiplied by `max(dot(normal, surface_to_light), 0)`.

Perceptual roughness is clamped to `0.05` only in the GGX distribution to
avoid a singular zero-width highlight. The existing directional radiance,
point inverse-square attenuation and exact coincidence/derived-overflow rules
remain unchanged. Each contribution and their sum are bounded to the linear
unit interval before the existing `Rgba8Unorm` target. Material alpha is not
lit. A scene with no active directional or point definition bypasses the BRDF
and returns the exact material base RGBA value.

The view direction comes from the selected camera's extracted world
translation. Camera translation, like models and active point positions, must
convert to finite GPU `f32` before submission. A zero or derived-overflow view
vector omits the specular term without producing a non-finite value; bounded
diffuse remains available.

The per-draw uniform grows from 448 to exactly 480 bytes. Its first 448 bytes
remain the complete CF024 model, view-projection, color, identity,
directional-light, and point-light layout. Two appended zero-padded `vec4`
slots contain camera position and then metallic/roughness. A render entity
without a material keeps its existing fallback color and uses a neutral
dielectric `metallic = 0`, `roughness = 0.8`, matching the compiler's neutral
material parameters.

## Consequences

- Metallic and roughness changes now produce distinct revision-linked color
  evidence while depth, stable identity, normals, visibility, and logical
  replay state remain unchanged.
- Existing lit color values intentionally change because active lights now use
  the public material model. The workspace remains unpublished at `0.0.0`, so
  this behavioral change requires no release or schema migration.
- Exact unlit output, fixed four-definition light capacities, stable ordering,
  point attenuation, one bind group, and one pipeline remain intact.
- Linear RGBA8 output and a controlled adapter tolerance remain the contract;
  cross-adapter bitwise image identity is not claimed.
- Textures, UVs, tangents, normal maps, image-based or ambient lighting,
  emissive response, shadows, spot lights, HDR, tone mapping, gamma changes,
  transparency, configurable lighting, culling, and clustering remain
  separate decisions.
- Protocol, world state, hashing, replay, recovery, asset formats, transport,
  persistence, dependencies, CI, deployment, and release publication do not
  change.

## Status

Implemented by CF026 with an exact 480-byte uniform test, prepared
camera/material tests, controlled dielectric/metallic/roughness and exact
unlit probes, directional/point regressions, imported-asset regressions, and
the canonical engine scenario on the validated Windows/Vulkan profile.
