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
adapter must support two color attachments, the configured target and readback
limits, and render/copy usages for linear `Rgba8Unorm`, `R32Uint`, and
`Depth32Float`.

No optional or experimental GPU feature is enabled. The adapter summary records
the adapter name, backend, device class, and WebGPU-compliance flag for
diagnostics, but backend handles never leave `cogniform-renderer`.

## Reference scene and outputs

The built-in scene is a unit cube, renderer-local entity ID `7`, fixed linear
RGBA color `[51, 153, 230, 255]`, and a fixed orthographic camera with a small
view shear. The background entity ID is `0` and cleared depth is `1.0`.

- entity IDs must match exactly;
- color channels use an absolute tolerance of 2 units in RGBA8;
- center depth uses an absolute tolerance of 0.02;
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
# Ok(())
# }
```

Configuration rejects zero dimensions, dimensions above 4096, more than
4,194,304 pixels, arithmetic overflow, a zero readback timeout, and a timeout
above 60 seconds before GPU allocation. Texture copies use 256-byte padded
rows; only initialized pixel bytes enter the tightly packed output vectors.

`PendingFrame::read` is intended for conformance tests, the bounded observation
worker, and diagnostics. It may block its caller for the configured timeout.
Submission leases one preallocated readback set and fails immediately when the
fixed pool is exhausted. The engine worker performs the blocking read outside
renderer submission and releases the lease after completion.

## Validation

Run the focused contract explicitly on Windows or Linux with a compatible,
approved adapter:

```text
cargo test -p cogniform-renderer --test headless_reference --locked --offline -- --ignored --exact reference_cube_produces_exact_ids_and_tolerant_color_depth
```

The integration test renders at 64 by 64 pixels, verifies exact object and
background IDs, probes color and depth using the declared tolerances, validates
output lengths and bounds behavior, and requires the selected backend to be
DX12 or Vulkan. Normal workspace CI compiles this test but leaves it ignored;
the architecture reserves adapter conformance for controlled local or
self-hosted hardware. The test does not contact a service, create a window,
upload an artifact, or require a paid runner.

See [ADR 0005](../adr/0005-bounded-headless-wgpu-baseline.md) for the dependency,
backend, identity, and synchronization decisions.
See [the extraction and observation guide](incremental-extraction-and-observations.md)
for the CF005 world-to-render and asynchronous feedback path.
