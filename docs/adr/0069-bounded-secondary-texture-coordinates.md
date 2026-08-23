# ADR 0069: Retain one bounded secondary glTF texture-coordinate set

- Status: Accepted
- Date: 2026-08-23
- Task: CF069

## Context

Cogniform retained only `TEXCOORD_0` even though core glTF recommends support
for two consecutive coordinate sets and lets each texture-info role select a
set. The ratified `KHR_texture_transform` extension can override that core
selector. Treating selector one as a wider feature prevented otherwise bounded
assets from using independent coordinates for base color, metallic-roughness,
normal, and emissive sampling.

Secondary coordinates cross the importer, public decoded-vertex ABI, GPU
layout, shader, material metadata, generated-tangent input, byte accounting,
and capability preflight. Admission must also validate every declared
coordinate set before a wider feature can receive a proxy, so malformed
`TEXCOORD_2` data cannot hide behind unsupported-feature classification.

## Decision

Accept optional consecutive `TEXCOORD_1` only after `TEXCOORD_0`, with the same
count and the same core or required-`KHR_mesh_quantization` formats admitted
for the primary set. Coordinate semantic suffixes must be canonical unsigned
decimal values without gaps or leading zeroes. Validate the complete bounded
source, accessor range, raw bounds, decoded finiteness, and position count of
every declared set before classifying `TEXCOORD_2` or later as unsupported.
Missing, skipped, mismatched, over-limit, or non-finite coordinate data rejects
without proxy substitution.

For base-color, metallic-roughness, normal, and emissive texture infos, retain
effective selector zero or one. An authored `KHR_texture_transform.texCoord`
overrides the corresponding core `TextureInfo.texCoord`; otherwise the core
selector or its zero default applies. Every selected set must be present on
the primitive. Each role applies its existing independent affine transform to
its selected set. Bounded MikkTSpace generation uses the selected transformed
normal-role coordinates; explicit tangents and both retained coordinate sets
remain authored values.

Append `AssetVertex::texcoord_1` after the exact accepted 64-byte vertex prefix
and change `ASSET_VERTEX_BYTES` to 72. Omitted secondary coordinates, built-in
geometry, and proxies use exact zero. Add shader attribute location five at
offset 64 and raise capability preflight to six attributes and a 72-byte
stride. Encode four selector bits in bits 6 through 9 of the existing exactly
representable material flag value. Keep the 624-byte draw uniform, nine-entry
bind group, 36-sampler table, two render pipelines, and all texture formats
unchanged.

Do not add `TEXCOORD_2` rendering, generated coordinates, occlusion, new image
or texture formats, mip storage, morph targets, skinning, animation, nodes,
scenes, mesh compression, protocol or world state, persistence, dependency,
workflow, release, deployment, or publication authority.

## Consequences

- A public decoded vertex gains one appended field and its exact decoded/GPU
  byte count grows from 64 to 72. This is an intentional additive and layout
  change within the unpublished `0.1.0-rc.1` source candidate; package versions
  do not change.
- Core and extension selectors compose independently across all four existing
  roles without adding a buffer, binding, sampler, texture, or pipeline.
- Existing assets preserve their first 64 encoded bytes and receive a zero
  secondary attribute. Logical world revisions, hashes, replay, observations,
  eviction, and explicit exact-hash rehydration remain unchanged.
- Valid consecutive wider coordinate sets remain unsupported and may follow
  the explicit proxy policy only after their complete sources validate.

## Status

Accepted and implemented by CF069. Importer tests cover float, core-normalized,
and ratified quantized secondary sources; canonical sequencing; complete wider-
set validation; core/extension precedence; selected-set presence; independent
transforms; generated tangents; zero fallbacks; and exact 72-byte accounting.
Renderer ABI tests protect the appended attribute and unchanged resource
topology. Controlled Windows/Vulkan tests prove independent four-role
selection and transforms plus exact-hash rehydration without revision, logical
hash, replay, depth, identity, or geometric-normal changes.
