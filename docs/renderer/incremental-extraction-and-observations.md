# Incremental extraction and observations

CF005 connects the authoritative world to renderer-owned state and bounded
observation completion. The path remains local, headless, and independent of
transport, encoding, assets, or model execution.

## Causal flow

```text
accepted patch
  -> bounded changed stable-ID set
  -> ordered RenderExtraction(base revision, target revision, generation)
  -> atomic renderer-state update
  -> frame(frame ID, revision, camera, extraction generation)
  -> fixed readback lease
  -> bounded observation worker
  -> metadata + selected owned payload
```

The world coalesces repeated changes to the same identity before extraction.
Creates, deletes, primitive/material/camera/light changes, and every descendant
whose world transform is recomputed produce a record. A logical-only edit can
advance the revision with zero render records. Extraction drains only these
identities; it never calls `WorldSnapshot` or clones all live entities.

Packets are accepted only in consecutive extraction-generation order and when
their base revision equals the renderer's current fully consumed revision.
The renderer update is preflighted before mutation. Stable world IDs remain in
CPU metadata; the GPU receives compact non-zero IDs whose per-frame mapping is
retained until readback completes.

## Bounded pressure

There are two related bounds:

- `WorldConfig::max_pending_render_changes` limits stable identities awaiting
  extraction. A transaction that would exceed it is rejected atomically.
- `RendererConfig::readback_capacity` preallocates complete
  color/depth/normal/ID staging sets. Submission returns
  `ReadbackPoolExhausted` immediately when no lease is available.
- `RendererConfig::max_draws_per_frame` bounds per-frame uniform and draw work;
  excess extracted primitives fail before GPU allocation.
- The renderer accepts independently bounded sets of at most four directional
  and four point definitions in stable entity-ID order. Zero-intensity
  definitions count toward their kind's fixed limit but are omitted from
  shading; a fifth definition fails before GPU submission.

`EngineConfig::observation_capacity` cannot exceed the readback pool or active
protocol queue limit. One permit is held while a request is queued, read back,
completed, and waiting for delivery. A full queue returns
`ObservationError::CapacityExceeded`; submission does not sleep, retry, spill,
or wait for a consumer.

The permit also owns one monotonic admission instant. Service status reports
only the saturating elapsed microseconds of the oldest live permit, from
successful reservation through queued, active, and completed-awaiting-delivery
states. Submission failure, successful delivery, delivery of a
worker/readback error, channel failure,
or any other permit drop releases both capacity and age state. A completed
readback error retains its permit while awaiting caller delivery, exactly like
a successful completion. Empty
observation state reports no age. The instant is not a wall-clock timestamp,
observation identity, metadata field, replay value, or exported signal.

The worker may wait for its submitted GPU frame. That synchronization is
outside renderer submission and cannot mutate the world or renderer scene.
Dropping or delivering the result releases its permit; completing readback
returns the fixed buffers to the renderer pool.

## Outputs and limitations

Each request selects one payload:

- linear RGBA8 color;
- normalized f32 depth;
- world-space unit normals decoded from quantized signed RGB8, flat for
  outward cuboids, positive-Z planes, and source-wound position-only assets,
  and interpolated from radial sphere or approved imported vertex directions,
  with explicit absent background pixels;
- exact `Option<StableEntityId>` per pixel, with `None` for background; or
- stable-ID-sorted visibility counts.

Every result uses `ObservationMetadata` with the source revision, frame,
camera, quality, completion time, latency, and staleness against the latest
known world revision. An observation cannot claim a revision newer than its
frame input.

CF038 keeps these owned vectors intact and moves their public value types into
the dependency-neutral `cogniform-observation` crate. An explicit in-memory
codec can bind any completed payload to its canonical metadata in a bounded,
fixed-layout binary envelope. Encoding is never part of renderer submission or
worker completion, and the crate owns no GPU, service, transport, file, or
shared-memory resource. See the
[observation-payload envelope guide](../protocol/observation-payload-envelope.md).
CF039 can carry that complete value through bounded caller-owned synchronous
streams after local delivery, without entering renderer submission or worker
completion. See the
[local stream-framing guide](../protocol/local-stream-framing.md).

CF040 moves `ObservationRequest` to the backend-neutral protocol boundary and
adds mandatory schema version plus exact expected scene revision. The engine
rejects mismatch before reserving its global observation slot or asking the
renderer for a frame, then checks the completed source revision again. The
session schema can carry this request and report accepted/pending state, while
the renderer output and completed CF039 observation representation stay
unchanged. See the
[local-session message guide](../protocol/local-session-messages.md).

The extracted draw path currently supports cuboids, centered XY planes, fixed
unit-diameter spheres, explicitly resident approved GLB meshes, and perspective
cameras. An unavailable asset uses its exact explicit primitive fallback;
resident assets retain precedence. All three current `PrimitiveShape` values
therefore select their named geometry without a silent cuboid substitution.
Content-hash eviction changes only CPU/GPU asset availability, not the
`RenderScene`, extracted component, renderer revision, or frame frontier. The
next preparation therefore selects the same authored fallback or returns the
same typed `AssetUnavailable` key until exact residency is explicitly restored.
The public unsupported-primitive renderer error remains reserved for
compatibility and future shape evolution. Directional lights use transformed
local positive Z as the surface-to-light direction. Point lights use extracted
world translation with capped inverse-square attenuation and exact-zero
distance safety. Both apply one bounded direct metallic-roughness response
through the same draw path, using the selected camera translation for the view
direction; no active light of either kind preserves exact unlit output.
Further texture roles and sampling policy, configurable point range/radius,
image-based lighting, image/compression encoding, shared memory, and remote
delivery remain later slices.

## Validation

Default offline validation exercises extraction ordering, sparse descendant
updates, logical-only revision advancement, capacity rejection, atomic
renderer state updates, compact-ID recycling, exact observation permit-age
reservation/release, microsecond saturation, and public metadata invariants
without requiring a GPU.

Run the controlled end-to-end contract on an approved DX12 or Vulkan adapter:

```text
cargo test -p cogniform-engine --test revision_causality --locked --offline -- --ignored --exact extracted_frames_and_bounded_observations_preserve_revision_causality
```

It renders an extracted cuboid, verifies exact stable entity IDs, color, depth,
normal, visibility, source revision/frame/camera, revision staleness, and
deterministic capacity failure. It creates no window, performs no external
call, and uploads no artifact.

See [ADR 0006](../adr/0006-coalesced-extraction-and-bounded-observations.md)
for the selected single-consumer extraction and pooling model. See
[ADR 0020](../adr/0020-bounded-imported-vertex-normals.md) for imported normal
validation and rendering.
See [ADR 0021](../adr/0021-centered-built-in-plane-rendering.md) for built-in
plane geometry and fallback selection.
See [ADR 0022](../adr/0022-fixed-built-in-uv-sphere-rendering.md) for fixed
sphere geometry, radial normals, bounding-diameter scaling, and fallback
selection.
See [ADR 0023](../adr/0023-bounded-directional-diffuse-lighting.md) for the
direction, capacity, combination, and exact unlit-compatibility rules.
See [ADR 0024](../adr/0024-bounded-point-diffuse-lighting.md) for Point
translation, attenuation, independent capacity, and zero-distance rules.
See [ADR 0038](../adr/0038-bounded-observation-payload-envelope.md) for the
separate payload-codec boundary and transport responsibilities.
See [ADR 0039](../adr/0039-bounded-local-stream-framing.md) for header-first
local stream bounds and the deferred session boundary.
See [ADR 0025](../adr/0025-outward-built-in-cuboid-winding.md) for the fixed
cuboid's outward winding and corrected normal/lighting compatibility behavior.
