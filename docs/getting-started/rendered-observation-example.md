# Render graphics and observation examples

The `render-example` command turns Cogniform's existing headless reference
frame into four ordinary PNG files. It is the quickest way to see what the
renderer produces without opening a window, creating a service session, or
downloading an asset.

## Run the example

Build and run from the repository root with a destination that does not exist:

```text
cargo run --release -p cogniform-cli --locked --offline -- render-example ./target/cogniform-render-example
```

The parent directory must already exist. Cogniform creates the final directory
and refuses files, directories, and symbolic links that already occupy that
path. Use `--` before a destination whose name starts with a dash.
The command needs a compatible headless GPU adapter: Windows builds select
Direct3D 12 or Vulkan, while Linux builds select Vulkan. Adapter selection,
capability preflight, submission, or readback can fail on unsupported systems.
A failure before rendering finishes leaves the destination absent.

One 64 by 64 reference-cube frame produces:

| File | What is visible | Useful for |
|---|---|---|
| `color.png` | The renderer's linear RGBA8 color values | Checking camera framing, material color, lighting, and whether a surface rendered at all |
| `depth.png` | Near pixels in white and far/background pixels in black | Finding occlusion, unexpected intersections, depth discontinuities, and misplaced geometry |
| `normals.png` | World-space XYZ remapped to RGB, with transparent background | Debugging winding, transforms, surface orientation, and geometric-normal continuity |
| `identity.png` | One deterministic display color per stable entity, with transparent background | Instance segmentation, selection, visibility reasoning, and associating pixels with scene entities |
| `manifest.json` | Adapter, dimensions, frame/revision/camera causality, file roles, and the exact entity-to-display-color map | Joining the visual files to one render and interpreting identity colors without guessing |

For this fixed reference cube, the files demonstrate several practical
graphics checks:

- an agent can compare color and identity to confirm that the cube is visible
  and that reference entity 7 occupies its pixels;
- a renderer developer can inspect depth and normals to isolate
  camera, transform, winding, or occlusion defects;
- a computer-vision developer can inspect the identity image as a
  human-viewable one-object segmentation example;
- a test author can retain this manifest with these PNGs for same-render
  diagnostic triage; and
- a contributor can use the color image as a quick built-in render smoke test
  before running focused conformance probes.

Cogniform's broader render and observation APIs can apply the same color,
depth, normal, and stable-identity passes to caller-authored supported scenes.
Those workflows must keep their own observations and causality together within
the originating session; `render-example` does not render a requested object,
export numeric observation payloads, or join evidence from another session.

## Interpretation limits

The PNGs are diagnostic visualizations, not new observation or conformance
formats. `manifest.json` therefore sets `diagnostic_only` to `true`.

- `color.png` preserves the renderer's linear RGBA8 source values and carries a
  linear gamma marker. It is not tone-mapped, exposure-adjusted, or converted
  to sRGB.
- `depth.png` is an eight-bit inverted normalized-depth visualization:
  `1.0 - depth`. Use the observation API when exact normalized floating-point
  depth matters.
- `normals.png` visualizes geometric world-space observations. Material normal
  maps affect direct lighting but intentionally do not replace this output.
- `identity.png` colors are cosmetic and collisions are possible. The stable
  IDs in the manifest and observation payload are authoritative.
- The reference scene contains one cube. It proves the output paths, not every
  supported primitive, light, asset, or material.
- Adapter identity can fingerprint or correlate the local host. Nothing is
  uploaded automatically; review the manifest before sharing it.

The command prepares all encoded bytes before creating the destination and
uses create-new writes for every file. A storage failure after directory
creation can leave earlier files plus a truncated current file. Treat any
failed invocation's new directory as incomplete; the command never overwrites
an existing target. Its cooperative create-new checks do not bind subsequent
child writes to a directory handle against hostile concurrent path
substitution. Cross-adapter bitwise image identity remains outside Cogniform's
contract; use the renderer's documented tolerances for controlled comparison.

This local example follows the same broad pattern as the Khronos
[glTF Sample Assets](https://github.com/KhronosGroup/glTF-Sample-Assets/blob/main/README.md):
small, named scenes make visual capabilities and test intent inspectable.
Cogniform deliberately uses its built-in scene here so the example remains
offline, license-simple, and independent of unsupported glTF features.
