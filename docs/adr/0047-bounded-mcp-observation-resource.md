# ADR 0047: Bounded MCP observation resource

- Status: Accepted
- Date: 2026-08-09
- Task: CF047

## Context

CF045 established one bounded local MCP stdio adapter and CF046 added the
camera-capable direct patch prerequisite. A fresh MCP session can now construct
a camera, but it still cannot request or transport the causal observation
payloads already owned by the engine and encoded by the CF038 `COGOBS01`
envelope.

Embedding up to 4 MiB of binary observation data in every tool result would
duplicate a bulk value inside ordinary result metadata. Retaining observation
history, advertising resource templates, or adding subscriptions and
list-change notifications would introduce new lifecycle, capacity, and
notification contracts before a single-result workflow has been proven. The
MCP resources capability already supports a narrower composition: one tool can
return a link to one bounded binary resource that a client explicitly reads.

## Decision

Append `cogniform.observe_scene` after the three existing tools. Its input is
one complete core `ObservationRequest`. The adapter parses and validates that
request before lazy service creation, submits it only through `LocalService`,
and polls only the correlated local-service delivery at a fixed positive 2 ms
cadence until a fixed 15 second deadline.

A completion is accepted only when its observation ID, requested revision, camera,
kind, quality, metadata roles, dimensions, and zero-staleness claim exactly
match the submission and the fixed 64x64 service profile. The adapter encodes
the owned payload with the existing version-one `COGOBS01` codec under
`ObservationPayloadLimits::default()`, whose complete-envelope cap is 4 MiB.
It does not define another image or metadata encoding.

Successful tool output is closed structured content
`{schema_version, resource_uri, resource_size, metadata}` plus an MCP resource
link. The URI is the exact custom URI
`cogniform://observations/{observation-id}` and the media type is
`application/vnd.cogniform.observation-envelope`. The resource body is the
RFC 4648 base64 representation of the exact canonical envelope bytes.
`resource_size` and MCP `Resource.size` count decoded envelope bytes, not
base64 characters.

The bounded transport admits only one request at a time and does not read the
next input message until the current response is fully encoded, written, and
flushed. This bounds SDK handler and response-send work before a resource read
can clone or serialize another payload. It intentionally does not add
preemptive cancellation while a request is live.

The server advertises resources without subscription or list-change support.
`resources/list` deterministically returns zero resources before the first
successful observation and exactly one afterward. `resources/read` accepts
only the exact retained URI, returns exactly one binary content item, and uses
the MCP resource-not-found error for every other URI. Resource templates are
not advertised.

Retention is atomic and latest-value only. A newly encoded resource replaces
the prior resource only after the complete observation, canonical envelope,
resource descriptor, and tool result are ready. Request rejection, delivery
failure, size rejection, and output-allocation failure preserve the previous
resource. A timeout, polling failure, or causally invalid service output also
preserves that resource, marks further service-backed calls failed, and
requires the parent to discard the child rather than infer service state.
Retained-resource reads remain available until the session ends.

The observation tool uses stable payload-redacted error codes:
`invalid_arguments`, `invalid_observation`, `observation_rejected`,
`observation_failed`, `observation_timeout`, `observation_too_large`,
`service_unavailable`, `service_failed`, `invalid_service_output`, and
`output_unavailable`.

Keep stable MCP `2025-11-25`, exact-pinned `rmcp` 2.2.0 and Tokio 1.53.1, the
project-owned bounded newline transport, one fixed lazy local service, and the
local single-user inherited-stream trust boundary unchanged. The parent still
owns identity, authorization, confidentiality, freshness, rate policy,
cancellation, and child supervision.

## Consequences

- MCP discovery now returns exactly `cogniform.query_scene`,
  `cogniform.submit_imagination`, `cogniform.apply_patch`, then
  `cogniform.observe_scene`. Existing tool shapes and per-call semantics are
  unchanged.
- One camera patch followed by one exact-revision observation can yield a
  transport-neutral canonical payload without teaching the engine about MCP.
- The adapter advertises the resources capability, but no template,
  subscription, list-change, notification, history, persistence, compression,
  shared-memory, prompt, task, model, HTTP, authentication, or multi-client
  surface.
- `OBSERVE_SCENE_TOOL` is an additive public Rust constant in the unpublished
  `0.0.0` workspace. The MCP crate adds only an existing local workspace edge
  to `cogniform-observation`; CLI tests add the same existing development edge.
  No external package, feature, checksum, SDK version, protocol version,
  listener, credential, deployment, or release surface changes.
- Base64 increases the largest 4 MiB envelope to about 5.6 MiB. The existing
  8 MiB output-line bound is retained and has an equality regression proving
  that the worst valid resource-read response fits before output begins.
- Pipelined requests are backpressured at one in flight through complete
  response flush, preventing aggregate resource clones or encoded buffers from
  growing with peer input.
- Tool annotations remain pessimistic: requesting a render and replacing the
  retained resource are observable local effects, so the tool is not declared
  read-only or idempotent even though it does not mutate authoritative scene
  state.

## Status

Accepted and implemented by CF047. Ordinary official-SDK client tests cover
four-tool discovery, capability flags, closed schemas, fake-backend
observe/link/list/read behavior, exact base64, atomic replacement, unknown URI,
and prior-resource preservation on failure. Unit tests cover causal mismatch,
strict deadline edges, codec bounds, the exact default transport-output
boundary, and stalled-writer pipelined-read backpressure.
Controlled production-service and CLI child tests on an approved adapter cover
camera patching, canonical observation encoding, linked resource listing and
readback, exact metadata/revision causality, protocol-pure stdout, and clean
EOF.
