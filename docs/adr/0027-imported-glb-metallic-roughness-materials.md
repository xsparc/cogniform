# ADR 0027: Imported GLB metallic-roughness materials

- Status: Accepted
- Date: 2026-08-06
- Task: CF027
- Refines: [ADR 0008](0008-content-addressed-assets-and-pure-built-in-procedures.md)
- Uses: [ADR 0026](0026-bounded-direct-metallic-roughness-response.md)

## Context

The approved GLB subset accepted `baseColorFactor` and retained it with each
decoded mesh. It also range-validated `metallicFactor` and `roughnessFactor`
but discarded both values. After CF026, the renderer has an existing bounded
direct-light path for all three numeric material inputs, so discarding two
validated source values makes materially different immutable assets render the
same unless a caller duplicates those values in world state.

Synthesizing a `MaterialComponent` during import would mutate or reinterpret
authoritative world state, alter hashing and replay semantics, and obscure the
existing rule that a scene material overrides an asset material. Adding a
second material buffer or shader path would duplicate the fixed per-draw
uniform without adding capability.

## Decision

One public immutable `AssetMaterial` value contains linear base RGBA, metallic,
and perceptual roughness as `UnitF32` values. A decoded mesh retains that value;
the existing `AssetUploadJob` carries it into renderer-owned residency, and
scene preparation copies it into the existing 480-byte draw uniform. The GPU
vertex payload, vertex format, shader, bind group, pipeline, and upload
scheduling do not change.

For an explicitly referenced GLB material, omitted `baseColorFactor`,
`metallicFactor`, and `roughnessFactor` use the glTF defaults of one. The
project's existing material-free asset fallback remains linear
`(0.8, 0.8, 0.8, 1.0)`, `metallic = 0`, `roughness = 0.8` for compatibility.
The unsupported-feature proxy remains magenta with the same neutral metallic
and roughness values. Every parsed channel and scalar is validated before a
decoded record becomes ready.

A resident asset supplies all three imported values only when its render
entity has no `MaterialComponent`. An explicit scene material replaces base
color, metallic, and roughness together. A missing resident asset still uses
only its explicit primitive fallback, whose existing default or scene material
behavior is unchanged. With no active light, the renderer continues to return
the selected material's exact base RGBA.

`AssetUploadJob::byte_len` and all pending/resident byte limits continue to
measure only the exact 24-byte position-plus-normal vertex payload. Material
metadata introduces no GPU buffer allocation and does not affect immutable
content identity: the source hash already covers the exact GLB bytes.

## Consequences

- Active direct lighting now distinguishes accepted GLB metallic and roughness
  factors without another world mutation or shader path.
- Existing explicit scene materials retain precedence and now override all
  three imported numeric values consistently.
- `AssetMaterial` and `AssetUploadJob::material` are additive public Rust API;
  the existing `AssetUploadJob::base_color` accessor remains available.
- A material that omitted metallic or roughness now follows the glTF default
  of one when lit. This is intentional source-format behavior in the still
  unpublished `0.0.0` workspace.
- Logical asset references, world snapshots, hashes, replay, recovery,
  persistence, rehydration, decoded/GPU byte accounting, and causal
  observations remain unchanged.
- Textures, images, samplers, UVs, tangents, normal maps, emissive and alpha
  material modes, image-based lighting, shadows, HDR, tone mapping, transport,
  deployment, and release publication remain separate decisions.

## Status

Implemented by CF027 with exact decoder/default/proxy and range-rejection
tests, upload/residency and scene-precedence tests, and a controlled
Windows/Vulkan GLB probe for exact unlit color plus distinct imported and
overridden direct-light response while depth, identity, normals, background,
and revision causality remain unchanged.
