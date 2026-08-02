# ADR 0005: Bounded headless wgpu baseline

- Status: Accepted
- Date: 2026-08-02
- Task: CF004

## Context

Cogniform needs a real render-domain boundary before world extraction and
revision-linked observations can be implemented. The boundary must render
without a window, preserve exact machine identity output, remain usable on a
software adapter on a controlled test host, and reject unsupported hardware
without panicking or silently enabling optional features.

Three implementation choices were considered:

1. use `wgpu` with its complete default native, browser, and window-adjacent
   feature graph;
2. bind directly to Vulkan and Direct3D; or
3. exact-pin `wgpu`, disable default features, and enable only the initial
   native backends and WGSL compiler.

The first option carries browser and backend breadth before those targets are
supported. The second creates unsafe and platform-specific code inside the
project. The third keeps the public renderer backend-neutral while retaining a
safe, replaceable native implementation.

## Decision

Use exact-pinned `wgpu` 30.0.0 with default features disabled. The first
compiled backend set is Vulkan and Direct3D 12, plus `std` and WGSL support.
This declares Windows and Linux as the CF004 conformance platforms. Metal,
OpenGL/GLES, browser WebGPU, windowing, and surfaces remain disabled. A platform
with no compiled backend receives `RendererError::BackendUnavailable` before
`wgpu::Instance` construction.

`HeadlessRenderer::new` is asynchronous and requests no optional or
experimental GPU features. It checks the selected adapter's limits and the
required usages for `Rgba8Unorm`, `Depth32Float`, and `R32Uint` before device or
target allocation. Target dimensions, pixel count, row padding, readback
buffer size, and readback duration are checked against fixed project bounds.
Capability and device failures contain a backend-neutral adapter summary and
structured issues.

The version-one reference scene is a built-in 36-vertex cube viewed through a
fixed orthographic camera. Built-in WGSL writes linear RGBA8 color and a
renderer-local `u32` identity attachment while hardware depth writes to
`Depth32Float`. The renderer-local ID is not a stable world ID and no GPU handle
crosses the crate boundary. CF005 will own the bounded mapping from
`StableEntityId` to compact render identity as part of extraction.

Frame submission and CPU synchronization are separate operations.
`submit_reference_scene` creates bounded offscreen resources, encodes the pass
and copies, submits once, and returns immediately. `PendingFrame::read` is the
explicit bounded wait and mapping point. It removes row padding, validates that
depth is finite and normalized, and returns tightly packed owned outputs. The
per-frame buffers are adequate for this conformance slice; bounded pooling and
asynchronous observation delivery belong to CF005.

Use exact-pinned `pollster` 0.4.0 only as a development dependency for driving
the asynchronous constructor in tests. The locked `wgpu` closure is vendored
for offline builds. It contains platform backends, proc macros, build scripts,
and reviewed upstream unsafe code; `unsafe` remains forbidden in
Cogniform-owned crates. No dependency performs telemetry, paid calls, or
runtime downloads.

Cargo's portable lock and vendor model includes source for target-specific
Windows, Apple, and WebAssembly dependencies even when their renderer features
are disabled on the active target. This increases the checked-in vendor tree by
about 163 MiB uncompressed. The cost is accepted for reproducible clean-clone
offline builds, but backend features must not be expanded casually and a future
change to the source-distribution policy requires a separate decision.

## Consequences

- Headless rendering creates no display handle, window, or presentation
  surface.
- Exact entity-ID probes are portable across the declared adapters. Color and
  depth use declared tolerances; cross-GPU image bit identity is not promised.
- Unsupported limits, formats, devices, and platforms fail through typed
  diagnostics rather than optional-feature assumptions.
- The default pull-request workflow remains one standard Linux quality job.
  It compiles the adapter integration test but leaves execution to an explicit
  controlled local or self-hosted conformance run. No paid GPU runner, matrix,
  artifact upload, cache, or separate workflow is added.
- Metal/GLES/browser support, window integration, normals, assets, extraction,
  readback pools, device-loss restart, shadows, culling, and performance gates
  require later approved slices and evidence.
