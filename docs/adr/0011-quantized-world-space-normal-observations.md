# ADR 0011: Quantized world-space normal observations

- Status: Accepted; flat-only normal-source decision superseded by ADR 0020
- Date: 2026-08-04
- Task: CF011

## Context

The source-first MVP already returns color, normalized depth, exact stable
identity, and structured visibility with revision/frame/camera causality. The
design reserves surface normals as the next machine-readable geometry output.
The renderer currently uploads position-only expanded triangles, uses fixed
four-byte-per-pixel readback layouts, and must keep submission independent of
GPU mapping and observation consumers.

Three approaches were considered:

1. import smooth vertex normals and extend every asset/upload vertex format;
2. render a higher-precision `Rgba16Float` or `Rgba32Float` normal target; or
3. derive flat geometric normals from rasterized world positions and encode
   them in `Rgba8Unorm`.

Imported normals would broaden the GLB subset, immutable asset contract,
residency accounting, and smoothing semantics in the same change. Float
targets increase readback memory and adapter requirements before a consumer has
demonstrated that need. The existing position-only triangle stream already
contains enough information for a bounded flat geometric output.

## Decision

Add `normal` as an additive schema-v1 `ObservationKind`. The local engine
payload is `Vec<Option<[f32; 3]>>`: geometry pixels carry finite unit vectors
and background pixels are `None`. Normal metadata follows the same dimensions,
quality, latency, staleness, scene revision, frame, and camera rules as other
image observations. Visibility remains the only dimensionless kind.

The renderer adds a third color attachment using `Rgba8Unorm`. The fragment
shader derives a flat geometric world-space direction from derivatives of the
interpolated world position, corrects framebuffer derivative orientation to
follow source triangle winding, maps signed XYZ into RGB `[0, 1]`, and writes
alpha `1`. A transparent clear value marks background. Readback accepts only
alpha `0` or `255`, decodes signed RGB, rejects invalid directions, and
renormalizes after quantization.

Each existing fixed readback lease gains one normal buffer. Submission copies
color, depth, normal, and identity without mapping or waiting; the observation
worker maps all four under the existing deadline. Pool and observation
capacities are unchanged. Adapter negotiation requires three color attachments,
twelve color-attachment bytes per sample, and normal-target render/copy usage.

Controlled conformance compares direction using a minimum `0.99` dot product,
not byte identity. Cross-adapter bitwise normal equality is not promised.

## Consequences

- Machine clients can reason about visible geometric orientation with the same
  causal envelope as existing observations.
- Position-only primitives and the approved GLB subset need no asset, hash,
  upload, or residency migration.
- Readback memory grows by one four-byte-per-pixel buffer per configured lease,
  but remains fixed and validated before rendering.
- `ObservationKind::Normal` is additive on the JSON surface but requires Rust
  clients with exhaustive enum matches to update. Packages remain unpublished
  at `0.0.0`, so no release version or compatibility promise changes.
- Smooth imported normals, normal maps, tangent space, higher-precision normal
  targets, encoding, shared memory, remote delivery, and shading changes remain
  separate decisions.

The observation and quantization decision remains active. [ADR 0020](0020-bounded-imported-vertex-normals.md)
later supersedes only the flat-only geometry source and position-only upload
parts of this record.
