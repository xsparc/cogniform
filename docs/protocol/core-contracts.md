# Core protocol contracts

Status: schema version 1 implemented by CF001 and extended through CF007.

`cogniform-protocol` is the dependency-neutral boundary between Cogniform's
world, render, and service domains. It contains values only: no ECS handles,
GPU resources, sockets, generated transport messages, or service lifecycle.

## Identity and numeric representation

`StableEntityId`, `TransactionId`, `IdempotencyKey`, `ImaginationId`,
`ProcedureId`, and `ObservationId` are non-zero 128-bit values encoded as 32
lowercase hexadecimal characters. This encoding is canonical and does not rely
on a JSON consumer preserving 128-bit numbers. `ContentHash` is the exact
SHA-256 identity of immutable source bytes and uses 64 lowercase hexadecimal
characters. `SceneRevision` is an unsigned 64-bit counter beginning at zero;
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

Component values cover names, local transforms, built-in primitives, materials,
perspective cameras, baseline lights, and immutable asset mesh references. An
asset reference contains only a content hash and zero-based mesh index; bulk
bytes and backend handles remain outside the protocol. A create operation
cannot repeat a component kind. Rotations must be non-zero, camera ranges must
be valid, and an entity cannot directly parent itself. Full hierarchy
cycle/depth, ownership, asset availability, and authoritative-world checks
belong to their owning domains.

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
observations require bounded non-zero dimensions. Schema v1 recognizes color,
depth, normal, entity-ID, and visibility kinds. Normal payloads are local owned
flat world-space unit vectors decoded from signed RGB8; background pixels are
absent. Structured visibility metadata has no pixel dimensions.

Bulk image bytes and vectors are deliberately absent. Renderer and future
transport adapters must keep payload storage separate from this causal
envelope. Older schema-v1 clients that exhaustively enumerate observation kinds
must update before accepting `normal` metadata.

## Imaginations and logical queries

CF006 adds a bounded `ImaginationEnvelope` for the pure primitive compiler. It
carries exact revision and idempotency identity, delivery semantics, an
explicit seed, sender-declared compilation and output-patch budgets, primitive
entity descriptions, a small typed relation subset, and stable-ID scene-view
preconditions. Missing optional runtime details are resolved by documented
defaults and every choice is returned as a structured compiler decision.

`SceneQuery` and `SceneQueryResult` provide exact-revision backend-neutral
logical views. Entity and component filters are unique and bounded. Results
must be in strict stable-ID/component-kind order and fail when the complete
match set exceeds the declared result limit. They never expose ECS handles.

The detailed compiler, admission, idempotency, and query behavior is documented
in the [local gateway guide](local-gateway-and-imagination.md).

CF005 also defines an in-process `RenderExtraction` value contract. It carries
one monotonic generation, exact base/target revisions, and strictly
stable-ID-ordered complete upserts or removals. Upserts contain a finite derived
world matrix and only render-domain components, including an immutable asset
mesh reference when present. This value is deliberately not a canonical public
transport schema: it contains no ECS/GPU handle and keeps bulk asset and
observation storage separate, but a future remote extraction boundary requires
its own versioned encoding and resynchronization design.

## Asset references and built-in procedures

CF007 adds `AssetMeshComponent` as ordinary authoritative scene state. It is
included in snapshots, the version-one logical hash, replay, and render
extraction. Exact source admission, CPU decoding, diagnostics, and GPU upload
are separate bounded domain APIs described in the
[GLB asset guide](../assets/glb-subset.md).

Built-in procedures are pure library functions with an explicit `ProcedureId`,
seed, transaction and idempotency identities, base revision, delivery
semantics, and output budgets. The initial cuboid-grid procedure returns an
ordinary validated `ScenePatch` in deterministic row-major order; it has no
world, filesystem, network, clock, entropy, renderer, or mutation access.

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
| Imagination entities | 256 |
| Imagination relations | 512 |
| Imagination constraints | 256 |
| Query result entities | 1,024 |
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
pixel limits, plus imagination and query collections. Parser errors retain only category and location, not untrusted
input or an unbounded diagnostic string.

Callers should decode through `ScenePatch::from_json`,
`ApplyReceipt::from_json`, `ObservationMetadata::from_json`,
`ImaginationEnvelope::from_json`, `SceneQuery::from_json`, or
`SceneQueryResult::from_json`, and encode with
the corresponding `to_canonical_json` method. Direct Serde use does not apply
runtime limits.
