# ADR 0045: Bounded MCP stdio adapter

- Status: Accepted
- Date: 2026-08-09
- Task: CF045

## Context

CF043 and CF044 established bounded transport-neutral compilation outcomes,
semantic submission, exact revision queries, optional apply receipts, and
retained idempotent replay. Those semantics are now mature enough for one
standard agent-tool adapter without moving protocol or model concerns into the
authoritative engine.

MCP is a JSON-RPC protocol with a larger SDK and dependency surface than the
existing project-owned binary session. Reusing a general stdio decoder that
buffers a complete line before applying Cogniform limits would weaken the
pre-allocation boundary. Enabling HTTP, authentication, task, prompt,
resource, sampling, or process-launch features would also create authority
outside the first adapter's purpose.

## Decision

Add a separate `cogniform-mcp` service adapter and the exact CLI command
`cogniform-cli serve-mcp-stdio`. Pin the official Rust SDK `rmcp` 2.2.0 with
default features disabled and its server surface only. Pin Tokio 1.53.1 to the
minimal inherited-stdio, I/O utility, current-thread runtime, macro, and
synchronization features. The adapter locks initialization to stable MCP
`2025-11-25`; other versions fail before service creation.

Expose exactly two deterministically ordered tools:

- `cogniform.query_scene` maps one typed `SceneQuery` to a validated canonical
  `SceneQueryResult` and declares read-only, non-destructive, idempotent,
  closed-world annotations.
- `cogniform.submit_imagination` maps one typed `ImaginationEnvelope` through
  existing gateway admission and at most one `process_next`, returning the
  exact validated compilation result and optional receipt. It declares
  mutating, destructive, idempotent, closed-world annotations.

One async mutex serializes all tool calls over one lazily created fixed 64x64
`LocalService`. Initialization, tool listing, malformed input, and invalid
arguments create no engine, renderer, or GPU adapter. Replay returns the
gateway-retained result without compiling or mutating again. Submitted
imagination, patch, compilation, and receipt identity, revision, operation,
and apply/replay roles are revalidated at the adapter boundary.

Use a project-owned newline transport around the SDK service handler. It checks
input bytes incrementally with `fill_buf` before line allocation, preflights
JSON object/array nesting before serde, encodes into a bounded in-memory writer
before the first output byte, applies the same nesting preflight to output, and
flushes each JSON-RPC line. Input, output, truncation, nesting, parse, read, and
write failures have stable payload-redacted categories. Stdout carries MCP
JSON-RPC only; diagnostics use stderr.

## Consequences

- Existing local-session and `serve-stdio` bytes and behavior are unchanged.
- MCP clients can discover and invoke only bounded logical query and semantic
  imagination operations. Observation resources, patch tools, procedures,
  assets, recovery, prompts, tasks, sampling, models, sockets, HTTP, OAuth,
  multiple clients, and full-duplex policy remain outside this adapter.
- The adapter inherits no peer authentication, authorization, confidentiality,
  freshness, remote replay, rate, tenancy, or process-supervision boundary.
  A trusted local parent must own both redirected streams and child lifetime.
- The new external graph is exact-pinned, locked, vendored, and offline. It
  includes async/runtime, schema, tracing, and proc-macro support required by
  the official SDK. The enabled production surface contains no HTTP client or
  server, TLS, OAuth, socket, child-process, UUID, or random-number feature.
- `rmcp`'s vendored build script is retained byte-exact for registry checksum
  integrity. It conditionally invokes `git config` only when both `.git` and
  `.githooks` exist two directories above the crate. Cogniform has no
  `.githooks`, its public-tree policy rejects unapproved hidden root state, and
  dependency review must be repeated before that assumption changes.
- `McpServerConfig`, `McpTransportLimits`, `McpServeError`, and
  `TransportFailureKind` are additive public Rust surfaces in the unpublished
  `0.0.0` workspace.

## Status

Accepted and implemented by CF045. Ordinary tests cover byte-bound equality,
nesting scanning, malformed and truncated input, exact version locking,
official-SDK initialization and tool discovery, annotations, lazy validation
failure, exact query, compilation/application, and retained replay. CLI
black-box tests cover exact arguments, initialize/list/EOF, stdout purity, and
redacted malformed input. A controlled ignored child test covers query,
semantic mutation, replay without a second revision, final query, and clean
EOF on an approved adapter.
