# ADR 0028: Bounded primary texture coordinates

- Status: Accepted
- Date: 2026-08-06
- Task: CF028
- Refines: [ADR 0008](0008-content-addressed-assets-and-pure-built-in-procedures.md)
- Follows: [ADR 0027](0027-imported-glb-metallic-roughness-materials.md)

## Context

The approved GLB subset retains geometry, vertex normals, and numeric material
factors, but discards every texture-coordinate attribute. Retaining a single
primary coordinate set is the smallest asset/renderer ABI step toward a future
texture decision. It must not imply image decoding, sampling, or any visual
change, and malformed optional data must not bypass the importer's existing
fail-closed boundary merely because the shader does not yet use it.

## Decision

An accepted primitive may add `TEXCOORD_0` to `POSITION` and optional `NORMAL`.
The accessor must be a non-normalized f32 `VEC2` with exactly the position
accessor's source count. Both components must be finite. Values are retained
bit-for-bit as finite f32 values; coordinates outside the unit interval are
valid because future wrap behavior belongs to a sampler contract, not import.
Indexed geometry expands position, normal, and primary coordinates through the
same checked source index.

The importer validates every source coordinate before allocating the expanded
vertex vector, including coordinates not selected by an index stream. A
non-finite value, count mismatch, or invalid range is malformed and never
receives a proxy. A syntactically valid normalized or differently encoded
accessor is `UnsupportedAccessor` and follows the caller's explicit proxy
policy.

`AssetVertex` and the renderer use one exact 32-byte interleaved layout:
position f32 `VEC3`, unit normal f32 `VEC3`, then primary coordinate f32
`VEC2`. The shader input reserves location 2 but does not use it. Assets without
`TEXCOORD_0`, all built-in geometry, and the unsupported-feature cuboid proxy
store exact zero coordinates. Decoded, pending-upload, per-mesh, and resident
byte accounting all use the same public 32-byte constant.

## Consequences

- One primary coordinate set survives immutable decode, upload, and renderer
  residency without changing current color, depth, identity, normal, hash,
  replay, recovery, or causal-observation behavior.
- The ABI and built-in payloads grow from 24 to 32 bytes per vertex: cuboid
  1,152 bytes, plane 192 bytes, and sphere 21,504 bytes.
- Exact zero defaults keep assets without coordinates and procedural geometry
  deterministic while avoiding a second vertex pipeline.
- Images, textures, samplers, `baseColorTexture`, transforms, other coordinate
  sets or encodings, tangents, normal maps, shader sampling, schema/world
  material changes, persistence, transport, deployment, and release remain
  separate decisions.

## Status

Implemented by CF028 with exact and indexed retention, zero-default,
non-finite/count/range rejection, unsupported-encoding proxy, byte-accounting,
vertex-layout, built-in payload, and controlled render-equivalence tests.
