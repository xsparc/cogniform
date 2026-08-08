# Observation payload envelope

CF038 adds a transport-neutral binary representation for the owned payload
that accompanies one validated `ObservationMetadata` value. Metadata remains
canonical JSON in `cogniform-protocol`; bulk bytes live in the separate
`cogniform-observation` crate so a future local or remote adapter can frame the
two values without putting image data into the causal schema.

The codec is an in-memory library boundary. It opens no listener or file,
allocates no shared memory, and does not compress, upload, retain, encrypt, or
authenticate observations. `CogniformEngine::try_receive_observation` still
returns owned vectors. A caller opts into encoding with
`Observation::to_payload_envelope` or the crate-level `encode_payload`.

## Version-one framing

All integers and floating-point bit patterns are big-endian. The fixed header
is 60 bytes:

| Offset | Bytes | Field |
|---:|---:|---|
| 0 | 8 | ASCII magic `COGOBS01` |
| 8 | 2 | unsigned version, currently `1` |
| 10 | 1 | kind: color `1`, depth `2`, normal `3`, entity ID `4`, visibility `5` |
| 11 | 1 | reserved zero byte |
| 12 | 8 | unsigned item count |
| 20 | 8 | unsigned payload-byte count |
| 28 | 32 | SHA-256 integrity digest |

The digest covers the 28-byte header prefix, the exact canonical
LF-terminated JSON encoding of the supplied `ObservationMetadata`, and the
payload bytes, in that order. This binding prevents a payload from being
silently paired with different valid metadata. It detects accidental or
untrusted byte changes, but it is not a MAC, signature, authorization check,
freshness proof, encryption scheme, or confidentiality control.

Version one permits no trailing bytes and uses these fixed item layouts:

| Kind | Bytes/item | Canonical payload |
|---|---:|---|
| Color | 4 | raw linear RGBA8 |
| Depth | 4 | finite normalized `f32` bits; negative zero is rejected |
| Normal | 13 | presence byte then three `f32` components; absence is 13 zero bytes |
| Entity ID | 17 | presence byte then one non-zero 128-bit stable ID; absence is 17 zero bytes |
| Visibility | 24 | non-zero 128-bit stable ID then non-zero unsigned pixel count |

Presence is exactly `0` or `1`. Present normal components are finite,
non-negative-zero values whose squared length differs from one by at most
`OBSERVATION_NORMAL_LENGTH_SQUARED_TOLERANCE` (`1e-3`). Visibility entries are
strictly increasing by stable ID, their aggregate count cannot exceed the
active runtime pixel limit, and their entry count has an independent bound.

## Bounds and failure behavior

`ObservationPayloadLimits` defaults to a 4 MiB complete envelope and 4,096
visibility entries. Image counts must also exactly match metadata dimensions
and the active `RuntimeLimits`. Callers may choose explicit non-zero limits;
the default envelope cap is intentionally more conservative than the largest
protocol image dimensions.

Encoding validates metadata, kind, count, values, ordering, and exact size
before allocating the output. Decoding checks the caller-provided input slice
against its envelope limit, validates framing and declared exact size, binds
canonical metadata, and verifies the digest before allocating the decoded
output vector. Errors are typed and expose only stable kinds, indexes, and byte
counts, never payload values or stable IDs.

The optional [local stream frame](local-stream-framing.md) supplies one fixed
header and declared-length cap before buffering this envelope from a
caller-owned synchronous stream. It still provides no endpoint, session,
authorization, rate, timeout, or confidentiality policy. Other stream or
datagram adapters must enforce equivalent pre-buffer bounds and their own
authenticated session. Passing an already-buffered slice to this codec does
not make the surrounding transport bounded or trusted. Shared-memory leases,
authenticated gRPC/QUIC, compression, image formats, observation retention,
and automatic delivery remain separate decisions.

See [ADR 0038](../adr/0038-bounded-observation-payload-envelope.md), the
[core contracts](core-contracts.md), and the
[extraction guide](../renderer/incremental-extraction-and-observations.md).
