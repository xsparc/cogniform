# ADR 0055: Render bounded source-tangent normal textures

- Status: Accepted
- Date: 2026-08-14
- Task: CF055

## Context

The strict GLB path already retained finite positions, optional source normals,
one primary texture-coordinate set, numeric metallic-roughness factors, and one
embedded PNG base-color texture. Direct lighting nevertheless used only the
geometric vertex normal. The glTF 2.0 normal-texture contract depends on a
source `TANGENT` `VEC4`, where XYZ defines the tangent direction and W is the
bitangent sign; a normal map must be sampled as linear data rather than sRGB.

Admitting the material field alone would cross several existing boundaries.
Tangents must be validated and accounted in the CPU/GPU vertex ABI, a second
image role must remain bounded and format-correct, renderer reservation and
eviction must be exact for both roles, non-uniform transforms must not create
an invalid basis, and the existing geometric-normal observation cannot silently
become a shaded-normal output. Generating missing tangents would also require a
separate MikkTSpace policy and potentially change mesh topology, so it is not
part of this slice.

## Decision

Extend the strict GLB subset with one optional source-tangent normal-texture
role:

- accept optional non-normalized f32 `TANGENT` `VEC4` accessors with the same
  source count as `POSITION`; validate the complete accessor, require finite
  non-zero XYZ, normalize XYZ deterministically, require W to be exactly `-1`
  or `1`, and reject mixed W signs inside one expanded triangle;
- accept at most one `material.normalTexture` texture index across the asset,
  with omitted or zero `texCoord`, finite `scale` defaulting to one, and a
  primitive that supplies source `NORMAL`, `TANGENT`, and `TEXCOORD_0`;
- retain at most two root textures and two referenced embedded `image/png`
  images across the existing base-color and normal roles. Unused table entries,
  custom samplers, external images, and role-inconsistent references reject.
  An image shared by both roles is decoded and counted once in CPU residency;
- retain explicit base-color and normal texture fields in decoded assets and
  upload jobs. The renderer keys GPU residency by content hash and role,
  uploads base color as `Rgba8UnormSrgb`, uploads normals as linear
  `Rgba8Unorm`, and atomically reserves all new role resources before queue
  mutation. Shared source pixels still require two GPU resources because the
  formats have different transfer behavior;
- expand the fixed interleaved vertex ABI from 32 to 48 bytes. The prior
  position/normal/`TEXCOORD_0` 32-byte prefix remains unchanged and the new
  tangent occupies shader location 3. Missing tangents, built-ins, and proxy
  geometry use `[1, 0, 0, 1]` without enabling normal sampling;
- transform tangents by the model linear matrix, reject their interpolated
  normal component through Gram-Schmidt against the geometric transformed
  normal, preserve source handedness with the model determinant sign, and
  fail back to the geometric direction for a degenerate sampled or transformed
  basis. Apply `normalTexture.scale` to sampled XY before normalization;
- use the perturbed normal only for directional and point direct-material
  response. Unlit color, depth, stable identity, and the normal observation
  attachment retain their previous geometric semantics. An explicit scene
  material overrides the imported material as a whole and disables both
  imported texture roles.

Keep the existing PNG decoder, sampler, per-image bounds, aggregate CPU/GPU
byte and count limits, explicit processing, exact-hash eviction/rehydration,
and local trust boundary. Add no dependency, generated tangent algorithm,
additional UV set, texture transform, mipmap, compression, image format,
material role, transport, persistence, or release authority.

## Consequences

- Approved GLBs can affect direct lighting through one bounded tangent-space
  normal map while observations continue to describe geometric orientation.
- A legacy three-vertex mesh now reserves 144 rather than 96 vertex bytes;
  configured limits continue to apply to the exact expanded requirement.
- `AssetUploadOutcome` and renderer texture statistics retain their public
  shape but texture counts now mean unique content-hash-and-role resources and
  uploaded bytes are aggregate across roles.
- Public Rust adds the required `AssetVertex::tangent` field,
  `AssetMaterial::{has_normal_texture, normal_scale}`,
  `AssetUploadJob::normal_texture`, `AssetDiagnosticCode::InvalidTangent`, and
  `RenderTargetKind::AssetNormal`; `ASSET_VERTEX_BYTES` changes to 48. The
  `AssetVertex` field and exhaustive renderer enum variant are source-breaking
  in this still-unpublished candidate workspace.
- Models without source tangents remain valid when they do not reference a
  normal texture. Normal-textured models without the required source basis are
  valid-but-unsupported and may use only the already-explicit proxy policy.
- Generated MikkTSpace tangents, multiple textures per role, other material
  textures, alpha modes, broader samplers, and geometric-normal observation
  changes require separate approved decisions.

## Status

Accepted and implemented by CF055. CPU tests cover valid, indexed and malformed
tangent/role input, shared-versus-distinct image accounting, exact limits,
proxy eligibility, 48-byte encoding, and atomic two-role reservations. A
controlled GPU test proves linear normal sampling changes direct-light color,
normal scale suppresses XY perturbation, alpha is ignored, and depth, identity,
background, and geometric-normal observations remain unchanged.
