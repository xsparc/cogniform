# MCP stdio adapter

Status: fixed local schema and runtime profile implemented by CF045, extended
with bounded direct patch application by CF046, and extended with one bounded
observation resource by CF047. CF048 refreshes the official Rust SDK while
preserving this protocol profile byte-for-byte. CF049 closes every advertised
tool result shape and adds bounded workflow instructions for agent clients.
CF053 makes one exact matching active-request cancellation observable while
keeping the child process terminal and the request pipeline bounded. CF054
adds exact MCP `2026-07-28` discovery and self-contained requests beside the
unchanged legacy lifecycle. CF064 gives the CLI composition root the same
closed launch-time profile allowlist as the binary stdio command without
changing MCP discovery or messages.

`cogniform-cli serve-mcp-stdio` serves exact MCP `2025-11-25` legacy sessions
and exact MCP `2026-07-28` stateless requests over inherited redirected
stdin/stdout. It is a local child-process adapter, not a listener or remote
security boundary. A parent owns pipe creation, child lifetime, peer identity,
authorization, confidentiality, freshness, rate policy, and supervision.

The CLI grammar is
`serve-mcp-stdio [--profile <default-local-64x64|local-256x256|local-480x270>]`.
Omission selects 64x64. Exactly one known name is accepted before runtime or
service effects; the selected dimensions remain immutable and are not exposed
as MCP negotiation or per-request authority.

## Protocol profile

Each JSON-RPC message occupies one UTF-8 JSON line terminated by LF. The
adapter accepts at most 1,114,112 input bytes and emits at most 8,388,608
output bytes per line, including LF. Object/array nesting is limited to 40,
including the JSON-RPC and tool-result wrapper around bounded core values.
Input bounds are enforced incrementally before the complete line is allocated;
output is completely encoded and checked before its first byte is written.
Every output line is flushed. The transport semantically dispatches one request
at a time. While its handler is active, it may decode one additional bounded
message: an exact matching `notifications/cancelled` is delivered immediately;
any other message occupies the sole pending slot until the active response is
completely encoded, written, and flushed. No further line is read while that
slot is occupied, so the fixed reader buffer and inherited pipe provide
backpressure. CR remains ordinary JSON whitespace.

The opening request selects one connection era:

- a legacy client sends `initialize` with exactly `2025-11-25`; ping is the
  only request accepted before it;
- a modern client sends `server/discover` or any supported tool/resource
  request with `_meta.io.modelcontextprotocol/protocolVersion` equal to
  `2026-07-28` and a decodable
  `_meta.io.modelcontextprotocol/clientCapabilities` object.

Every later modern request repeats those two fields. Optional client identity
is not inherited from discovery, and neither identity nor declared extensions
grant authority. Modern `initialize`, direct 2025 requests, missing or
malformed modern metadata, unsupported versions, and switching either way
after the first accepted request fail before semantic dispatch. Identified
legacy wrong-order and unsupported-initialize requests retain their small
pre-service JSON-RPC errors. Opening exchange, discovery, and tool listing do
not construct the local service or select a GPU adapter. The fixed service is
created lazily on the first valid tool call and all calls are serialized.

Initialization returns this exact 508-byte ASCII/UTF-8 instruction:

```text
Fresh child: call query_scene with scene_revision 0. Thereafter use exact revisions from receipts or metadata. Use submit_imagination for semantic changes or apply_patch for direct changes; reuse transaction_id and idempotency_key only for an exact retry. Add a Camera before observe_scene, then read its cogniform:// resource. Calls are serialized. Discard the child after service_failed, invalid_service_output, observation_timeout, or mutating output_unavailable; never infer or retry an uncertain effect.
```

Legacy initialization and modern discovery both return the instruction. The
first 512 bytes are therefore self-contained. It grants no capability and does
not replace the typed arguments, structured outcomes, or parent-owned
supervision policy below.

The implementation dependency is exact-pinned `rmcp` 3.1.2, but SDK support is
not adapter support. Modern discovery advertises only `2026-07-28`, tools, and
resources; it advertises no extension or Tasks capability. Only
`server/discover`, `tools/list`, `tools/call`, `resources/list`, and
`resources/read` are accepted in the modern era. Every supported modern result
contains `resultType: "complete"` and informational namespaced server identity.
Discovery, tool/resource lists, and resource reads use `ttlMs: 0` with
`cacheScope: "private"`, so the mutable latest-resource view is immediately
stale. Accepted 2025 responses remain byte-compatible and omit `resultType`,
cache hints, and namespaced server identity.

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
a fixed 2 ms cadence until a fixed 15 second deadline. Poll waits are
cooperative and observe the active RMCP request token. A completion must match
the observation ID, exact revision, camera, kind, quality, dimensions, metadata,
and zero-staleness roles. Its owned payload is encoded with the existing
version-one `COGOBS01` codec under the default 4 MiB complete-envelope bound.
The widest named profile remains inside that bound: an all-absent 480x270
EntityId observation is exactly 2,203,260 envelope bytes and 2,937,680 base64
bytes, below the fixed 8 MiB output-line limit.

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

## Cancellation

For an active request in either connection era, the parent may send MCP
`notifications/cancelled` with the exact numeric or string request ID. It must
be the next message if the parent needs prompt cancellation; a previously
decoded nonmatching message occupies the sole pending slot and keeps later
bytes unread until response flush. Missing or different IDs are ordinary
nonmatching notifications. Once response writing has begun, cancellation is
late and is read only after that response flushes.

A matching active cancellation reaches RMCP's request token, suppresses every
response for that ID, prevents pending or later dispatch, and makes the child
session terminal. The CLI exits successfully after bounded cleanup. The parent
must reap and replace the child and must not infer an effect result or retry
uncertain mutation in that process. Observation cancellation leaves the prior
fully completed resource unchanged until teardown and poisons/drops the local
service. Cancellation supplies no rollback, response, structured tool error,
general operation deadline, or reusable-session guarantee. The optional reason
text is ignored before SDK handling and never appears in diagnostics.

Prompts, resource templates, resource subscriptions and notifications,
observation history, multi-round-trip results, task execution, sampling,
elicitation, logging, custom
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

Wrong opening order, unsupported initialization, or invalid first modern
metadata returns a stable JSON-RPC error when possible and exits with
`initialization rejected`. Later missing, malformed, unsupported, mixed-era,
or unsupported-method requests receive bounded JSON-RPC errors without
semantic dispatch. A client Response or Error is an invalid direction and
terminates with `invalid_message`. Failures never include the input payload,
tool arguments, OS error text, adapter identity, or scene content. A partial
physical output remains possible after an operating system write failure; the
adapter never retries or resynchronizes that line.

See [ADR 0045](../adr/0045-bounded-mcp-stdio-adapter.md),
[ADR 0046](../adr/0046-bounded-mcp-apply-patch-tool.md),
[ADR 0047](../adr/0047-bounded-mcp-observation-resource.md),
[ADR 0048](../adr/0048-pin-current-rust-mcp-sdk-without-protocol-expansion.md),
[ADR 0049](../adr/0049-conformant-mcp-discovery-contract.md),
[ADR 0053](../adr/0053-bounded-terminal-mcp-cancellation.md),
[ADR 0054](../adr/0054-bounded-dual-era-mcp-stdio-lifecycle.md),
[ADR 0064](../adr/0064-bounded-named-stdio-profiles.md), the
[quickstart](../getting-started/mcp-stdio-adapter.md), and the
[threat model](../threat-model/mvp.md).
