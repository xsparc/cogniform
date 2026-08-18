# ADR 0056: Render bounded glTF metallic-roughness textures

- Status: Accepted
- Date: 2026-08-15
- Task: CF056

## Context

The strict GLB path retained numeric metallic and roughness factors plus
bounded base-color and normal textures, but rejected glTF's packed
`metallicRoughnessTexture`. The glTF contract treats this texture as linear
data: green supplies perceptual roughness, blue supplies metallic, and each
sample multiplies its corresponding numeric factor. Red and alpha have no
material meaning.

Admitting the field requires more than a shader binding. Decode must preserve
the role without double-counting a source image shared with another role,
renderer admission must reserve as many as three role resources atomically,
and the role must use linear rather than sRGB sampling. Existing unlit,
observation, logical-state, and explicit scene-material semantics must remain
unchanged.

## Decision

Extend the approved GLB subset with one optional shared-per-role
`pbrMetallicRoughness.metallicRoughnessTexture` index:

- accept only omitted or zero `texCoord` and require `TEXCOORD_0` on every
  referencing primitive;
- retain at most three root texture and image records across base-color,
  metallic-roughness, and normal roles. Decode each referenced source image
  once for aggregate CPU accounting even when several roles share it;
- carry explicit role metadata and immutable texels through `AssetMaterial`,
  `DecodedAsset`, and `AssetUploadJob`;
- key GPU residency by content hash and role, upload metallic-roughness as
  linear `Rgba8Unorm`, and atomically reserve all zero-to-three missing role
  resources before queue mutation;
- bind a renderer-owned `[255, 255, 255, 255]` linear fallback. Sample green
  and blue, multiply the imported roughness and metallic factors respectively,
  and ignore red and alpha;
- apply the sampled values only inside the existing directional and point
  direct-light response. Preserve exact unlit base color, depth, identity,
  background, geometric-normal observations, logical revision, replay, and
  world hash;
- disable base-color, metallic-roughness, and normal imported roles whenever
  an explicit scene `MaterialComponent` overrides the imported material.

Keep the existing PNG decoder, fixed sampler, per-image and aggregate bounds,
explicit CPU/GPU processing, exact-hash eviction/rehydration, and local trust
boundary. Add no dependency, sampler, UV set, image format, mipmap, occlusion,
emissive, alpha, ambient/image-based lighting, protocol, persistence, or
release authority.

## Consequences

- Approved GLBs can modulate both direct-light BRDF factors with one bounded
  packed texture while all non-color observations keep their prior semantics.
- CPU decoded bytes describe unique source images; GPU texture statistics
  describe unique content-hash-and-role resources. One shared four-byte image
  may therefore count as four CPU bytes and twelve GPU role bytes.
- Public Rust adds additive
  `AssetMaterial::has_metallic_roughness_texture` and
  `AssetUploadJob::metallic_roughness_texture` accessors. Existing public
  structs remain externally non-constructible through private fields, and no
  exhaustive public enum changes.
- Occlusion remains unsupported because glTF applies it to indirect lighting,
  which Cogniform does not implement. Emissive intensity/exposure policy also
  remains a separate decision.

## Status

Accepted and implemented by CF056. CPU tests cover typed role decode,
malformed coordinates and resource limits, unique shared-image accounting,
exact aggregate boundaries, and atomic three-role reservation. A controlled
release-mode GPU test proves linear G/B factor multiplication for directional
and point lights, R/A irrelevance, neutral fallback and explicit override
semantics, exact eviction/rehydration, and unchanged depth, identity,
background, and geometric-normal observations.
