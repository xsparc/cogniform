# Headless reference renderer

CF004 establishes the first real render-domain implementation. It creates a
`wgpu` instance, adapter, device, queue, pipeline, textures, and readback
buffers without creating a display handle, window, or presentation surface.
The public API exposes only project-owned configuration, diagnostics, adapter
metadata, and owned output values.

## Supported baseline

The initial compiled backend set is:

| Platform | Backend | CF004 role |
|---|---|---|
| Windows | Direct3D 12 or Vulkan | Local hardware or software validation |
| Linux | Vulkan | Controlled native or self-hosted validation |

Metal, OpenGL/GLES, browser WebGPU, and visible windowing are not compiled in
this slice. Unsupported platforms return a structured backend error. A selected
adapter must support three color attachments, twelve color-attachment bytes per
sample, the configured target and readback limits, and render/copy usages for
linear `Rgba8Unorm` color, `R32Uint` identity, `Rgba8Unorm` normal, and
`Depth32Float` depth.

No optional or experimental GPU feature is enabled. The adapter summary records
the adapter name, backend, device class, and WebGPU-compliance flag for
diagnostics, but backend handles never leave `cogniform-renderer`.

## Reference scene and outputs

The built-in scene is a unit cube, renderer-local entity ID `7`, fixed linear
RGBA color `[51, 153, 230, 255]`, and a fixed orthographic camera with a small
view shear. The background entity ID is `0`, cleared depth is `1.0`, and
normal alpha `0` marks background.

Extracted built-in geometry supports cuboids, planes, and spheres. A plane is a
centered unit square at local Z = 0, expanded as two counter-clockwise XY
triangles with a positive-Z unit normal. Its positive XYZ dimensions scale the
full model: X and Y control visible size and Z participates in normal
transformation without creating thickness. One fixed 144-byte plane vertex
payload is allocated at renderer initialization; frames do not tessellate or
upload it. The baseline pipeline does not cull the back side and does not flip
its source normal.

A sphere is centered, unit diameter, and uses a positive-Z polar axis. Its
fixed 16 longitude sectors and 8 latitude bands form 224 non-degenerate
outward counter-clockwise triangles, expanded to 672 vertices with unit radial
normals in the same 24-byte layout. The exact 16,128-byte payload is generated
once at renderer initialization. XYZ dimensions are bounding diameters, so
non-uniform values produce an ellipsoid and the existing inverse-transpose
normal path preserves the smooth direction. Sphere topology supplies no UV
attribute, and frames perform no built-in tessellation or upload.

- entity IDs must match exactly;
- color channels use an absolute tolerance of 2 units in RGBA8;
- center depth uses an absolute tolerance of 0.02;
- cuboid, plane, and position-only geometry normals are flat unit vectors
  derived from source triangle winding; sphere radial normals and approved
  imported vertex normals are inverse-transformed into world space and
  interpolated before fragment normalization; all paths are quantized through
  signed RGB8 and renormalized after readback, and controlled comparisons use
  a 0.99 minimum dot product;
- cross-adapter bitwise image equality is not claimed.

The `u32` attachment is compact render identity, not `StableEntityId`. CF005
adds a bounded renderer-owned mapping and retains its compact-to-stable snapshot
with every pending frame, so machine-facing observations expose exact stable
identity even when compact values are later recycled.

## API lifecycle

```rust
use cogniform_renderer::{HeadlessRenderer, RendererConfig};

# async fn example() -> Result<(), Box<dyn std::error::Error>> {
let mut renderer = HeadlessRenderer::new(RendererConfig::new(64, 64)).await?;
let pending = renderer.submit_reference_scene()?;

// Explicit synchronization point. Submission itself does not wait.
let frame = pending.read()?;
assert_eq!(frame.entity_id_at(32, 32), Some(7));
assert!(frame.normal_at(32, 32).is_some());
# Ok(())
# }
```

Configuration rejects zero dimensions, dimensions above 4096, more than
4,194,304 pixels, arithmetic overflow, a zero readback timeout, and a timeout
above 60 seconds before GPU allocation. Texture copies use 256-byte padded
rows; only initialized pixel bytes enter the tightly packed output vectors.

`PendingFrame::read` is intended for conformance tests, the bounded observation
worker, and diagnostics. It may block its caller for the configured timeout.
Submission leases one preallocated color/depth/normal/identity readback set and
fails immediately when the fixed pool is exhausted. The engine worker performs
the blocking read outside renderer submission and releases the lease after
completion.

## Validation

Run the focused contract explicitly on Windows or Linux with a compatible,
approved adapter:

```text
cargo test -p cogniform-renderer --test headless_reference --locked --offline -- --ignored --exact reference_cube_produces_exact_ids_and_tolerant_color_depth_normals
cargo test -p cogniform-renderer --test headless_reference --locked --offline -- --ignored --exact extracted_plane_produces_color_depth_identity_and_plus_z_normal
cargo test -p cogniform-renderer --test headless_reference --locked --offline -- --ignored --exact extracted_sphere_produces_curved_depth_identity_and_radial_normals
cargo test -p cogniform-renderer --test asset_fixture --locked --offline -- --ignored
```

The integration tests render at 64 by 64 pixels, verify exact object and
background IDs, probe cuboid, plane, and sphere color, depth, position-only
winding normals, curved sphere depth and radial normals, and smooth
normals under non-uniform scale using the declared tolerances, validate output
lengths and bounds behavior, and require the selected backend to be DX12 or
Vulkan. Normal workspace CI compiles these tests
but leaves it ignored;
the architecture reserves adapter conformance for controlled local or
self-hosted hardware. The test does not contact a service, create a window,
upload an artifact, or require a paid runner.

See [ADR 0005](../adr/0005-bounded-headless-wgpu-baseline.md) for the dependency,
backend, identity, and synchronization decisions.
See [ADR 0020](../adr/0020-bounded-imported-vertex-normals.md) for the
position/normal vertex contract and inverse-transpose decision.
See [ADR 0021](../adr/0021-centered-built-in-plane-rendering.md) for the plane
geometry, dimension, and fallback convention.
See [ADR 0022](../adr/0022-fixed-built-in-uv-sphere-rendering.md) for the fixed
sphere topology, dimension, normal, and allocation convention.
See [the extraction and observation guide](incremental-extraction-and-observations.md)
for the CF005 world-to-render and asynchronous feedback path.
