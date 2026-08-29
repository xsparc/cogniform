# ADR 0080: Admit one bounded glTF material anisotropy texture

- Status: Accepted
- Date: 2026-08-29
- Task: CF080
- Refines: [ADR 0079](0079-bounded-gltf-material-anisotropy-factors.md)

## Context

Ratified `KHR_materials_anisotropy` defines one linear texture. Red and green
encode a tangent-space direction after mapping from `[0,1]` to `[-1,1]`; blue
multiplies the retained anisotropy strength; alpha has no meaning. CF079
validated the complete texture authority before treating it as unsupported,
but deliberately deferred the GPU role.

Admitting the role grows Cogniform's fixed imported-material resource surface
from eleven to twelve textures and samplers. WebGPU guarantees at least
sixteen sampled textures and sixteen samplers per shader stage, so this remains
inside the portable baseline without bindless resources or dynamic layouts.

## Decision

Retain one optional linear RGBA8 anisotropy texture only after the existing
strict declaration, texture-info, selected-coordinate, affine-transform, core
sampler, embedded-PNG, root-resource, and tangent-space checks succeed. Raise
root image, texture, and sampler collection caps from eleven to twelve. Decode
a shared source once on CPU, but key GPU resources by content hash and role and
reserve, upload, evict, and rehydrate all zero through twelve roles atomically.
A thirteenth referenced resource rejects before allocation or proxy policy.

Sample the role in linear space. Map sampled red/green to `[-1,1]`, normalize
the direction, compose it with the retained numeric rotation, and multiply the
numeric strength by sampled blue. Ignore alpha. If the mapped direction is
degenerate, set effective anisotropy strength to zero rather than inventing a
direction or normalizing a zero vector. The deterministic squared-length
threshold is `1e-6`: it admits every nonzero direction representable by one
RG8 texel while absorbing near-zero filtered interpolation error on the
validated GPU path. Missing texture authority and any effective zero strength
preserve the accepted CF079 compatibility path.

Append two padded affine rows after the exact accepted 928-byte uniform prefix,
producing 960 bytes. The first appended row's padding lane records role
presence. Use selector bit 17 for secondary anisotropy coordinates. Append
view/sampler bindings 23 and 24, producing twenty-five bind entries, and
require twelve sampled textures and twelve samplers per shader stage.

Preserve the 72-byte vertex ABI, 36-entry sampler table, two pipelines,
attachments, observations, protocol, persistence, dependencies, lifecycle,
replay, release, and deployment authority. Do not add new image formats, mip
generation, ambient or image-based lighting, shadows, transmission, volume,
iridescence, dispersion, remote authority, deployment, release, or
publication authority.

## Consequences

- Ratified texture direction and strength compose with the accepted numeric
  anisotropy factors and direct-light response.
- Linear color-space and channel authority are explicit; alpha cannot affect
  rendering and a degenerate direction cannot create a non-finite value.
- The fixed renderer layout grows within baseline WebGPU limits and
  insufficient adapters fail during explicit capability preflight.
- Every previously accepted uniform byte, vertex byte, observation, and
  authority boundary remains stable.

## Status

Accepted and implemented by CF080. Importer tests cover retained linear texels,
selected coordinates, transforms, samplers, malformed and dangling authority,
invalid-before-unsupported precedence, and exact twelve-versus-thirteen
resource bounds. Renderer unit tests pin atomic twelve-role reservations,
exact eviction accounting, the 928-byte uniform prefix, appended rows,
selector bit 17, and twenty-five-binding preflight. Controlled Vulkan evidence
covers red/green direction, blue strength, ignored alpha, numeric rotation,
tangent handedness, generated tangents, combined layers, independent sampler,
coordinate and transform authority, filtered degenerate direction, exact zero
identity, directional and point lights, scene override, no-light behavior, and
unchanged non-color observations. Shared and distinct CPU image accounting and
exact-hash twelve-role rehydration are pinned separately.
