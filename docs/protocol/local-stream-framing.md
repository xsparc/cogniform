# Local stream framing

CF039 defines a bounded synchronous frame over caller-owned local byte streams.
CF042 now uses that boundary in the CLI's fixed-profile inherited-stdio
session, but this crate is not itself a CLI command, process supervisor, pipe,
listener, socket, daemon, or session protocol.

The `cogniform-local-transport` crate operates only on supplied
`std::io::Read` and `std::io::Write` implementations. It supports exact
schema-owned control bytes and complete observations composed from canonical
`ObservationMetadata` plus the CF038 payload envelope. It starts no thread or
async runtime and performs no ambient I/O.

## Version-one frame

All integer fields are unsigned big-endian. The fixed header is exactly 68
bytes:

| Offset | Bytes | Field |
|---:|---:|---|
| 0 | 8 | ASCII magic `COGLOC01` |
| 8 | 2 | version, currently `1` |
| 10 | 1 | kind: control `1`, observation `2` |
| 11 | 1 | reserved zero byte |
| 12 | 8 | non-zero correlation ID |
| 20 | 8 | control-section byte count |
| 28 | 8 | bulk-section byte count |
| 36 | 32 | SHA-256 integrity digest |

The digest covers the 36-byte header prefix, exact control section, and exact
bulk section, in that order. A control frame requires a non-empty control
section and no bulk section. Its bytes are opaque to this framing layer: the
CF040's optional `cogniform-local-session` adapter supplies one bounded
direction-specific schema for a local patch/query/observation loop. The frame
layer remains unaware of that schema and does not parse, execute, or authorize
it.

An observation frame requires both sections. Control is the exact canonical
LF-terminated JSON encoding of `ObservationMetadata`; bulk is its version-one
observation-payload envelope. Decode first verifies the outer frame digest,
then requires byte-for-byte canonical metadata and invokes the inner payload
decoder so kind, dimensions, count, values, ordering, bounds, and metadata
binding all remain singular.

The SHA-256 digests detect corruption and substitution. They are not message-
authentication codes, signatures, authorization decisions, freshness proofs,
encryption, or confidentiality controls. A malicious writer can replace the
body and both digests.

## Bounds and stream behavior

`LocalFrameLimits` defaults to:

| Limit | Default bytes |
|---|---:|
| Complete frame | 5,242,948 |
| Control section | 1,048,576 |
| Bulk section | 4,194,304 |

`LocalFrameConfig` also carries the active protocol runtime limits and CF038
payload limits. Callers may select different explicit non-zero ceilings.

`read_frame` reads the fixed header into stack storage, validates magic,
version, kind, reserved byte, correlation ID, section layout, checked length
arithmetic, and all three outer limits before reserving either body vector.
Short reads and `Interrupted` are retried. End-of-stream before any header byte
returns `None`; end-of-stream after a frame begins is typed truncation. A
stream may contain back-to-back frames, while `decode_frame` requires one exact
borrowed frame and rejects trailing bytes.

`write_frame` validates and encodes the complete frame before calling the
writer, and it handles short writes and `Interrupted`. Stream writes are not
transactional: a writer error can still occur after an encoded prefix reaches
the caller-owned sink. The API does not flush, retry a failed whole frame,
resynchronize after corruption, or choose shutdown behavior.

Errors retain only stable operation, section, kind, index, and byte-count
categories. `LocalFrame` debug output reports aggregate byte or item counts,
not control or observation payload values.

## Session-schema boundary

The [CF040 local-session schema](local-session-messages.md) now defines
versioned hello, patch, query, observation, failure, and close control values.
It reuses this frame's outer correlation ID and exact control bounds, but it
does not own `Read`, `Write`, stdin/stdout, a service, a scheduler, or shutdown.

## Endpoint boundary

The separate [CF041 executor](local-session-executor.md) now defines bounded
caller-driven lifecycle, service mapping, and deterministic advancement.
CF042's [`serve-stdio` composition](local-stdio-session.md) supplies one fixed
local policy for inherited redirected standard I/O, half-duplex scheduling,
per-frame flush, live-operation deadlines, stream failures, and shutdown.
Those decisions remain outside this framing crate and do not create a named
pipe, listener, socket, or process. A remote listener additionally requires authenticated
identity, authorization, confidentiality, replay/freshness protection, and
tenancy isolation. Shared-memory handle negotiation and lease lifetime are
also separate work. This crate supplies none of those policies and must not be
presented as a secure remote protocol.

See [ADR 0039](../adr/0039-bounded-local-stream-framing.md), the
[observation-payload envelope](observation-payload-envelope.md), and the
[local service](local-service.md).
