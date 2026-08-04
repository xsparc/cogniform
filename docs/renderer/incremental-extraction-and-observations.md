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

`EngineConfig::observation_capacity` cannot exceed the readback pool or active
protocol queue limit. One permit is held while a request is queued, read back,
completed, and waiting for delivery. A full queue returns
`ObservationError::CapacityExceeded`; submission does not sleep, retry, spill,
or wait for a consumer.

The worker may wait for its submitted GPU frame. That synchronization is
outside renderer submission and cannot mutate the world or renderer scene.
Dropping or delivering the result releases its permit; completing readback
returns the fixed buffers to the renderer pool.

## Outputs and limitations

Each request selects one payload:

- linear RGBA8 color;
- normalized f32 depth;
- flat world-space unit normals decoded from quantized signed RGB8, with
  explicit absent background pixels;
- exact `Option<StableEntityId>` per pixel, with `None` for background; or
- stable-ID-sorted visibility counts.

Every result uses `ObservationMetadata` with the source revision, frame,
camera, quality, completion time, latency, and staleness against the latest
known world revision. An observation cannot claim a revision newer than its
frame input.

The extracted draw path currently supports cuboids and perspective cameras.
Plane and sphere records are retained by renderer state but frame submission
returns a structured unsupported-primitive error until their bounded mesh
paths land. Lighting, assets, encoding, shared memory, remote delivery, and
service APIs remain later slices.

## Validation

Default offline validation exercises extraction ordering, sparse descendant
updates, logical-only revision advancement, capacity rejection, atomic
renderer state updates, compact-ID recycling, observation permits, and public
metadata invariants without requiring a GPU.

Run the controlled end-to-end contract on an approved DX12 or Vulkan adapter:

```text
cargo test -p cogniform-engine --test revision_causality --locked --offline -- --ignored --exact extracted_frames_and_bounded_observations_preserve_revision_causality
```

It renders an extracted cuboid, verifies exact stable entity IDs, color, depth,
normal, visibility, source revision/frame/camera, revision staleness, and
deterministic capacity failure. It creates no window, performs no external
call, and uploads no artifact.

See [ADR 0006](../adr/0006-coalesced-extraction-and-bounded-observations.md)
for the selected single-consumer extraction and pooling model.
