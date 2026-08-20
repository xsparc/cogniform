# ADR 0063: Render bounded core glTF vertex colors

- Status: Accepted
- Date: 2026-08-20
- Task: CF063

## Context

The approved imported vertex path retained positions, normals, one primary
coordinate, and one source tangent, but classified every color attribute as
unsupported. Core glTF defines `COLOR_0` as an optional linear multiplier of
the material base color. Supporting it requires an exact CPU/GPU ABI change,
strict handling of normalized integer accessors, and explicit precedence over
otherwise proxy-eligible wider attributes.

Color input is adversarial. Non-finite floats, invalid normalization, malformed
set names, count mismatches, misaligned ranges, or a wider set must not bypass
the existing fail-closed import and capacity rules. An explicit scene material
must also retain complete authority over the imported material rather than
silently inheriting vertex color.

## Decision

Extend the strict imported vertex subset as follows:

- accept optional same-count `COLOR_0` as `VEC3` or `VEC4` with non-normalized
  f32, normalized unsigned byte, or normalized unsigned short components;
- require finite f32 components and clamp each finite source component to the
  core `[0, 1]` color range. Expand normalized integers exactly into that same
  range and synthesize alpha one for `VEC3`;
- validate every declared color set before any unsupported/proxy result.
  Bound each primitive to at most sixteen attribute semantics and validate an
  aliased color accessor only once, so wider-set validation work remains
  bounded by both the semantic and accessor limits.
  Malformed names, a missing primary set, skipped or malformed sets, invalid
  types, counts, normalization, or values use the new non-proxyable
  `InvalidColor` diagnostic; invalid binary ranges retain the existing
  non-proxyable range diagnostic. A valid wider `COLOR_1` or later set remains
  an explicit unsupported/proxy candidate only after hard validation;
- reject missing `POSITION`, multiple primitives, and attribute-semantic limit
  failures without proxy substitution;
- apply `max_asset_decoded_bytes` to generated proxy geometry as well as
  decoded source geometry before either can enter CPU residency;
- append four unit color components to `AssetVertex`, preserving the previous
  48-byte position/normal/coordinate/tangent prefix and growing the exact
  decoded and GPU stride to 64 bytes. Omitted colors, built-ins, and proxy
  vertices use exact white;
- append shader location 4 and multiply interpolated linear vertex RGBA with
  sampled base RGBA and the imported base-color factor before imported lit,
  unlit, OPAQUE, and MASK behavior. OPAQUE still emits alpha one. Emission and
  geometric-normal observations remain independent;
- enable the imported color only for a resident asset with no explicit scene
  `MaterialComponent`. Scene material, built-in, and authored fallback paths
  bind white behavior through one existing uniform flag bit;
- preflight at least five vertex attributes and a 64-byte vertex-buffer stride
  before device creation.

Keep the 496-byte uniform, four texture roles, nine-entry bind group, fixed
36-sampler table, two face pipelines, stable draw order, observations,
eviction/rehydration, revision, logical hash, and replay contracts. Do not add
`COLOR_1` rendering, morph colors, additional coordinates, texture transforms,
BLEND, occlusion/ambient/IBL, HDR, dependencies, protocol/world changes, or
release authority.

## Consequences

- `AssetVertex` gains one public field and `ASSET_VERTEX_BYTES` changes from 48
  to 64. This is source-breaking for exhaustive Rust construction or matching
  and changes decoded/GPU capacity planning; packages remain unpublished and
  non-publishable.
- `AssetDiagnosticCode::InvalidColor` is an additive variant on a public
  non-exhaustive enum. Valid wider color sets continue through the established
  explicit unsupported policy.
- Built-in cube, plane, and sphere buffers grow proportionally because they
  carry exact white, while texture, sampler, pipeline, uniform, logical scene,
  and durable formats do not change.
- Rollback is a code-and-document revert with no persisted migration. Assets
  that rely on `COLOR_0` return to unsupported handling after rollback.

## Status

Accepted and implemented by CF063. Ordinary tests cover every admitted
component/type/shape, finite clamping, normalized expansion, VEC3 alpha,
indexed expansion, malformed and wider-set precedence, exact byte limits,
white fallback, scene override, and fixed layout. Controlled optimized Vulkan
tests prove interpolation, factor/texture/emissive composition, imported alpha
coverage, default material behavior, double-sided back faces, non-color
observation stability, lifecycle neutrality, and unchanged replay causality.
