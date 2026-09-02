# ADR 0081: Export one create-new rendered-observation example set

- Status: Accepted
- Date: 2026-09-02
- Task: CF081
- Extends: [ADR 0005](0005-bounded-headless-wgpu-baseline.md),
  [ADR 0006](0006-coalesced-extraction-and-bounded-observations.md), and
  [ADR 0011](0011-quantized-world-space-normal-observations.md)

## Context

Cogniform exposes real offscreen color, depth, normal, and entity-identity
outputs, but its canonical scenario reports only compact probes and causal
evidence. A contributor can therefore prove rendering without receiving a
human-viewable example of each output pass.

The Khronos
[glTF Sample Assets](https://github.com/KhronosGroup/glTF-Sample-Assets/blob/main/README.md)
separate showcase, testing, and core examples, and the
[glTF Sample Renderer](https://github.com/KhronosGroup/glTF-Sample-Renderer)
uses compact reference rendering to make implementation behavior inspectable.
Cogniform needs the same documentation benefit without importing remote assets,
expanding its glTF subset, or turning diagnostic files into protocol authority.

## Decision

Add `cogniform-cli render-example <new-directory>`. The command checks an
absent destination and existing parent before selecting an adapter, renders the
fixed 64 by 64 renderer reference scene once, prepares all bytes in memory, and
then creates exactly one new directory containing:

- `color.png`, preserving linear RGBA8 source values with a linear gamma marker;
- `depth.png`, mapping normalized near depth to white and far depth to black;
- `normals.png`, remapping world-space XYZ to RGB with transparent background;
- `identity.png`, assigning each stable entity a deterministic bright display
  color with transparent background; and
- newline-terminated schema-version-one `manifest.json`, retaining dimensions,
  adapter summary, frame/revision/camera/extraction causality, file roles, and
  the exact stable-entity display palette.

The final directory and each fixed child file use create-new behavior. Existing
files, directories, and symbolic links reject without overwrite. Diagnostics
are path-redacted. An I/O failure after directory creation may leave earlier
files plus a truncated current file. Any failed invocation's new directory is
incomplete; the command does not recursively remove a path that another
process could have modified. Create-new protects cooperative local use, but
child writes are not directory-handle-relative and do not resist hostile
concurrent path substitution.

The PNGs are explicitly diagnostic derivatives. Exact numeric observations,
stable identities, renderer tolerances, and protocol envelopes remain
authoritative. No image is checked into the repository, no network or external
asset is used, and no render, world, engine, protocol, persistence, release, or
deployment semantics change. The CLI gains direct edges only to the existing
workspace renderer and already-vendored PNG encoder.

## Consequences

- Users can see concrete graphics and understand distinct color, geometry, and
  identity use cases with one offline command.
- Every image and palette entry is joined to one frame through the manifest,
  avoiding an accidental mix of independently rendered outputs.
- Existing targets remain safe, while partial output after a storage failure is
  visible rather than hidden by broad cleanup authority.
- Adapter details can fingerprint the host, display palettes can collide, the
  eight-bit depth/normal views lose numeric precision, and cross-adapter PNG
  identity is not promised.
- The example remains intentionally small and is not a viewer, asset gallery,
  screenshot baseline, protocol exporter, release artifact, or new renderer
  feature.

## Status

Accepted and implemented by CF081. Unit tests pin endpoint conversions,
transparent background, exact stable palette generation, effective PNG gamma,
and the canonical manifest bytes. Black-box CLI tests pin argument parsing,
parent/target and symbolic-link preflight, and create-new preservation. A
controlled optimized Vulkan test renders the four exact-dimension PNGs from
one reference frame, verifies the emitted identity pixels against the manifest
palette, checks every adapter, causality, and file-role field, and proves a
repeat invocation cannot overwrite the result.
