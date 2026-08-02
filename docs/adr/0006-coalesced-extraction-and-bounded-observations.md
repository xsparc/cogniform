# ADR 0006: Coalesced extraction and bounded observations

- Status: Accepted
- Date: 2026-08-02
- Task: CF005

## Context

The authoritative world and headless renderer need a causal boundary that does
not expose ECS or GPU handles, clone the complete world after a sparse edit, or
retain an unbounded deletion journal when rendering falls behind. Frame
readback must also remain asynchronous from renderer submission without hiding
an unbounded queue or allocating a new set of staging buffers indefinitely.

Three extraction models were considered:

1. clone a complete logical snapshot after every accepted revision;
2. retain a per-revision journal and tombstones for arbitrary consumers; or
3. support the engine's single renderer consumer through a bounded coalescing
   set that drains into ordered immutable packets.

Full snapshots make sparse updates proportional to scene size. An arbitrary
journal needs a retention and resynchronization protocol that CF005 does not
otherwise require. The single-consumer drain matches the current one-world,
one-renderer architecture and can fail admission before world mutation when
its configured capacity is unavailable.

## Decision

`cogniform-protocol` owns an in-process, backend-neutral `RenderExtraction`
contract. A packet contains a non-zero monotonic extraction generation, an
exact base and target scene revision, and strictly stable-ID-ordered upserts or
removals. An upsert carries only render-domain components and a finite derived
world matrix with its transform generation. It contains no ECS handle, compact
GPU identity, transport encoding, or bulk observation bytes.

`AuthoritativeWorld` coalesces changed stable identities in a bounded ordered
set. Creates, deletes, render-component changes, and every entity recomputed by
sparse transform propagation enter the set. Repeated changes to one identity
occupy one slot. Name-only edits advance the target revision through an empty
packet. If admitting a transaction would exceed the pending extraction bound,
preflight rejects the complete patch before mutation. Successful extraction
drains the set and advances the world's fully extracted revision and
generation.

The renderer validates the next generation and exact base revision before
mutating renderer-owned state. It preflights capacity and compact identity
allocation, then applies the packet atomically. Compact non-zero `u32` IDs are
renderer-local and may be recycled after removal. Every pending frame owns a
snapshot of the compact-to-stable mapping used by that submission, so recycled
values cannot change historical observation identity.

Extracted cuboids are drawn through per-draw uniform data using the camera's
derived world transform and perspective component. Plane and sphere drawing
remain unsupported and fail with a stable renderer error; adding their mesh
paths is separate from extraction causality. A distinct non-zero per-frame draw
budget rejects excess primitives before uniform or target allocation. Frames record frame ID, fully
consumed scene revision, camera ID, and extraction generation before
submission.

Readback uses a fixed pool of color, depth, and entity-ID buffers. Admission is
non-blocking and returns `ReadbackPoolExhausted` when every set is in flight.
The engine uses one standard-library worker and a global atomic permit held
from request admission through result delivery. Its bounded job and result
channels therefore cannot collectively exceed the configured outstanding
observation capacity. Renderer submission never waits for GPU readback,
encoding, delivery, or a consumer. Completed color, depth, exact stable-ID, and
visibility payloads retain the source frame metadata and calculate staleness
against the latest world revision at delivery.

## Consequences

- Sparse extraction cost is proportional to the coalesced changed identities,
  including transform descendants, rather than every live entity.
- There is intentionally one drain cursor. A future independent renderer or
  remote subscriber requires an approved bounded journal or snapshot/resync
  protocol rather than silently sharing this cursor.
- World commits receive typed backpressure if the renderer is not draining
  changes. The normal engine path drains immediately after each accepted
  transaction.
- Observation payload memory and readback resources have explicit fixed
  capacities. Pressure is visible and does not create hidden spill.
- Timing fields are observational and not part of logical replay determinism;
  revision, frame, camera, extraction generation, and stable identity are the
  causal contract.
- The implementation adds no dependency, unsafe code, transport, encoder,
  external call, cache, artifact, runner, or CI job. Default CI compiles but
  does not execute adapter-dependent tests; the end-to-end causality test is a
  controlled local or explicitly provisioned self-hosted check.
