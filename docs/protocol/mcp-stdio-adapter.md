# MCP stdio adapter

Status: fixed local schema and runtime profile implemented by CF045 and
extended with bounded direct patch application by CF046.

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
Every output line is flushed. CR remains ordinary JSON whitespace.

Initialization must request exactly protocol version `2025-11-25`. Ping is the
only request accepted before initialize. Initialization and tool discovery do
not construct the local service or select a GPU adapter. The fixed service is
created lazily on the first valid tool call and all tool calls are serialized.

The server advertises only the tools capability and these tools in this order:

| Tool | Input | Output | Annotations |
|---|---|---|---|
| `cogniform.query_scene` | One complete core `SceneQuery` object | One complete core `SceneQueryResult` as `structuredContent` | read-only, non-destructive, idempotent, closed world |
| `cogniform.submit_imagination` | One complete core `ImaginationEnvelope` object | `{schema_version, admission, compilation, receipt}` as `structuredContent` | mutating, destructive, idempotent, closed world |
| `cogniform.apply_patch` | One complete core `ScenePatch` object | Success `{schema_version, admission, receipt}` or stable error `{schema_version, error}` as `structuredContent` | mutating, destructive, idempotent, closed world |

The advertised JSON Schemas deterministically fix each tool's top-level fields,
required names, success/error wrapper roles, and selected scalar constraints. They are
discovery metadata, not a duplicate recursive definition of Cogniform's core
schema. Deserialization plus the core type's bounded canonical validation is
authoritative for nested patch, imagination, query, result, and receipt values.

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

Patch-path structured tool error codes are `invalid_arguments`, `invalid_patch`,
`patch_rejected`, `service_busy`, `service_unavailable`, `service_failed`,
`invalid_service_output`, and `output_unavailable`. Malformed or semantically
invalid patches reject before service creation. `patch_rejected` covers
submission failures and engine world-apply errors documented to leave the
authoritative world unchanged, including stale base revisions and conflicting
idempotency use. Treat `service_failed`, `invalid_service_output`, and
`output_unavailable` as loss of trust in that child; do not infer or retry an
effect in the same process.

Prompts, resources, observation payloads, task execution, sampling,
elicitation, logging, custom models, procedure/asset/recovery
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
[ADR 0046](../adr/0046-bounded-mcp-apply-patch-tool.md), the
[quickstart](../getting-started/mcp-stdio-adapter.md), and the
[threat model](../threat-model/mvp.md).
