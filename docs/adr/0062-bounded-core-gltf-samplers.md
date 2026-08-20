# ADR 0062: Render bounded core glTF samplers

- Status: Accepted
- Date: 2026-08-19
- Task: CF062

## Context

The approved embedded-PNG path retained one primary coordinate set and four
independent material texture roles, but required an omitted sampler and bound
one repeat/linear sampler to every role. This changed authored edge and filter
behavior and prevented otherwise bounded core glTF assets from rendering
faithfully. Cogniform stores one image level, so accepting mipmapped source
filters also needs a deterministic fallback rather than implicit backend
selection.

Sampler input remains adversarial. A valid unsupported peer must not hide a
malformed selected or unused sampler, texture index, image, material, or
coordinate record. Per-asset sampler creation or a sampler cache would also
add attacker-keyed GPU allocation and accounting state.

## Decision

Extend the strict imported texture subset as follows:

- admit at most four root sampler objects. Each object contains only optional
  core `magFilter`, `minFilter`, `wrapS`, and `wrapT` integer fields. Explicit
  null, wrong type, unknown field, invalid enum, an out-of-range texture
  sampler index, or a fifth sampler is malformed and cannot proxy, including
  when the record or texture is unused;
- retain exact source minification as typed immutable metadata. Map source
  `NEAREST`, `NEAREST_MIPMAP_NEAREST`, and `NEAREST_MIPMAP_LINEAR` to nearest
  sampling of Cogniform's one retained level. Map source `LINEAR`,
  `LINEAR_MIPMAP_NEAREST`, and `LINEAR_MIPMAP_LINEAR` to linear sampling. This
  follows the glTF recommendation when mipmap generation is unavailable;
- default omitted magnification and minification to linear and omitted S/T
  wrapping to repeat. An omitted texture sampler therefore remains compatible
  with the prior Cogniform output;
- retain one sampler descriptor independently for base-color, normal,
  metallic-roughness, and emissive roles, even when roles share one source
  image or sampler record. Valid unused sampler records retain the existing
  explicit unsupported/proxy classification only after complete hard
  validation;
- create exactly 36 renderer-owned samplers once at initialization from three
  S wraps by three T wraps by two magnification filters by two effective
  minification filters. The table has no per-asset entries, runtime growth, or
  lifecycle accounting. Disabled roles, built-ins, fallbacks, imported unlit
  inactive roles, and explicit scene overrides bind the linear/repeat entry;
- preserve view bindings 1, 3, 4, and 5 and base sampler binding 2; append
  normal, metallic-roughness, and emissive sampler bindings 6 through 8. The
  adapter preflight explicitly requires at least four sampled textures, four
  samplers per shader stage, and nine bind-group entries.

Keep four root images/textures/samplers, one retained mip, four role-separated
textures, the 48-byte vertex, 496-byte uniform, two fixed pipelines, stable
draw order, scene override, alpha, face, unlit, observations, eviction,
rehydration, revision, logical hash, and replay contracts. Do not add mip
generation or storage, anisotropy, comparison or LOD controls, texture
transforms, more coordinates or roles, JPEG, BLEND, ambient/IBL/occlusion,
dependencies, protocol/world changes, or release authority.

## Consequences

- `AssetSampler`, non-exhaustive filter/min-filter/wrap enums, immutable
  getters, and four additive `AssetMaterial` sampler accessors become public.
  Packages remain unpublished and non-publishable.
- The bind layout grows from six to nine fixed entries and initialization owns
  36 samplers. Texture counts and bytes are unchanged because sampler metadata
  and GPU samplers are not texture resources.
- Rollback is a code-and-document revert with no persisted migration. Assets
  that rely on explicit samplers return to unsupported handling after rollback.

## Status

Accepted and implemented by CF062. Ordinary tests exhaustively cover source
enums, defaults, all 36 effective keys, strict selected/unused failures,
malformed-peer precedence, four independent role descriptors, and unchanged
shared-image accounting. Controlled optimized Vulkan tests distinguish all
nine S/T wrap combinations, nearest/linear magnification, the two documented
one-mip minification families, and four independent role bindings for one
shared image. All prior optimized asset and headless renderer probes pass
together.
