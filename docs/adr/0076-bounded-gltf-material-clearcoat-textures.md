# ADR 0076: Admit bounded glTF material clearcoat textures

- Status: Accepted
- Date: 2026-08-26
- Task: CF076
- Refines: [ADR 0075](0075-bounded-gltf-material-clearcoat-factors.md)

## Context

The ratified
[`KHR_materials_clearcoat`](https://github.com/KhronosGroup/glTF/blob/main/extensions/2.0/Khronos/KHR_materials_clearcoat/README.md)
defines three independent texture roles. Linear clearcoat-texture red
multiplies coat intensity, linear roughness-texture green multiplies coat
roughness, and a linear RGB normal texture supplies a tangent-space coat
normal whose finite scale affects X and Y. The coat normal remains independent
of the base normal; without a coat normal texture, the coat uses the geometric
normal.

CF075 already validates these texture infos and all referenced resources, but
returns well-formed texture authority as unsupported. Admitting the roles
requires bounded root collections, role-keyed CPU/GPU accounting, three fixed
texture/sampler pairs, independent coordinate and transform retention, and an
unambiguous generated-tangent rule.

Current WebGPU requires implementations to support at least sixteen sampled
textures and sixteen samplers per shader stage. Cogniform still performs an
explicit preflight at the smaller fixed requirement used by this renderer.
Draft `KHR_materials_clearcoat_ior` and `KHR_materials_coat` proposals remain
advisory and do not change the ratified fixed-IOR semantics.

## Decision

Admit clearcoat intensity, roughness, and normal textures only after the
existing strict declaration, texture-info, sampler, image, PNG, coordinate,
and affine-result validation. Raise root image, texture, and sampler limits
from six to nine. Retain zero through nine immutable role values while decoding
a shared source once on CPU. Reserve and release GPU resources by content hash
and role, so shared bytes remain distinct resources where color-space or
channel semantics differ.

Sample intensity from linear red and roughness from linear green; ignore their
other channels. Multiply each sampled value by its retained numeric factor.
Sample the coat normal from linear RGB, apply finite authored scale to X and Y,
normalize through the existing guarded tangent basis, and use it only for the
coat lobe and coat Fresnel weight. A missing coat normal retains the geometric
coat normal even when the base normal is mapped. Exact numeric coat factor zero
skips all coat texture and tangent-normal work and preserves the accepted
CF075 output path.

Each new role independently retains sampler, effective coordinate set zero or
one, and `KHR_texture_transform`. Authored tangents permit the base and coat
normal roles to select different effective coordinate streams. When tangents
must be generated, a coat-normal-only material generates them from the coat
normal's effective transformed coordinates. If both base and coat normals are
present without source tangents, their effective coordinate set and transform
must match exactly; otherwise admission returns bounded
`UnsupportedFeature` at `glb.decoded.generated_tangent_space` rather than
choosing one basis silently.

Append six aligned affine rows after the exact accepted 736-byte prefix:
clearcoat intensity, clearcoat roughness, then clearcoat normal. The coat-normal
rows also carry scale and role presence. The fixed uniform becomes 832 bytes.
Use selector bits 12 through 14, which keep the complete material flag value
within exact f32 integer range. Append bindings 13 through 18 for the three
texture/sampler pairs, producing nineteen bindings and explicit requirements
of nine sampled textures and nine samplers per shader stage. Reuse linear-white
fallbacks for intensity and roughness and the neutral-normal fallback for coat
normal. Preserve the 72-byte vertex ABI, 36-entry sampler table, exactly two
pipelines, attachments, observations, protocol, persistence, lifecycle,
dependency, release, and deployment boundaries.

Do not add draft coat extensions, configurable coat IOR, new image formats,
occlusion, sheen, anisotropy, iridescence, ambient or image-based lighting,
refraction, transmission, volume, dispersion, runtime binding growth, release,
deployment, or publication authority.

## Consequences

- All ratified clearcoat texture authority now renders instead of becoming a
  proxy candidate, while malformed or over-budget authority still fails
  closed.
- A material can use a coat normal independently from its base normal without
  increasing vertex bytes or pipeline count.
- Tangent generation never silently picks between incompatible normal-role
  coordinate streams.
- The adapter requirement grows from six to nine sampled textures and samplers
  and from thirteen to nineteen bind-group entries, below WebGPU's guaranteed
  per-stage sampled-resource floor.

## Status

Accepted and implemented by CF076. Importer tests cover all three channels,
finite scale, independent samplers/selectors/transforms, shared and distinct
nine-role accounting, ten-resource rejection, coat-only tangent generation,
matching dual-normal generation, ambiguous generated-basis rejection, and
authored-tangent independence. Uniform and GPU tests pin the exact 736-byte
prefix, appended rows, fixed resource counts, neutral fallback identity,
red/green channel selection, factor equivalence, zero-factor identity,
independent coat normals, scale zero, eviction, and rehydration.
