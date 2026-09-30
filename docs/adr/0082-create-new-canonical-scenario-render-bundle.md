# ADR 0082: Export canonical-scenario observations as a create-new graphics bundle

- Status: Accepted
- Date: 2026-09-03
- Task: CF082
- Extends: [ADR 0009](0009-recorded-engine-and-local-typed-service.md),
  [ADR 0035](0035-versioned-canonical-scenario-json.md), and
  [ADR 0081](0081-create-new-rendered-observation-examples.md)

## Context

`render-example` makes the four renderer attachments visible, but it uses an
internal one-cube reference frame at revision zero. The canonical unattended
scenario exercises Cogniform's real product loop—atomic scene patches, a room,
table, light, camera, exact-revision observations, and deterministic replay—yet
its CLI output retains only compact proof fields. Users cannot inspect the
graphics produced by that end-to-end scene.

The Khronos
[glTF Sample Assets](https://github.com/KhronosGroup/glTF-Sample-Assets/blob/main/README.md)
pair named scenes with screenshots and descriptions so feature intent can be
inspected. Cogniform needs the equivalent for its canonical scene without
adding a viewer, external asset, new scene format, or new render behavior.

## Decision

Add `cogniform-cli render-scenario <new-directory>`. The command checks one
absent destination and its existing parent before selecting a GPU adapter,
creates a fresh fixed 64 by 64 local service, and runs the unchanged
`canonical-mvp-v1` scenario to revision two. It then requests color, depth,
normal, and entity-ID observations sequentially for the canonical camera at
that exact revision.

Each observation is produced by a distinct frame. The schema-version-one
manifest therefore records observation ID, frame ID, scene revision, and
camera ID per file rather than claiming one shared frame. It also records the
adapter summary, canonical entity identities, matching live and replayed
logical hashes, and the exact identity-display palette. The four PNG
transformations and create-new filesystem behavior reuse ADR 0081:

- linear RGBA8 color with a linear gamma marker;
- near-white/far-black eight-bit normalized depth;
- world-space XYZ remapped to RGB with transparent background; and
- deterministic stable-identity display colors with transparent background.

All observations and encoded bytes are prepared before output creation.
Existing files, directories, and symbolic links are not overwritten, and
diagnostics do not echo paths. A failed write can leave an incomplete new
directory, and cooperative child creation does not defend against hostile
concurrent path substitution. The result is a local diagnostic derivative,
not a protocol payload, conformance baseline, persisted scene, or release
asset.

No engine, renderer, observation, protocol, asset, dependency, CI, release, or
deployment contract changes. Caller-defined scenes and dimensions, external
assets, networking, and publication remain outside this command.

## Consequences

- Users can see the actual room and updated table produced by the canonical
  patch/query/render/replay workflow.
- Per-file causality makes independently rendered observation passes explicit
  and prevents consumers from assuming simultaneous capture.
- The color image supports appearance and lighting inspection; depth and
  normals isolate occlusion and geometry problems; identity supports
  agent-visible grounding and segmentation.
- Adapter identity and logical hashes can correlate the local host or run, so
  no output is uploaded automatically.
- PNG values remain diagnostic and lossy where documented; numeric observation
  payloads and logical hashes remain authoritative.

## Status

Accepted and implemented by CF082. Unit tests pin manifest framing and distinct
frame representation. Black-box tests pin exact arguments, pre-GPU target
validation, and create-new preservation. A controlled optimized Vulkan test
runs the unchanged canonical scenario, verifies four 64 by 64 PNGs, the known
table center color, same-revision/camera metadata across strictly increasing
frames, matching replay hashes, identity palette evidence, and repeat-invocation
overwrite rejection.
