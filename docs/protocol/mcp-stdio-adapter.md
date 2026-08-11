# MCP stdio adapter

Status: fixed local schema and runtime profile implemented by CF045, extended
with bounded direct patch application by CF046, and extended with one bounded
observation resource by CF047. CF048 refreshes the official Rust SDK while
preserving this protocol profile byte-for-byte. CF049 closes every advertised
tool result shape and adds bounded workflow instructions for agent clients.

`cogniform-cli serve-mcp-stdio` serves stable MCP `2025-11-25` over inherited
redirected stdin/stdout. It is a local child-process adapter, not a listener or
remote security boundary. A parent owns pipe creation, child lifetime, peer
identity, authorization, confidentiality, freshness, rate policy, and
supervision.

## Protocol profile

Each JSON-RPC message occupies one UTF-8 JSON line terminated by LF. The
adapter accepts at most 1,114,112 input bytes and emits at most 8,388,608
output bytes per line, including LF. Object/array nesting is limited to 40,
including the JSON-RPC and tool-result wrapper around bounded core values.
Input bounds are enforced incrementally before the complete line is allocated;
output is completely encoded and checked before its first byte is written.
Every output line is flushed. The transport admits one request at a time and
does not read the next input message until that request's response has been
completely encoded, written, and flushed. CR remains ordinary JSON whitespace.

Initialization must request exactly protocol version `2025-11-25`. Ping is the
only request accepted before initialize. Initialization and tool discovery do
not construct the local service or select a GPU adapter. The fixed service is
created lazily on the first valid tool call and all tool calls are serialized.

Initialization returns this exact 508-byte ASCII/UTF-8 instruction:

```text
Fresh child: call query_scene with scene_revision 0. Thereafter use exact revisions from receipts or metadata. Use submit_imagination for semantic changes or apply_patch for direct changes; reuse transaction_id and idempotency_key only for an exact retry. Add a Camera before observe_scene, then read its cogniform:// resource. Calls are serialized. Discard the child after service_failed, invalid_service_output, observation_timeout, or mutating output_unavailable; never infer or retry an uncertain effect.
```

The first 512 bytes are therefore self-contained. The instruction summarizes
the existing contract; it grants no capability and does not replace the typed
arguments, structured outcomes, or parent-owned supervision policy below.

The implementation dependency is exact-pinned `rmcp` 3.1.2, but SDK support is
not adapter support. The handler advertises only `2025-11-25` and rejects
`server/discover`, Tasks methods, and per-request selection of `2026-07-28`;
it advertises no extension capability. Accepted 2025 responses omit the newer `resultType`
discriminator and per-tool execution metadata.

The server advertises tools plus resources without subscription or list-change
support. It exposes these tools in this order:

| Tool | Input | Output | Annotations |
|---|---|---|---|
| `cogniform.query_scene` | One complete core `SceneQuery` object | Success `SceneQueryResult` or stable error `{schema_version, error}` as `structuredContent` | read-only, non-destructive, idempotent, closed world |
| `cogniform.submit_imagination` | One complete core `ImaginationEnvelope` object | Success `{schema_version, admission, compilation, receipt}` or stable error `{schema_version, error}` as `structuredContent` | mutating, destructive, idempotent, closed world |
| `cogniform.apply_patch` | One complete core `ScenePatch` object | Success `{schema_version, admission, receipt}` or stable error `{schema_version, error}` as `structuredContent` | mutating, destructive, idempotent, closed world |
| `cogniform.observe_scene` | One complete core `ObservationRequest` object | Success `{schema_version, resource_uri, resource_size, metadata}` plus one resource link, or stable error `{schema_version, error}` | local effect, non-destructive, non-idempotent, closed world |

The advertised JSON Schemas deterministically fix each tool's top-level fields,
required names, success/error wrapper roles, and selected scalar constraints. They are
discovery metadata, not a duplicate recursive definition of Cogniform's core
schema. Deserialization plus the core type's bounded canonical validation is
authoritative for nested patch, imagination, query, result, receipt, and
observation values.

Query-path structured tool error codes are `invalid_arguments`, `invalid_query`,
`service_unavailable`, `service_failed`, `query_rejected`,
`invalid_service_output`, and `output_unavailable`. Imagination-path codes are
`invalid_arguments`, `invalid_imagination`, `service_busy`,
`service_unavailable`, `service_failed`, `imagination_rejected`,
`invalid_service_output`, and `output_unavailable`. Treat `service_failed` and
`invalid_service_output` as loss of trust in the child. Imagination
`output_unavailable` follows an admitted mutating call and is also uncertain;
discard the child and neither infer nor retry the effect.

`query_scene` requires the exact current revision and never mutates service
state. `submit_imagination` accepts a new command only when the adapter-owned
service queue is empty, processes at most one admitted command, and returns
`admission` equal to `queued` or `replayed`. A replay returns the retained
compilation and an `idempotent_replay` receipt without a second compile or
world revision. Invalid caller values return a small stable structured tool
error and no nested diagnostics.

`apply_patch` follows the same empty-queue, one-command, and retained-replay
rules without invoking the compiler. The adapter validates the complete patch
before lazy service creation, submits it only through `LocalService`, and
revalidates the returned receipt against the patch transaction, idempotency
key, base revision, operation count, and `applied` or `idempotent_replay`
status. A new accepted patch returns `admission: "queued"`; an exact retained
retry returns `admission: "replayed"` without another world revision.

`observe_scene` validates the complete request before lazy service creation,
submits only through `LocalService`, and polls only the correlated delivery at
a fixed 2 ms cadence until a fixed 15 second deadline. A completion must match
the observation ID, exact revision, camera, kind, quality, dimensions, metadata,
and zero-staleness roles. Its owned payload is encoded with the existing
version-one `COGOBS01` codec under the default 4 MiB complete-envelope bound.

On success, the tool returns a link to exactly one retained resource named by
`cogniform://observations/{observation-id}` with media type
`application/vnd.cogniform.observation-envelope`. `resources/list` returns
zero resources before the first success and exactly the latest resource
afterward. `resources/read` accepts only that exact URI and returns one base64
binary content item; every other URI receives the standard MCP
resource-not-found error. A later success atomically replaces the prior
resource only after its envelope and tool result are complete. There are no
resource templates, subscriptions, list-change notifications, history, or
persistence.

The success `resource_size` and MCP `Resource.size` are the number of decoded
canonical envelope bytes. They do not count the longer base64 text returned by
`resources/read`.

Patch-path structured tool error codes are `invalid_arguments`, `invalid_patch`,
`patch_rejected`, `service_busy`, `service_unavailable`, `service_failed`,
`invalid_service_output`, and `output_unavailable`. Malformed or semantically
invalid patches reject before service creation. `patch_rejected` covers
submission failures and engine world-apply errors documented to leave the
authoritative world unchanged, including stale base revisions and conflicting
idempotency use. Treat `service_failed`, `invalid_service_output`, and
`output_unavailable` as loss of trust in that child; do not infer or retry an
effect in the same process.

Observation-path structured tool error codes are `invalid_arguments`,
`invalid_observation`, `observation_rejected`, `observation_failed`,
`observation_timeout`, `observation_too_large`, `service_unavailable`,
`service_failed`, `invalid_service_output`, and `output_unavailable`.
Request rejection, per-request delivery failure, size rejection, and output
allocation failure preserve the prior resource. Timeout, poll failure, or
causally invalid service output also preserve the prior resource but poison
further service-backed calls; discard the child. A retained resource remains
readable until the session ends.

Prompts, resource templates, resource subscriptions and notifications,
observation history, `server/discover`, multi-round-trip results, task
execution, sampling, elicitation, logging, custom
models, procedure/asset/recovery
tools, HTTP, sockets, OAuth, and server-created processes are not advertised or
supported.

## Failure behavior

Malformed, oversized, over-nested, truncated, read-failed, and write-failed
transport frames terminate the session. CLI stderr uses only these stable
categories:

| Category | Diagnostic suffix |
|---|---|
| input byte bound | `input_size_exceeded` |
| output byte bound | `output_size_exceeded` |
| JSON nesting | `nesting_exceeded` |
| JSON-RPC decode | `invalid_message` |
| EOF inside a line | `truncated_message` |
| inherited read | `input_failed` |
| inherited write or flush | `output_failed` |

Wrong initialization order or protocol version returns a stable JSON-RPC
error when possible and exits with `initialization rejected`. Failures never
include the input payload, tool arguments, OS error text, adapter identity, or
scene content. A partial physical output remains possible after an operating
system write failure; the adapter never retries or resynchronizes that line.

See [ADR 0045](../adr/0045-bounded-mcp-stdio-adapter.md),
[ADR 0046](../adr/0046-bounded-mcp-apply-patch-tool.md),
[ADR 0047](../adr/0047-bounded-mcp-observation-resource.md),
[ADR 0048](../adr/0048-pin-current-rust-mcp-sdk-without-protocol-expansion.md),
[ADR 0049](../adr/0049-conformant-mcp-discovery-contract.md), the
[quickstart](../getting-started/mcp-stdio-adapter.md), and the
[threat model](../threat-model/mvp.md).
