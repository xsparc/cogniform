# ADR 0029: Bounded embedded PNG base-color textures

- Status: Accepted
- Date: 2026-08-06
- Task: CF029
- Refines: [ADR 0008](0008-content-addressed-assets-and-pure-built-in-procedures.md)
- Follows: [ADR 0028](0028-bounded-primary-texture-coordinates.md)

## Context

The approved GLB subset retains primary texture coordinates and numeric
metallic-roughness factors, but cannot produce a textured observation. The
smallest useful next step is one base-color image that preserves the existing
content identity, explicit scheduling, renderer ownership, recovery, and
bounded direct-light contracts. General image loading, sampler configuration,
and additional material texture roles would widen several trust boundaries at
once.

## Decision

An asset may contain at most one `textures` entry and one `images` entry. A
`pbrMetallicRoughness.baseColorTexture` may reference only texture zero with an
omitted or zero `texCoord`. The texture must reference image zero and omit its
sampler. The image must use one in-BIN `bufferView`, exact MIME type
`image/png`, and no URI. A referencing primitive must supply the approved
`TEXCOORD_0` accessor. Multiple resources, explicit samplers, JPEG, URI-backed
images, transforms, additional coordinate sets, and other material texture
roles remain typed unsupported input.

The asset domain uses exact-pinned vendored `png` 0.18.1 with transformations
disabled. It accepts only static, non-interlaced, 8-bit RGB or RGBA PNG. RGB is
expanded to tightly packed RGBA8 with opaque alpha. Header dimensions, pixel
count, retained RGBA8 bytes, decoder working bytes, per-asset decoded bytes,
and aggregate CPU residency are independently bounded. PNG framing, checksum,
decode, or range failures never proxy; valid formats outside the subset may
follow the explicit unsupported-feature proxy policy.

One immutable `AssetTexture` is shared by upload jobs from the same exact
source. Renderer admission reserves unique pending and resident texture counts
and bytes separately from mesh buffers. Only explicit upload processing creates
one `Rgba8UnormSrgb` texture and view per source hash. Renderer initialization
creates one 1x1 white fallback and one repeat/linear sampler with a single mip
level. Every draw uses the same bind-group contract. The shader samples sRGB
RGB into linear space, multiplies sampled RGBA by `baseColorFactor`, and then
uses the resulting RGB in the existing exact-unlit or bounded direct-light
path. An explicit scene `MaterialComponent` replaces the imported material as
a whole and selects the white fallback, disabling the imported texture.

## Consequences

- Texture decoding and upload remain caller-driven; frames perform neither.
- Exact source hashes, mesh keys, world schema, replay, logical hashes, and
  recovery envelopes do not change. Restored services still require explicit
  exact-hash CPU and GPU rehydration.
- Texture rows retain PNG top-to-bottom order and the fixed sampler implements
  the approved omitted-sampler behavior without mipmaps or anisotropy.
- The direct dependency adds a seven-package locked closure. All packages are
  vendored; the decode path adds no runtime I/O, telemetry, service, or native
  library. The reviewed closure includes the `crc32fast` compiler-version cfg
  build probe plus guarded transitive unsafe SIMD/pointer implementations; the
  first-party boundary and `png` parser crate remain safe Rust.
- JPEG, data/external URIs, configurable samplers, mipmaps, other texture
  roles, alpha modes/blending, image-based lighting, HDR, tone mapping,
  persistence catalogs, transport, deployment, and release remain separate
  decisions.

## Status

Implemented by CF029 with bounded RGB/RGBA decode, malformed and over-limit
rejection, unique CPU/GPU accounting, explicit shared upload, white fallback,
sRGB sampling, factor multiplication, direct-light and scene-override evidence,
and exact-hash rehydration tests.
