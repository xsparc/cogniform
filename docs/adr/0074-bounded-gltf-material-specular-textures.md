# ADR 0074: Sample bounded glTF material specular textures

- Status: Accepted
- Date: 2026-08-25
- Task: CF074
- Refines: [ADR 0073](0073-bounded-gltf-material-specular-factors.md)

## Context

The ratified
[`KHR_materials_specular`](https://github.com/KhronosGroup/glTF/blob/main/extensions/2.0/Khronos/KHR_materials_specular/README.md)
extension defines two independent texture multipliers for the numeric factors
accepted by CF073. `specularTexture` contributes only its linear alpha channel
to dielectric strength. `specularColorTexture` contributes only its sRGB-
decoded RGB channels to dielectric color. The unused strength RGB and color
alpha channels have no material effect.

CF073 deliberately validated both texture infos and their complete resource
authority before classifying them as unsupported. Cogniform already has
bounded embedded PNG decoding, coordinate sets zero and one, ratified texture
transforms, core samplers, exact shared-image CPU accounting, and role-
separated GPU residency. Adopting the two deferred roles can reuse those
contracts without adding a decoder, image format, mip level, sampler variant,
or pipeline.

The Khronos
[`SpecularTest`](https://github.com/KhronosGroup/glTF-Sample-Assets/tree/main/Models/SpecularTest)
sample documents the same channel semantics and warns that strength RGB may
contain obsolete authoring data. CF074 uses that behavior as a test-design
reference but adds no third-party sample bytes or assets to the repository.

## Decision

Admit optional `specularTexture` and `specularColorTexture` only within the
strict `KHR_materials_specular` declaration, material-exclusion, texture-info,
resource, PNG, coordinate, transform, and sampler contracts established by
CF073 and the existing core texture slices. Retain each role's effective core
or extension-overridden coordinate selector, affine transform, and authored
core sampler independently. Malformed, dangling, undeclared, non-finite,
missing-coordinate, over-limit, or inconsistent role authority rejects
without proxy substitution. Well-formed wider extension data retains the
existing explicit unsupported policy after supported fields validate.

Raise the strict root image, texture, and sampler collection limits from four
to six. Every retained image and texture must still be referenced by one of
the six supported roles, and each role still permits one shared texture index
across materials. Decode a shared source image once into CPU residency while
retaining distinct immutable role values and reserving each missing content-
hash-and-role GPU resource atomically. Eviction and exact-hash rehydration
release and restore zero through six role resources exactly.

Upload `specularTexture` as linear `Rgba8Unorm`, sample alpha only, and
multiply it with retained `specularFactor`. Upload `specularColorTexture` as
`Rgba8UnormSrgb`, sample decoded RGB only, and multiply it component-wise with
retained `specularColorFactor`. Bind the existing linear-white and sRGB-white
fallbacks when either role is inactive, preserving exact CF073 output for
omission and explicit neutral textures. Imported unlit materials keep these
validated resources visually inert, and an explicit scene material disables
both roles with the rest of the imported material.

Append four padded affine rows in strength-then-color order after the accepted
656-byte draw-uniform prefix, producing an exact 720-byte uniform. Add two
coordinate-selector bits at positions 10 and 11; the complete flag value
remains exactly representable as f32. Append strength texture/sampler bindings
9 and 10 and color texture/sampler bindings 11 and 12. Adapter preflight now
requires six sampled textures, six samplers per shader stage, and thirteen
bindings per group. Keep the fixed 36-entry sampler table, 72-byte vertex ABI,
three attachments, exactly two pipelines, stable draw order, and every prior
uniform byte unchanged.

Do not add external or data URIs, JPEG, wider PNG forms, mip generation or
storage, anisotropy, comparison or explicit LOD controls, texture arrays,
occlusion, clearcoat, sheen, anisotropy, iridescence, transmission, volume,
refraction, dispersion, ambient or image-based lighting, deployment, release,
or publication authority.

## Consequences

- Authored dielectric strength and color textures compose in the Khronos-
  specified channels and color spaces under both direct-light paths.
- Shared image bytes remain counted once on CPU, while distinct role semantics
  remain independently bounded and lifecycle-accounted on GPU.
- The fixed bind group and uniform grow once, without pipeline, sampler-table,
  dependency, protocol, persistence, or logical-scene growth.
- Adapters below the six-texture, six-sampler, or thirteen-binding baseline now
  fail structured capability preflight before pipeline construction.

## Status

Accepted and implemented by CF074. Importer tests cover declarations,
resources, valid admission, independent selectors/transforms/samplers, exact
four-to-six collection boundaries, shared and distinct CPU accounting, atomic
six-role reservation, eviction, and rejection precedence. Uniform tests pin
the complete 656-byte prefix and appended transform order. Controlled
Windows/Vulkan comparisons cover neutral fallbacks, ignored channels, linear
strength alpha, sRGB color RGB, directional and point composition, patterned
coordinate overrides, transforms, authored sampler wrapping, scene override,
exact role eviction/rehydration, and unchanged non-color observations.
