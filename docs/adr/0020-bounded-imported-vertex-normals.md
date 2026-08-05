# ADR 0020: Bounded imported vertex normals

- Status: Accepted
- Date: 2026-08-05
- Task: CF020
- Supersedes: the flat-only normal-source decision in [ADR 0011](0011-quantized-world-space-normal-observations.md)

## Context

ADR 0011 deliberately derived flat normals from rasterized positions so the
first normal observation did not also broaden the asset contract. The strict
GLB importer, immutable upload boundary, GPU residency accounting, and
controlled adapter fixture are now established. Smooth surface orientation is
the next bounded visual input, but textures, tangent space, scene traversal,
and material lighting would multiply the parser and renderer trust surface.

A normal attribute cannot be treated as unvalidated decoration. Its accessor
range and source count affect indexed expansion, zero or non-finite vectors
cannot produce a valid direction, and adding three f32 values changes every
decoded and GPU vertex from 12 to 24 bytes.

## Decision

One accepted triangle primitive contains exactly `POSITION`, or `POSITION`
plus `NORMAL`. `NORMAL` uses a non-normalized f32 `VEC3` accessor with the same
source count as `POSITION`. Sparse, integer, and normalized encodings remain
unsupported. The importer validates the accessor before output allocation,
rejects invalid ranges, mismatched counts, non-finite values, and zero-length
directions, and normalizes accepted values deterministically.

Indexed geometry expands positions and normals with the same source index.
When `NORMAL` is absent, the importer computes one unit cross-product direction
from each expanded triangle's winding and assigns it to all three vertices. A
degenerate triangle cannot synthesize a direction and is rejected. Invalid
normal data and degenerate fallback geometry never qualify for a proxy;
syntactically valid but unsupported accessor encodings continue to follow the
explicit unsupported-feature policy.

`AssetVertex` and `AssetUploadJob` carry interleaved position and normal data.
CPU decode and renderer admission reserve exactly 24 bytes per expanded vertex
before allocation or upload. The built-in cube uses the same layout with a
winding-derived normal repeated per face.

The vertex shader transforms normals with the inverse transpose of the model's
linear 3x3 transform. The fragment shader normalizes the interpolated direction
and writes the existing signed `Rgba8Unorm` world-space normal target. The
observation payload, quantization, background marker, readback bounds,
causality, and 0.99 controlled dot-product tolerance do not change.

## Consequences

- Position-only GLBs and the built-in cube preserve flat winding-derived
  output, while valid source normals can produce smooth interpolated output.
- The larger exact vertex size reduces how many vertices fit under unchanged
  decoded and GPU byte limits; limits remain caller-configurable and are not
  silently raised.
- `AssetVertex` gains a public field and `AssetUploadJob::byte_len` doubles for
  the same expanded vertex count. This is source-breaking for Rust struct
  literals and observable to capacity planning. Packages are unpublished at
  `0.0.0`, so CF020 performs no version, tag, or release action.
- UVs, tangents, colors, textures, images, normal maps, material lighting,
  multiple primitives, skinning, morph targets, nodes/scenes, compression, and
  higher-precision normal targets remain separate decisions.

## Status

Accepted and implemented by CF020. Controlled DX12/Vulkan-profile tests must
continue to cover the position-only fallback, built-in cube, imported normal,
and non-uniform model-transform paths.
