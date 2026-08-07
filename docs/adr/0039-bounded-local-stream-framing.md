# ADR 0039: Bounded framing over caller-owned local streams

- Status: Accepted
- Date: 2026-08-08
- Task: CF039

## Context

CF038 gives bulk observations a deterministic bounded payload envelope, but a
stream reader still needs the metadata and payload lengths before it can safely
buffer either value. Allowing a future stdio, pipe, shared-memory negotiation,
or authenticated remote adapter to invent that outer framing would duplicate
limit, truncation, correlation, integrity, and error behavior.

Implementing a complete stdio service now would mix framing with request and
response schemas, scheduling, cancellation, shutdown, and GPU lifecycle.
Opening a socket would additionally require authentication, authorization,
confidentiality, replay protection, tenancy, and rate policy. Shared memory
would require platform handles and lease negotiation. Those are independent
security and product decisions.

## Decision

Add a dependency-neutral `cogniform-local-transport` crate over caller-owned
synchronous `Read` and `Write` implementations. Version one uses a fixed
68-byte big-endian header containing `COGLOC01` magic, version, kind, reserved
zero byte, non-zero correlation ID, control length, bulk length, and SHA-256
digest. The digest binds the header prefix and both exact body sections.

Control frames contain non-empty schema-owned bytes and no bulk. This layer
does not interpret or claim canonical control semantics. Observation frames
contain exact canonical `ObservationMetadata` JSON and the CF038 payload
envelope; decoding verifies both the outer frame and the inner semantic
metadata/payload binding before returning an owned value.

The reader obtains the complete fixed header in stack storage and rejects
unsupported, noncanonical, inconsistent, overflowing, or over-limit lengths
before any declared body allocation. It distinguishes clean pre-frame EOF from
header/control/bulk truncation, retries short and interrupted I/O, and preserves
back-to-back boundaries. The writer encodes and validates the complete frame
before its first write, while acknowledging that arbitrary stream writes are
not atomic. Errors and debug output remain payload-redacted.

The frame digest detects corruption, not authenticity, authorization,
freshness, confidentiality, or encryption. The crate opens no resource and
owns no session or abuse policy.

## Consequences

- Future local session adapters can reuse one exact pre-buffer bound and carry
  complete observations without duplicating CF038 semantics.
- Version, kind tags, fixed header, correlation identity, section layouts,
  limits, digest domain, EOF/truncation behavior, and I/O retry rules are
  compatibility-tested; incompatible changes require a new version.
- Schema owners remain responsible for parsing and bounding control bytes.
- A partial physical write remains possible after complete preparation; the
  future session must define whether to close or recover the stream.
- CLI commands, process and pipe creation, listeners, sockets, async runtimes,
  operation schemas, session negotiation, cancellation, authentication,
  authorization, tenancy, confidentiality, rate policy, shared-memory leases,
  files, compression, retention, automatic delivery, deployment, versioning,
  and release remain separate approved work.
