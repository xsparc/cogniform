# Local-session control messages

CF040 assigns bounded schema-owned meaning to the control bytes carried by a
[CF039 local frame](local-stream-framing.md). It is an in-memory message
contract, not an executable service, stdio command, process supervisor, pipe,
listener, socket, or remote protocol.

CF042's separate [`serve-stdio` composition](local-stdio-session.md) executes
this contract without adding endpoint ownership or scheduling to the schema
crate.
CF044 adds a compatible schema version two for bounded semantic imagination;
version one remains byte-for-byte fixed.

## Direction and correlation

`cogniform-local-session` exposes distinct `LocalSessionClientMessage` and
`LocalSessionServerMessage` roots. Version one requires `schema_version: 1`;
version two requires `schema_version: 2` and locks that choice at hello. Their
variant sets are not interchangeable. Every message is compact canonical JSON
followed by exactly one LF and is carried only in `LocalFrame::Control`.

The CF039 header's non-zero correlation ID is the only work correlation. It is
not copied into JSON. A completed observation remains
`LocalFrame::Observation` with the correlation of its originating request,
preserving CF038 metadata/payload binding.

## Schema-version-one flow

| Client message | Server message | Meaning |
|---|---|---|
| `hello` | `hello` | Advertise client receive limits and return effective limits |
| `submit_patch` | `patch_admission` | Report queued, already queued, superseded, dropped, or replayed admission |
| — | `patch_completed` | Return one newly applied receipt after explicit execution |
| `query` | `query_result` | Execute one exact-revision logical query |
| `request_observation` | `observation_accepted` | Admit one exact-revision observation request |
| — | `observation_pending` | Report that accepted work is not complete |
| — | CF039 observation frame | Return the complete causal observation value |
| any valid request | `failure` | Return one stable payload-redacted code |
| `close` | `closed` | Request and acknowledge orderly closure |

Patch admission and completion are deliberately separate. A `replayed`
admission contains an `idempotent_replay` receipt whose key must match the
admission key. `patch_completed` requires an `applied` receipt. The schema does
not schedule or automatically process either outcome.

CF041's separate [local-session executor](local-session-executor.md) now owns
the lifecycle and explicit service mapping. Keeping that state in another crate
preserves this schema as a reusable typed/byte contract and prevents endpoint
I/O policy from entering message validation.

## Schema-version-two imagination flow

Version two retains every version-one operation and adds:

| Client message | Server message | Meaning |
|---|---|---|
| `hello` with `compilation_receive_limits` | `hello` with `effective_compilation_limits` | Negotiate independent compilation-result bounds |
| `submit_imagination` | `imagination_admission` | Report queued, already queued, superseded, dropped, or replayed semantic work without compilation at admission |
| — | `imagination_completed` | Return one deterministic compilation and an apply receipt only when it produced a patch |

A compiled completion contains one valid `CompilationResult` patch and one
matching `applied` receipt. Imagination ID, idempotency key, transaction,
scene/base/previous revision, and operation count agree across the enclosing
completion, result, patch, and receipt. An unresolved completion contains no
patch, at least one unresolved entry, and no receipt. A replayed admission
contains the exact cached completion; a compiled replay receipt is marked
`idempotent_replay`, while an unresolved replay still has no receipt.

Hello compilation limits are a field-wise intersection of the client receive
policy, executor policy, and service compiler's runtime-derived bounds. The
explicit `*_with_limits` codec entry points enforce the negotiated result
limits. Legacy server codec entry points reject a version-two hello or
result-bearing imagination response when explicit compilation limits are not
supplied; result-free pre-negotiation failures remain representable.
Version-one hello has no compilation field, and a version-one imagination
variant, a version-two hello without compilation limits, or any post-hello
version switch fails closed.

`failure` has only a stable code: invalid message, unsupported version,
protocol state, limit, revision, capacity, command, query, observation,
service-unavailable, or internal. It carries no input echo, parser string,
path, endpoint, stack detail, or arbitrary message.

## Bounds and canonical bytes

The effective control-message ceiling is the smallest of:

- `RuntimeLimits.max_encoded_bytes`;
- `LocalFrameLimits.max_control_bytes`; and
- `LocalFrameLimits.max_frame_bytes - 68`.

Input byte length and JSON object/array nesting are checked before
deserialization. Serde rejects missing, duplicate, mistyped, direction-invalid,
and unknown fields. The decoded value then validates its session version,
advertised limits, and nested protocol patch, receipt, query, result, or
observation request. Version two additionally validates the nested imagination
and compilation result. Finally it is encoded again with a bounded writer,
output nesting is checked, and the bytes must match exactly, including LF.
Whitespace substitutions, omitted LF,
extensions, truncation, trailing documents, and noncanonical nested values
therefore fail closed.

`LocalSessionLimits` advertises complete-frame bytes, effective session-control
bytes, observation bulk bytes, the effective CF038 envelope and visibility
ceilings, and all core runtime limits. Values are non-zero and the
control/bulk/envelope ceilings must fit in the advertised post-header frame
body; control bytes must also fit the advertised runtime encoded ceiling. A
server's effective values cannot exceed the active local receive configuration.
Version-two compilation results also cross independent encoded, logical,
nesting, text, decision, unresolved, and nested-patch bounds. The complete
enclosing control message must still fit the negotiated frame and
session-control ceilings.

## Exact-revision observations

`ObservationRequest` is now a core protocol value with schema version,
observation ID, exact expected scene revision, camera ID, kind, and quality.
The engine rejects an unsupported schema or revision mismatch before reserving
an observation slot or submitting renderer work. Completion also verifies the
rendered frame revision and camera match the request before producing causal
metadata.

## Non-goals and security boundary

The session codec provides input bounding, schema validation, canonicalization,
and corruption-compatible framing composition. It does not authenticate the
peer, authorize operations, encrypt bytes, provide confidentiality, prevent
replay, isolate tenants, enforce a rate policy, recover a partially written
stream, or define a lifecycle state machine. The fixed local stdio composition
does not add those remote-security properties merely because it uses these
messages.

Schema version one intentionally has no imagination submission or
compilation-result variant. Version two adds only the bounded local mapping; it
does not add model execution, a remote endpoint, or broader authority.

See [ADR 0040](../adr/0040-bounded-versioned-local-session-messages.md), the
[core contracts](core-contracts.md), the
[compilation result contract](compilation-results.md), the
[local service](local-service.md), and the [CF041 executor](local-session-executor.md).
The version-two decision is recorded in
[ADR 0044](../adr/0044-versioned-local-imagination-session-mapping.md).
