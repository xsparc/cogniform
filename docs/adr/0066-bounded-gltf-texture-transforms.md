# ADR 0066: Apply bounded glTF texture transforms

- Status: Accepted
- Date: 2026-08-22
- Task: CF066

## Context

The imported-material path retained one finite primary coordinate set and four
independently sampled texture roles, but classified every
`KHR_texture_transform` marker as unsupported. Authors therefore could not use
the ratified glTF mechanism for offset, rotation, or scale without baking
different coordinates into geometry. Normal-textured primitives also need one
consistent interpretation: generated default tangents must be derived from the
coordinates actually used by the normal texture, while an explicit source
tangent remains authored data.

The extension can override the coordinate set and permits future properties.
Cogniform still retains only `TEXCOORD_0`, so accepting those wider forms as if
they were the supported subset would silently render different content. Finite
extension inputs can also overflow during f32 affine evaluation and must not
reach a shader as non-finite uniform or sampling data.

## Decision

Recognize ratified `KHR_texture_transform` declarations and markers on the
existing base-color, metallic-roughness, normal, and emissive texture-info
roles. Accept only:

- omitted or finite two-component `offset`, defaulting to `[0, 0]`;
- omitted or finite `rotation` in radians, defaulting to zero;
- omitted or finite two-component `scale`, defaulting to `[1, 1]`; and
- omitted or zero extension `texCoord`, preserving the existing requirement
  for core `texCoord` to be omitted or zero.

Apply the Khronos `translation * rotation * scale` order. Precompute two padded
affine rows per active texture role in immutable `AssetMaterial` metadata. An
omitted marker retains exact identity. A nonzero coordinate override or any
otherwise well-formed wider property remains an explicit
`UnsupportedExtension` candidate; malformed payloads, undeclared markers, and
non-finite values reject as `InvalidJson` before proxy classification. Validate
the complete affine evaluation against every expanded primary coordinate for
every active role. Any non-finite product or sum rejects as `InvalidTexcoord`
at `glb.decoded.texture_transform` before immutable asset adoption.

Append eight `vec4` affine rows after the exact prior 496-byte per-draw uniform
prefix, producing one fixed 624-byte layout. Transform the primary coordinate
independently at each of the four shader sampling sites. Inactive roles use
identity; imported unlit materials retain only the active base-color transform,
and an explicit scene material disables every imported role and transform.

When default MikkTSpace tangents are generated, use the transformed normal-role
coordinates both for the fixed pre-library weld-key work guard and the
generator input. Preserve the retained `AssetVertex::texcoord_0` and all valid
explicit source tangents exactly. Reject a transformed normal coordinate that
makes generation unsuitable through the existing no-partial-admission path.

Do not add another coordinate set, occlusion, image formats, mip levels,
compression, anisotropy, resource counts, texture bytes, pipelines, logical
scene state, hashing, replay, protocol, persistence, release, or deployment
authority.

## Consequences

- The four existing imported roles can select different bounded affine
  mappings from one shared `TEXCOORD_0` stream and one shared image.
- `AssetTextureTransform` and additive per-role `AssetMaterial` accessors expose
  immutable precomputed rows without exposing an unchecked constructor.
- Asset CPU/GPU byte and resource accounting, the 64-byte vertex layout, the
  nine-entry bind group, the 36-sampler table, and two fixed pipelines remain
  unchanged. Only the private draw uniform grows from 496 to 624 bytes while
  preserving the complete old prefix.
- Texture transforms are visual asset metadata only. They do not enter the
  world, logical hash, replay, observation schema, recovery point, or asset
  source identity beyond the existing exact source hash.

## Status

Accepted and implemented by CF066. Focused importer tests cover defaults,
independent affine rows, required declarations, malformed and undeclared
payloads, wider-coordinate/property proxy handling after malformed peers,
finite-evaluation overflow, unchanged resource accounting, transformed
generated tangents, unchanged retained coordinates, and exact explicit source
tangents. A controlled Vulkan comparison applies independent scale, rotation,
and translation across all four roles in one shared image and matches four
one-texel references exactly.
