# ADR 0068: Decode bounded glTF mesh quantization into the fixed vertex ABI

- Status: Accepted
- Date: 2026-08-23
- Task: CF068

## Context

Cogniform's GLB path retained only floating-point position, normal, tangent,
and primary texture-coordinate sources even though core glTF permits normalized
unsigned integer texture coordinates and the ratified
`KHR_mesh_quantization` extension permits a bounded set of integer vertex
attributes. Requiring authors to rewrite those sources as f32 increases source
size without improving Cogniform's immutable decoded or GPU representation.

The extension also permits scene-node transforms to dequantize positions and
can be combined with wider glTF features that Cogniform does not implement.
Accepting its name as a claim of full conformance would therefore be
misleading. Integer normalization, alignment, accessor bounds, and complete
source validation must also be explicit so malformed or adversarial source
data cannot hide behind indexed expansion or proxy fallback.

## Decision

Add core support for normalized unsigned-byte and unsigned-short `VEC2`
`TEXCOORD_0`. Recognize `KHR_mesh_quantization` in the existing strict,
unique, required-subset extension preflight and require it in both
`extensionsUsed` and `extensionsRequired` whenever a selected accessor uses
one of these extension-only formats:

- `POSITION` `VEC3`: signed byte, unsigned byte, signed short, or unsigned
  short, normalized or unnormalized;
- `NORMAL` `VEC3`: normalized signed byte or signed short;
- `TANGENT` `VEC4`: normalized signed byte or signed short; and
- `TEXCOORD_0` `VEC2`: signed byte or signed short, normalized or
  unnormalized, or unnormalized unsigned byte or unsigned short.

Decode normalized components with the Khronos equations: unsigned values are
divided by their component maximum, while signed values are the greater of
`c / 127` or `-1` for bytes and `c / 32767` or `-1` for shorts. Normalize
normal and tangent XYZ from the decoded f64 values before the existing finite
f32 materialization. Tangent W must decode exactly to `-1` or `1`.

Require every extension-format accessor start and element stride to be four-
byte aligned. Core normalized primary coordinates retain component-aligned
tight packing, while an explicit vertex stride remains four-byte aligned.
Keep checked ranges and the existing 252-byte stride ceiling. Cap the
source count of every selected position, normal, tangent, primary-coordinate,
and color accessor by `max_vertices_per_mesh` before scanning its complete
source. Validate every present selected-attribute `min` or `max` against the
actual raw component extrema; the accessor's normalization flag does not
change those extrema. Integer bounds must be integral and within their source
type, and floating bounds are compared after f32 rounding.

Quantized positions require both exact `min` and `max`. Preserve the earlier
Cogniform subset's acceptance of finite float positions without bounds for
compatibility, even though core glTF requires POSITION bounds. This is an
intentional subset compatibility deviation, not a broader conformance claim.

Always materialize the existing 64-byte `AssetVertex`. Do not retain packed
source components in decoded state or GPU residency. Do not add nodes, scenes,
node-based dequantization transforms, morph targets, skinning, animation,
sparse accessors, additional coordinate sets, compression, materials, images,
textures, dependencies, protocol, persistence, release, or deployment
authority.

## Consequences

- Core normalized primary coordinates and the admitted extension formats feed
  the same immutable decoded/upload path, texture transforms, MikkTSpace
  generation, renderer bindings, shaders, observations, lifecycle, and replay
  semantics as equivalent f32 sources.
- The decoded and GPU vertex stride, attribute locations, mesh byte
  accounting, renderer capability checks, and public Rust ABI remain
  unchanged.
- Extension-only formats with a missing or merely used marker reject as
  malformed input. Invalid bounds, non-finite decoded values, invalid tangent
  handedness, over-limit source counts, and malformed ranges cannot proxy.
- A file that relies on node transforms for dequantization remains outside the
  supported subset even when its accessors are otherwise admitted.

## Status

Accepted and implemented by CF068. Focused importer tests cover the complete
admitted component/normalization matrix, signed-min and unsigned-max equations,
required declarations, exact raw bounds, four-byte alignment, source-count
limits, and unchanged 64-byte accounting. Existing workspace and renderer
tests protect the shared decoded/GPU path.
