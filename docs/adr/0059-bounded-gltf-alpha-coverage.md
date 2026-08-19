# ADR 0059: Render bounded glTF alpha coverage

- Status: Accepted
- Date: 2026-08-19
- Task: CF059

## Context

The strict GLB path retained base-color factor and texture alpha but rejected
core `alphaMode` and `alphaCutoff`. That left imported opaque materials writing
source alpha into observations even though glTF defines opaque coverage as
fully opaque. Core mask coverage is deterministic without draw sorting:
multiply base-color factor alpha by sampled base-color texture alpha, discard
values below the cutoff, and keep equality.

Blend coverage requires ordering and pipeline blending authority that the
headless baseline does not yet own. Alpha coverage must also remain subordinate
to the existing explicit scene-material override and must not mutate logical
state or widen texture residency.

## Decision

Extend the approved imported-material subset as follows:

- strictly decode optional string `alphaMode`, using the glTF default `OPAQUE`,
  and support only `OPAQUE` and `MASK`;
- strictly decode optional finite non-negative `alphaCutoff`, require an
  explicit `alphaMode` when it is present, and use `0.5` when a mask omits it.
  Values above one are valid and deterministically discard all bounded alpha;
- classify `BLEND` and other well-formed wider string modes as unsupported and
  proxy-eligible only under the explicit proxy policy. Malformed or over-limit
  peer data is still validated first and cannot be hidden by that diagnosis;
- retain typed `AssetAlphaMode` and the mask cutoff in `AssetMaterial` without
  changing `AssetMaterial::new`, decoded-byte accounting, upload jobs, or
  texture roles;
- for an imported material, make opaque coverage ignore factor and texture
  alpha and emit alpha one. For a mask, compare factor alpha multiplied by the
  sampled base-color alpha, discard only when the product is below the cutoff,
  and emit surviving fragments with alpha one;
- perform the mask discard before color, depth, entity-ID, or normal outputs.
  An explicit scene `MaterialComponent` disables imported alpha coverage and
  retains the existing scene alpha behavior;
- preserve the private 496-byte draw uniform by storing exact alpha flags with
  the existing normal flag and the cutoff in the prior emissive padding lane.

Do not add blending, draw sorting, alpha-to-coverage, MSAA, double-sided
materials, occlusion, ambient/image-based lighting, new textures, samplers,
coordinates, image formats, world/protocol/recovery/transport changes,
dependencies, or release authority.

## Consequences

- `AssetAlphaMode`, `AssetMaterial::alpha_mode`, and
  `AssetMaterial::alpha_cutoff` are additive public API. The enum is
  non-exhaustive and the existing constructor remains `const` with opaque
  coverage.
- Imported GLB opaque output intentionally changes from preserving imported
  base alpha to alpha one. Built-in draws and explicit scene materials retain
  their previous alpha semantics.
- Mask equality survives; a cutoff above one discards all fragments. Discarded
  fragments leave cleared color, depth, identity, and normal observations.
- Vertex, texture, bind-group, uniform-size, logical scene, observation schema,
  package, dependency, workflow, version, tag, and release contracts are
  unchanged.

## Status

Accepted and implemented by CF059. Ordinary tests cover typed defaults,
cutoffs, public retention, accounting neutrality, malformed and proxy
precedence, scene selection, and exact uniform encoding. Controlled optimized
Vulkan tests distinguish factor-only, texture-only, and multiplied alpha,
prove equality and cutoff-above-one behavior, opaque normalization, scene
override, all four observation outputs, and unchanged revision/hash/replay.
