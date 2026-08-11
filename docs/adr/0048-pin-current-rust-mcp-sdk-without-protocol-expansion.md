# ADR 0048: Pin the current Rust MCP SDK without protocol expansion

- Status: Accepted
- Date: 2026-08-09
- Task: CF048

## Context

CF045 introduced the isolated MCP adapter on exact-pinned `rmcp` 2.2.0, and
CF046-CF047 completed its four-tool and one-resource surface. The official Rust
SDK has since reached stable 3.1.2 and supports the newer, breaking MCP
`2026-07-28` lifecycle as well as the accepted `2025-11-25` session model.
Combining SDK maintenance with a protocol expansion would make wire
compatibility, lifecycle authority, and dependency changes difficult to
review independently.

The newer SDK defaults to every protocol revision it knows and implements
`server/discover`, multi-round-trip results, subscriptions, and Tasks-related
types. Its `server` feature also now requires UUID version-four support. Those
capabilities must not enter Cogniform merely because the implementation
dependency is refreshed.

## Decision

Replace exact-pinned `rmcp` 2.2.0 with exact-pinned `rmcp` 3.1.2. Keep default
features disabled. Production enables only `server`; official-client tests add
only `client` and `transport-async-rw`. Keep Tokio 1.53.1 and the project-owned
bounded newline transport unchanged.

Continue to implement exactly MCP `2025-11-25`. The pre-service initialization
gate accepts only that revision, and `ServerHandler::supported_protocol_versions`
returns only `2025-11-25`. Explicitly reject `server/discover`; advertise no
extension or Tasks capability; rely on the SDK's capability gate to reject
`tasks/*`; and return only complete tool and resource results. The 2026
`resultType` discriminator is stripped from every successful 2025 response.
Per-request selection of `2026-07-28` fails with the SDK's unsupported-version
error and advertises only `2025-11-25`.

Regenerate `Cargo.lock` and `vendor/` from registry checksums. The resolved
delta removes `async-trait`, updates only `rmcp`, and adds `uuid` 1.24.0,
`getrandom` 0.4.3, and target-only `r-efi` 6.0.0. UUID version-four and operating
system randomness are compiled because upstream includes them in `server`, but
the only enabled SDK call site is task creation. Cogniform neither advertises
nor invokes that path. HTTP, authentication, socket, worker, built-in stdio,
process, request-state, macros, prompts, sampling, and remote transports remain
disabled or unimplemented.

## Consequences

- Exact four-tool order, schemas, annotations, structured outputs, resource
  list/read behavior, transport bounds, lazy service ownership, stable errors,
  CLI bytes, and the trusted inherited-stdio boundary remain unchanged.
- A raw MCP fixture now rejects `server/discover`, `tasks/get`, and per-request
  `2026-07-28`, and proves that extensions, per-tool execution metadata, and
  `resultType` are absent from accepted 2025 responses.
- The unavoidable UUID/random graph adds no Cogniform authority or ambient
  runtime call in this profile. Enabling Tasks or any other path that consumes
  it requires a separate approved ADR and dependency review.
- The exact vendored `rmcp` and `getrandom` build scripts are retained.
  `rmcp` can configure Git hooks only when an unapproved repository `.githooks`
  directory exists; `getrandom` only emits a memory-sanitizer cfg. Neither
  performs a runtime download, telemetry call, or network operation.
- No public Cogniform Rust type or wire schema changes. The workspace remains
  unpublished at version `0.0.0`; no release, deployment, or merge is
  authorized by this decision.

## Status

Accepted and implemented by CF048. Ordinary official-client and raw CLI tests
cover exact 2025 negotiation, four tools, one resource surface, newer-version
and added-capability rejection, legacy result bytes, bounds, backpressure, and
stable failures. Controlled release-mode real-service and CLI tests cover the
complete query, patch, imagination replay, observation, resource-read, and
shutdown path on an approved adapter.
