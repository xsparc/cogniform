# ADR 0058: Render bounded core glTF emissive textures

- Status: Accepted
- Date: 2026-08-19
- Task: CF058

## Context

The strict GLB path retained bounded core `emissiveFactor` and three embedded
PNG texture roles, but rejected core `material.emissiveTexture`. Core glTF
defines the emissive texture as sRGB RGB multiplied by the linear emissive
factor; alpha is ignored, `texCoord` defaults to zero, and an omitted texture
behaves as white. WebGPU's `Rgba8UnormSrgb` sampling performs the required
sRGB-to-linear RGB conversion.

Adding this role widens asset image, GPU residency, bind-group, and shader
boundaries. It must remain surface-local, bounded, explicit, and subordinate
to the existing scene-material override without granting light, exposure, or
cross-surface authority.

## Decision

Extend the approved GLB subset with one optional core `emissiveTexture` role:

- accept one shared-per-role in-range texture index using omitted or zero
  `texCoord`, the existing explicit `TEXCOORD_0`, and the existing embedded
  static RGB/RGBA PNG subset;
- retain at most four root textures and four referenced root images across
  base-color, metallic-roughness, normal, and emissive roles. Decode a source
  image shared by roles once for CPU accounting;
- reject malformed/null/type-invalid/out-of-range indices, malformed
  coordinates, missing primary coordinates, invalid images, and count or byte-
  limit failures before readiness, without proxy substitution. Valid but
  unsupported nonzero coordinate sets or unused bounded records remain
  eligible only for the explicit proxy policy and retain no texture;
- carry explicit emissive role metadata and immutable texels through
  `AssetMaterial`, `AssetUploadJob`, renderer admission, residency, eviction,
  and rehydration;
- reserve zero to four missing content-hash-and-role GPU resources atomically.
  A shared CPU image still creates separate GPU role resources because formats
  and sampling semantics differ;
- upload emissive texels as `Rgba8UnormSrgb`, sample RGB through the fixed
  repeat/linear one-mip sampler, ignore alpha, multiply by the existing linear
  numeric emissive factor, then add through the CF057 bounded surface path;
- bind the existing white sRGB fallback when the role is absent. A zero
  emissive factor remains neutral, and an explicit scene `MaterialComponent`
  disables all four imported texture roles and imported emission;
- keep decode, upload, eviction, rendering, and rehydration logically neutral:
  revision, replay, world hash, identity, depth, background, and geometric-
  normal observations do not change because of asset lifecycle work.

Do not add `KHR_materials_emissive_strength`, cross-surface illumination,
bloom, HDR, exposure, tone mapping, ambient or image-based lighting,
occlusion, alpha modes, double-sided materials, additional coordinate sets,
custom samplers, external images, mipmaps, generated tangents, protocol,
recovery, transport, dependency, or release authority.

## Consequences

- Approved GLBs can modulate bounded self-emission with one standard embedded
  PNG while preserving the existing RGBA8 observation contract.
- `AssetMaterial::has_emissive_texture` and
  `AssetUploadJob::emissive_texture` are additive public accessors. Existing
  constructors and numeric defaults remain unchanged.
- Asset root image/texture and role-residency counts increase from three to
  four. CPU decoded bytes remain unique-image-based; GPU counts and bytes
  remain content-hash-and-role-based.
- The private bind group gains one sampled texture binding. The 496-byte draw
  uniform, vertex ABI, world schema, public observations, and dependencies are
  unchanged.

## Status

Accepted and implemented by CF058. Ordinary tests cover strict typed decode,
missing-coordinate rejection, exact shared/distinct CPU accounting, atomic
four-role GPU reservation, limit-minus-one rejection, and eviction. Controlled
release-mode Vulkan evidence covers sRGB RGB multiplication, ignored alpha,
white and zero-factor neutrality, unlit/directional/point composition, scene
override, unchanged non-color observations and logical causality, plus exact
four-role upload, eviction, and rehydration.
