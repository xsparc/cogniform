# ADR 0060: Render bounded glTF double-sided materials

- Status: Accepted
- Date: 2026-08-19
- Task: CF060

## Context

The imported-material path rendered every triangle through one unculled
pipeline and classified core glTF `material.doubleSided` as unsupported. This
made an omitted or explicit-false material visible from its back, contrary to
the glTF default, and prevented an explicit double-sided material from using
the supported asset path. Back faces that remain visible also need their
geometric and shaded normals oriented toward the rendered side before direct
lighting and normal observations are produced.

An asset-keyed pipeline cache or draw reordering would add unnecessary
resource and ordering authority: only culled and unculled triangle states are
required. The correction must preserve the existing fixed uniform, texture
roles, alpha coverage, explicit scene-material precedence, and logical
causality.

## Decision

Extend the approved imported-material subset as follows:

- strictly decode optional boolean `doubleSided` for every material, including
  unused records. Omission and explicit `false` retain false; explicit `true`
  retains true; null and every non-boolean form are malformed and never proxy;
- retain the value privately in `AssetMaterial`, keep `AssetMaterial::new`
  source-compatible with false, and expose the additive read-only
  `AssetMaterial::double_sided` accessor without changing byte accounting;
- derive one private draw policy from material selection:
  `SingleSided` for an imported false material, `DoubleSided` for an imported
  true material, and `Disabled` for built-ins, unresolved authored primitive
  fallbacks, or an explicit scene `MaterialComponent`;
- create exactly two renderer-owned pipelines at initialization from the same
  shader, layouts, attachments, and depth state: one CCW pipeline that culls
  back faces and one CCW unculled pipeline. Select between them for each draw
  in stable entity order; do not cache by asset or reorder draws;
- reserve material-flag bit 3 in the existing 496-byte draw uniform. For an
  imported double-sided back face, reverse the completed geometric normal and
  the completed tangent-mapped shaded normal before observation output and
  lighting. Do not apply this correction to built-ins or scene overrides;
- keep MASK discard before every attachment output and preserve OPAQUE/MASK,
  four texture roles, vertex layout, bind groups, revision, logical hash,
  replay, recovery, and explicit asset lifecycle contracts.

Generated magenta proxy cuboids retain `AssetMaterial::new` and therefore use
the imported single-sided default. Their generated topology and accounting are
unchanged; this intentional culling consequence is explicit rather than
overloading `doubleSided` with proxy provenance.

Do not add BLEND, sorting, blending, MSAA, alpha-to-coverage, mirrored or
negative transforms, generated tangents, new samplers/images/coordinates,
ambient or image-based lighting, world/protocol/recovery/transport changes,
dependencies, or release authority.

## Consequences

- `AssetMaterial::double_sided` is additive public API. Valid explicit
  `doubleSided` moves from unsupported/proxy classification into the supported
  core path.
- Imported omitted/false back faces intentionally stop writing color, depth,
  identity, normal, or derived visibility. Imported true back faces render and
  report a face-oriented geometric normal.
- Built-ins, authored primitive fallbacks, and explicit scene materials retain
  their prior unculled, unflipped behavior.
- Pipeline count increases from one to exactly two fixed instances. No
  attacker-controlled pipeline cardinality, per-frame creation, draw sorting,
  uniform growth, dependency, schema, package, workflow, tag, or release
  change is introduced.
- Rollback is a code-and-document revert. No persisted state needs migration.

## Status

Accepted and implemented by CF060. Ordinary tests cover strict selected and
unused material decoding, accounting neutrality, malformed/proxy precedence,
all private draw policies, explicit override/fallback behavior, and exact
496-byte combined flags. Controlled optimized Vulkan tests use positive-scale
180-degree Y rotations to prove per-draw cull/uncull/cull transitions, stable
identity and visibility, face-oriented geometric normals, normal-map lighting,
MASK discard/equality, built-in and scene-override compatibility, and unchanged
eviction, revision, logical hash, and idempotent replay.
