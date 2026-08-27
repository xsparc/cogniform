# ADR 0078: Admit bounded glTF material sheen textures

- Status: Accepted
- Date: 2026-08-27
- Task: CF078
- Refines: [ADR 0077](0077-bounded-gltf-material-sheen-factors.md)

## Context

Ratified
[`KHR_materials_sheen`](https://github.com/KhronosGroup/glTF/blob/main/extensions/2.0/Khronos/KHR_materials_sheen/README.md)
defines two optional texture roles. `sheenColorTexture` supplies sRGB-decoded
RGB that multiplies `sheenColorFactor`; its alpha is ignored.
`sheenRoughnessTexture` supplies linear alpha that multiplies
`sheenRoughnessFactor`; its RGB is ignored. CF077 validated both infos and
their complete referenced authority but kept otherwise valid texture-bearing
sheen unsupported.

Admitting the roles grows Cogniform's fixed imported-material resource surface
from nine to eleven textures and samplers. WebGPU guarantees at least sixteen
sampled textures and sixteen samplers per shader stage, so the bounded layout
remains inside the portable baseline without adding bindless resources,
runtime-selected layouts, or a device-specific fallback.

## Decision

Admit both sheen texture roles only for an otherwise supported imported
metallic-roughness material. Reuse the strict embedded PNG, unique declaration,
texture-info, core sampler, coordinate-set zero-or-one, and
`KHR_texture_transform` contracts. Retain independent samplers, effective
coordinate selectors, and finite affine transforms. Raise root image, texture,
and sampler collection caps from nine to eleven. A source shared by roles is
decoded once on CPU; GPU resources remain uniquely keyed by content hash and
role and reserve, upload, evict, and rehydrate atomically.

Upload sheen color as `Rgba8UnormSrgb` and roughness as `Rgba8Unorm`.
Multiply sampled color RGB and roughness alpha into the retained numeric
factors before the accepted bounded Charlie response. Ignore color alpha and
roughness RGB. Bind role-correct white fallbacks. Exact zero effective sheen
color bypasses the complete sheen path and preserves prior output.

Append four padded affine rows after the exact accepted 848-byte uniform
prefix, producing 912 bytes. Selector bits 15 and 16 choose secondary
coordinates for sheen color and roughness; the maximum accepted flag value
131,071 remains exactly representable in the existing f32 lane. Append
bindings 19 through 22 for the two views and samplers, producing twenty-three
entries. Require eleven sampled textures and eleven samplers per shader stage.
Preserve the 72-byte vertex ABI, fixed 36-entry sampler table, two pipelines,
attachments, observations, lifecycle, replay, protocol, persistence,
dependencies, and release authority.

Do not add tangent-generation roles, new image formats, mip generation,
anisotropy, lookup textures, ambient or image-based lighting, transmission,
volume, remote authority, deployment, release, or publication authority.

## Consequences

- Ratified sheen textures compose with the accepted numeric factors, bounded
  direct-light response, scene override, and clearcoat-above-sheen ordering.
- Root collection and GPU residency limits rise exactly to eleven while shared
  CPU image accounting remains unchanged.
- The fixed layout grows within guaranteed WebGPU sampled-texture and sampler
  limits; adapters below the explicit binding requirements fail before
  pipeline construction.
- No new tangent ambiguity is introduced because neither sheen role affects a
  shading normal.

## Status

Accepted and implemented by CF078. Importer tests cover retained texels,
factors, defaults, selected coordinates, affine transforms, samplers,
malformed/dangling authority, and the exact eleven-resource boundary. Renderer
unit tests pin atomic eleven-role reservations, exact eviction accounting, the
848-byte uniform prefix, four appended rows, and the expanded selector flags.
Controlled Vulkan tests cover role-correct color space and channels, neutral
fallbacks, factor multiplication, scene override, independent samplers,
coordinate selectors, affine transforms, and unchanged non-color
observations.
