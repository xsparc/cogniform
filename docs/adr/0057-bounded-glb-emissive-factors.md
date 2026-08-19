# ADR 0057: Render bounded core glTF emissive factors

- Status: Accepted
- Date: 2026-08-19
- Task: CF057

## Context

The strict GLB path retained bounded metallic-roughness values and three
texture roles, but rejected core `material.emissiveFactor`. Core glTF defines
this value as three unit-interval linear RGB multipliers with a zero default.
It is independent of direct-light material response and does not require an
emissive texture, exposure policy, HDR target, bloom, or indirect lighting.

Supporting the factor must preserve Cogniform's bounded color target and its
existing authority boundary. Imported material metadata may affect rendering,
but it must not create light entities, illuminate other surfaces, alter scene
state, or bypass an explicit scene `MaterialComponent`.

## Decision

Extend the approved GLB subset with optional core `emissiveFactor`:

- decode exactly three finite unit-interval linear RGB values and use
  `[0, 0, 0]` when the field or selected material is absent;
- reject wrong-length, wrong-type, non-finite, or out-of-range values before
  asset readiness, without proxy substitution;
- retain the validated value in `AssetMaterial` without changing decoded mesh,
  texture, upload, or residency accounting;
- use imported emission only when the entity has no explicit scene
  `MaterialComponent`; built-in, material-free, and proxy draws use zero;
- append one zero-padded emissive `vec4` to the private draw uniform, preserving
  its existing 480-byte prefix and producing an exact 496-byte layout;
- after the existing unlit base-color or directional/point direct-light path,
  add emissive RGB and clamp each channel to one. Preserve material alpha,
  depth, identity, background, and geometric-normal observations;
- keep asset processing explicit and logically neutral. Rendering, eviction,
  and rehydration do not advance world revision, alter logical hash, or change
  idempotent replay.

Do not add `emissiveTexture`, `KHR_materials_emissive_strength`, illumination
of other entities, ambient or image-based lighting, HDR, exposure, tone
mapping, bloom, new texture/image/sampler authority, protocol fields,
dependencies, persistence, or release authority.

## Consequences

- Approved GLBs can display bounded self-emission under zero, directional, or
  point lights while retaining the existing RGBA8 output contract.
- `AssetMaterial::emissive` is an additive public accessor. The existing
  public `AssetMaterial::new` constructor remains `const`, keeps its signature,
  and creates zero emission.
- The renderer's private uniform grows by one aligned vector; its prior prefix
  and all public observation encodings are unchanged.
- Emission is a surface-color term only. It grants no light, shadow, exposure,
  or cross-entity authority.

## Status

Accepted and implemented by CF057. CPU tests cover defaults, exact retention,
unchanged accounting, malformed values, proxy defaults, scene override, and
the exact uniform append. A controlled release-mode GPU test covers unlit,
directional, point, clamp, alpha, revision/hash/replay, and unchanged
depth/identity/background/geometric-normal behavior.
