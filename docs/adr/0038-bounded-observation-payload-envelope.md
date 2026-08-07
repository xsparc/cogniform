# ADR 0038: Bounded transport-neutral observation payload envelope

- Status: Accepted
- Date: 2026-08-08
- Task: CF038

## Context

Observation metadata already has a bounded canonical JSON contract, while
renderer results deliberately retain bulk color, depth, normal, identity, and
visibility data as in-process owned vectors. Every future shared-memory, local
stream, or authenticated remote adapter would otherwise invent its own value
layout and its own association between those bytes and causal metadata.

Selecting a listener, session protocol, authentication scheme, tenancy model,
retention policy, compression, or image format now would prematurely combine
independent security and deployment decisions. Putting bulk data into
`cogniform-protocol` would also weaken its small backend-neutral contract.

## Decision

Create `cogniform-observation`, depending only on `cogniform-protocol` and the
existing pinned SHA-256 implementation. It owns the public payload value types
and a deterministic version-one binary envelope. `cogniform-engine` re-exports
the moved types for source compatibility and offers one explicit convenience
method; observation receipt and delivery behavior otherwise remain unchanged.

Version one uses a fixed 60-byte header with `COGOBS01` magic, unsigned
big-endian version, kind, reserved zero byte, item count, payload byte count,
and SHA-256 digest. The digest binds the header prefix, exact canonical
LF-terminated `ObservationMetadata` JSON, and payload. Color, normalized depth,
optional unit normal, optional stable entity ID, and sorted visibility values
use fixed canonical big-endian layouts. Exact lengths, presence tags, finite
canonical floats, identity/order/count invariants, runtime pixel bounds, an
independent visibility-entry bound, and a complete-envelope bound are checked.

Encoding completes validation before output allocation. Decoding checks the
borrowed input bound, framing, exact declared size, metadata, and integrity
before allocating the decoded vector. The crate performs no I/O and owns no
renderer, service, transport, session, file, or shared-memory resource.

The SHA-256 value provides integrity detection, not authenticity,
authorization, freshness, confidentiality, or encryption. A future transport
must enforce its own framing cap before it buffers the slice passed to this
decoder and must supply its own identity and abuse controls.

## Consequences

- Payload layout and causal binding can be reused by future adapters without
  giving transport concerns to the renderer or protocol crate.
- The engine's existing payload import path remains available, while direct
  transport-oriented consumers can depend on the narrower new crate.
- Exact fixtures, all-kind round trips, truncation/trailing/corruption tests,
  metadata substitution, invalid canonical values, and limit tests make a
  future format change require an explicit new version and review.
- The default 4 MiB envelope cap is intentionally below the protocol's maximum
  possible image size; embedders must select a reviewed larger limit when
  needed.
- Listeners, sessions, authentication, authorization, tenancy, rate limits,
  shared-memory allocation, gRPC/QUIC/MCP, compression, image formats,
  retention, automatic delivery, deployment, versioning, and release remain
  separate approved work.
