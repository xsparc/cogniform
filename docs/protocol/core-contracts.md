# Core protocol contracts

Status: schema version 1 implemented by CF001.

`cogniform-protocol` is the dependency-neutral boundary between Cogniform's
world, render, and service domains. It contains values only: no ECS handles,
GPU resources, sockets, generated transport messages, or service lifecycle.

## Identity and numeric representation

`StableEntityId`, `TransactionId`, `IdempotencyKey`, and `ObservationId` are
non-zero 128-bit values encoded as 32 lowercase hexadecimal characters. This
encoding is canonical and does not rely on a JSON consumer preserving 128-bit
numbers. `SceneRevision` is an unsigned 64-bit counter beginning at zero;
`FrameId` is non-zero. Revision increments are checked for overflow.

Protocol floating-point wrappers reject NaN and infinity. Negative zero is
normalized to positive zero. Unit, positive, and non-negative wrappers express
range requirements before a value reaches a component.

## Scene patches

A `ScenePatch` always carries:

- schema, transaction, and idempotency identity;
- the authoritative base revision and exact-base conflict policy;
- `MustApply`, keyed `LatestWins`, or `BestEffort` delivery semantics;
- a sender-declared operation/component/text/logical-decoded-byte budget; and
- a non-empty ordered sequence of create, delete, set-component,
  remove-component, or reparent operations.

Initial component values cover names, local transforms, built-in primitives,
materials, perspective cameras, and baseline lights. A create operation cannot
repeat a component kind. Rotations must be non-zero, camera ranges must be
valid, and an entity cannot directly parent itself. Full hierarchy cycle/depth,
ownership, asset, and authoritative-world checks belong to the world boundary;
cycle and depth enforcement is implemented by CF003.

Patch validation compares actual counts and declared budgets with
`RuntimeLimits`. Validation never reorders operations. Atomic commit and
idempotency storage are world responsibilities; the contract makes their input
and receipt fields mandatory.

## Receipts, queues, and observations

An accepted `ApplyReceipt` describes exactly one revision increment, a non-zero
operation count, the original transaction and idempotency values, bounded typed
diagnostics, stage timings, and an estimated visible frame. An
`IdempotentReplay` status identifies the recorded receipt returned for a
repeated key.

`QueueConfig` combines a non-zero bounded capacity with explicit delivery
semantics. `LatestWins` cannot exist without a non-empty supersession key.

`ObservationMetadata` always carries observation, revision, frame, and camera
identity; observation kind and quality; completion timestamp and latency; and
an exact staleness calculation relative to the latest known revision. Image
observations require bounded non-zero dimensions. Structured visibility
metadata has no pixel dimensions.

Bulk image bytes are deliberately absent. Later renderer or transport adapters
must keep payload storage separate from this causal envelope.

CF005 also defines an in-process `RenderExtraction` value contract. It carries
one monotonic generation, exact base/target revisions, and strictly
stable-ID-ordered complete upserts or removals. Upserts contain a finite derived
world matrix and only render-domain components. This value is deliberately not
a canonical public transport schema: it contains no ECS/GPU handle and keeps
bulk observation storage separate, but a future remote extraction boundary
requires its own versioned encoding and resynchronization design.

## Canonical JSON and limits

The supported encoder produces one compact JSON object followed by LF. Field
order is fixed by the schema and canonical messages contain no maps. Checked-in
fixtures under `crates/cogniform-protocol/tests/fixtures/` are the byte-level
compatibility evidence.

The default `RuntimeLimits` are deliberately conservative starting points:

| Limit | Default |
|---|---:|
| Encoded message bytes | 1,048,576 |
| Logical decoded bytes | 4,194,304 |
| JSON nesting depth | 32 |
| Operations per patch | 1,024 |
| Components per patch | 8,192 |
| Components per created entity | 64 |
| Aggregate text bytes | 65,536 |
| Diagnostics per receipt | 128 |
| Queue capacity | 1,024 |
| Observation width or height | 4,096 |
| Observation pixels | 16,777,216 |

The default sender `PatchBudget` is 256 operations, 2,048 components, 16,384
text bytes, and 262,144 logical decoded bytes. Embedders may select lower or
higher non-zero runtime limits, but a patch declaration above the active
runtime ceiling is rejected rather than silently clamped.

The decoder rejects an encoded message before parsing when it exceeds
`max_encoded_bytes` or `max_json_nesting_depth`. Serde rejects missing,
duplicate, unknown, or type-incompatible fields. Typed validation then applies
the deterministic `max_decoded_bytes` accounting described in ADR 0002 plus
operation, component, text, diagnostics, queue, observation dimension, and
pixel limits. Parser errors retain only category and location, not untrusted
input or an unbounded diagnostic string.

Callers should decode through `ScenePatch::from_json`,
`ApplyReceipt::from_json`, or `ObservationMetadata::from_json`, and encode with
the corresponding `to_canonical_json` method. Direct Serde use does not apply
runtime limits.
