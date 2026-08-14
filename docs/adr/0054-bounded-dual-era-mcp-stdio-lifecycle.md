# ADR 0054: Support bounded dual-era MCP stdio lifecycles

- Status: Accepted
- Date: 2026-08-14
- Task: CF054

## Context

ADR 0048 deliberately refreshed the exact-pinned official Rust MCP SDK without
activating its newer protocol. The accepted adapter therefore served only the
stateful MCP `2025-11-25` initialize lifecycle even though vendored `rmcp`
3.1.2 also implements the stateless MCP `2026-07-28` discovery lifecycle. The
newer lifecycle requires every request to carry its own protocol version and
client capabilities, every successful result to carry `resultType`, and
cacheable discovery, list, and read results to state explicit cache policy.

Activating the SDK defaults directly would be unsafe. A connection could move
between initialize and per-request semantics, modern requests could inherit
legacy peer state, `server/discover` could expose every SDK capability, and
mutable latest-resource results could become stale in a client cache. Replacing
the project-owned transport would also regress the response-flush,
cancellation, byte, nesting, and bounded-work guarantees from ADRs 0045, 0047,
and 0053. Conversely, keeping a manual legacy-only handshake would duplicate
the modern lifecycle and make official-client compatibility harder to prove.

## Decision

Serve both exact protocol eras over the existing inherited-stdio command and
`BoundedTransport`:

- legacy clients use exactly `initialize` with `2025-11-25`, followed by the
  existing session requests and optional `notifications/initialized`;
- modern clients use `server/discover` or any directly supported request with
  exact `2026-07-28` request metadata; every modern request independently
  supplies a decodable protocol version and client-capabilities object;
- modern `initialize`, direct legacy requests without initialization, malformed
  or missing modern metadata, unsupported versions, and later era switching
  reject before the semantic handler;
- one connection is pinned to its first accepted era. The pin selects wire
  rules only and never supplies missing modern identity or capabilities.

Retain a small bounded opening preflight before handing accepted input to
`rmcp::serve_server`. It answers pre-opening ping, preserves the existing
identified wrong-order and unsupported-initialize JSON-RPC bytes, rejects
invalid client response/error directions through the existing redacted
`invalid_message` transport category, and retains one accepted opener for the
normal transport permit path. RMCP then owns accepted initialize, discovery,
request context, cancellation-token, and service-loop mechanics.

Wrap the existing `CogniformMcpServer` in a connection-era service boundary.
The wrapper validates and pins requests before delegation and admits only
`server/discover`, `tools/list`, `tools/call`, `resources/list`, and
`resources/read` in the modern era. It advertises exactly the existing tools
and resources capabilities, no extensions, and only `2026-07-28` from modern
discovery. Declaring an unadvertised client extension does not grant authority
and does not invalidate an otherwise core-only request; extension-dependent or
unsupported methods still return method-not-found. Tasks, multi-round-trip
input, subscriptions, Apps, prompts, sampling, and model calls remain absent.

Every supported modern success carries `resultType: "complete"` and
informational `io.modelcontextprotocol/serverInfo` metadata. Discovery,
`tools/list`, `resources/list`, and `resources/read` carry `ttlMs: 0` and
`cacheScope: "private"`; the mutable latest resource is therefore immediately
stale and cannot be reused as a freshness claim. Legacy initialization,
results, tool/resource ordering and schemas, stable errors, cancellation,
backpressure, and identified pre-initialization rejection bytes remain
unchanged and omit every modern-only result field.

Keep the exact dependency graph, current-thread runtime, four tools, one
latest-value resource, lazy single `LocalService`, local trusted-parent profile,
and all existing byte, nesting, payload, polling, and response-flush limits.
This decision creates no listener, socket, authentication, remote client,
storage, release, or deployment authority.

## Consequences

- Modern agents can discover or directly invoke the exact existing surface
  without a session initialization exchange, while legacy clients retain their
  byte contract.
- A well-formed modern request is self-contained. Discovery identity and client
  extension declarations are informational and never authorize behavior.
- Conservative zero-lifetime private cache hints may cause repeated list/read
  requests. That is intentional for the mutable latest-value resource and
  remains bounded by one request through response flush.
- Modern cancellation has the same exact-ID, pre-response, response-suppressed,
  process-terminal semantics as legacy cancellation. It is not rollback or an
  effect receipt.
- Adding another protocol version, method, capability, extension behavior,
  cache lifetime, transport, client, or persistent resource requires a new
  approved decision.
- Public Rust adds `MCP_MODERN_PROTOCOL_VERSION`; the existing
  `MCP_PROTOCOL_VERSION` remains the legacy initialization constant. The
  workspace remains unpublished and this is not a release action.

## Status

Accepted and implemented by CF054. Official RMCP clients, raw duplex fixtures,
ordinary CLI children, and controlled release-mode production-service and CLI
children cover discovery, direct opening, exact metadata/result/cache roles,
all four tools, latest-resource readback, invalid directions, missing,
malformed, unsupported and mixed-era requests, extension neutrality,
cancellation, and legacy byte compatibility.
