# ADR 0061: Render bounded glTF unlit materials

- Status: Accepted
- Date: 2026-08-19
- Task: CF061

## Context

The strict GLB importer classified every extension declaration and nested
extension object as unsupported. That excluded the ratified
`KHR_materials_unlit` material marker even though the renderer already had a
bounded base-color path and the extension requires no new texture role,
sampler, pipeline, or scene authority. Stylized and baked-light assets could
therefore be accepted only through a proxy or rendered through direct
metallic-roughness lighting that changed their authored color.

Accepting arbitrary extension objects or retaining attacker-controlled maps in
`AssetMaterial` would weaken the existing typed boundary. The supported marker
also must not hide malformed core materials or fallback resources, including
records whose values are visually ignored by unlit rendering.

## Decision

Extend the approved imported-material subset as follows:

- preflight optional top-level `extensionsUsed` and `extensionsRequired` as
  non-empty arrays of unique non-empty strings. Require every required name to
  appear in the used set and require every actual extension member, including
  `KHR_materials_unlit` and unknown payloads, to be declared in
  `extensionsUsed`;
- recognize only an exact empty `KHR_materials_unlit` object on a material.
  Retain the result in one bounded per-material boolean side table before the
  strict typed core decode. Null, scalar, array, undeclared, duplicate, empty-
  declaration, or otherwise inconsistent forms are malformed and never proxy;
- treat a non-empty unlit payload or a well-formed unknown extension as the
  existing unsupported/proxy classification, but return it only after every
  selected and unused core material, index, coordinate, image, texture, and
  fallback resource has passed strict validation;
- add non-exhaustive public `AssetShadingModel::{MetallicRoughness, Unlit}` and
  the read-only `AssetMaterial::shading_model` accessor.
  `AssetMaterial::new` remains source-compatible and defaults to
  `MetallicRoughness`; no arbitrary extension data crosses the asset boundary;
- reserve material-flag bit 4 in the existing 496-byte draw uniform. A selected
  imported unlit material without an explicit scene material renders only
  `baseColorFactor * baseColorTexture`, independently of directional or point
  lights. Metallic, roughness, normal, and emissive values and textures remain
  strictly decoded and lifecycle-accounted but are visually inert;
- keep imported OPAQUE/MASK coverage and single/double-sided policy active.
  An explicit scene `MaterialComponent` disables imported unlit selection and
  retains the ordinary scene metallic-roughness response.

Keep the exact 48-byte vertex, four role-separated texture resources, 496-byte
uniform, two fixed pipelines, bind layouts, stable draw order, revision,
logical hash, replay, eviction, and rehydration contracts. Do not add other
extensions, scene-authored unlit authority, samplers, mipmaps, additional UVs,
occlusion, BLEND, ambient/IBL, shadows, HDR, dependencies, protocol or world
changes, or release authority.

## Consequences

- `AssetShadingModel` and `AssetMaterial::shading_model` are additive public
  API. The enum is non-exhaustive so downstream matches must retain a wildcard;
  all packages remain unpublished and non-publishable.
- Exact ratified unlit markers move from unsupported/proxy classification into
  the supported path. Wider payloads and other extensions retain explicit
  unsupported/proxy behavior after malformed-input exclusion.
- Retained normal, metallic-roughness, and emissive textures still consume the
  same bounded CPU and role-keyed GPU capacity even when their selected unlit
  draw binds renderer-owned neutral fallbacks. This preserves deterministic
  lifecycle accounting and avoids extension-dependent resource authority.
- Pipeline count, vertex bytes, uniform bytes, bind groups, attachments,
  dependencies, schemas, packages, workflows, tags, and release state do not
  change. Rollback is a code-and-document revert with no persisted migration.

## Status

Accepted and implemented by CF061. Ordinary tests cover declaration shape,
uniqueness, subset and marker consistency, selected/unused material retention,
constructor defaults, malformed/proxy precedence, exact accounting, scene
selection, and combined material flag value `31`. Controlled optimized Vulkan
tests prove byte-identical sampled base color with no, directional, point, and
combined lights; four-role eviction and rehydration; explicit scene override;
OPAQUE/MASK and double-sided composition; face-oriented geometric normals; and
unchanged revision, logical hash, and idempotent replay. All prior optimized
asset and headless renderer probes pass together.
