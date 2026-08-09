# ADR 0046: Bounded MCP apply-patch tool

- Status: Accepted
- Date: 2026-08-09
- Task: CF046

## Context

CF045 exposed exact scene queries and semantic imagination through one bounded
local MCP stdio adapter. That surface cannot create camera components because
the current deterministic imagination compiler intentionally emits only its
approved primitive, transform, name, and material subset. A later MCP
observation resource would therefore be unusable from a fresh public session.

The engine and local service already accept complete version-one `ScenePatch`
values with bounded validation, atomic exact-base application, deterministic
delivery semantics, retained idempotent replay, and typed pre-mutation
rejection. Adding camera-specific compiler behavior or an implicit bootstrap
world would change narrower domain semantics merely to satisfy an adapter.

## Decision

Append one MCP tool, `cogniform.apply_patch`, after the two CF045 tools. Its
input is one complete core `ScenePatch`; its output is
success `{schema_version, admission, receipt}` or stable error
`{schema_version, error}`. The tool is mutating, destructive, idempotent, and
closed-world. These MCP annotations are interoperability hints, not
authorization controls.

The adapter parses and validates the patch under the fixed service runtime
limits before lazy service creation. It serializes access through the existing
mutex, accepts new work only while the service command queue is empty, submits
only through `LocalService::submit_patch`, and processes at most one newly
queued command. An exact retained replay returns immediately without another
`process_next` call or world revision.

Before returning success, the adapter revalidates the receipt schema and
bounds plus the submitted transaction, idempotency key, base revision,
operation count, and applied-versus-replayed status. A world-apply rejection is
reported as the stable payload-redacted `patch_rejected` tool outcome because
that engine error is explicitly pre-mutation. An unexpected empty process
result, post-commit service error, wrong response kind, or causal mismatch is
reported as `service_failed`, `invalid_service_output`, or
`output_unavailable`; a parent must discard that child rather than infer
whether an effect occurred.

Keep stable MCP `2025-11-25`, exact-pinned `rmcp` 2.2.0 and Tokio 1.53.1, the
project-owned bounded newline transport, one fixed 64x64 lazy local service,
and the local single-user inherited-stream trust boundary unchanged.

## Consequences

- MCP discovery now returns exactly `cogniform.query_scene`,
  `cogniform.submit_imagination`, then `cogniform.apply_patch`.
- A fresh session can create every component already accepted by the core patch
  contract, including a camera required by a separately approved observation
  slice. CF046 adds no observation tool or resource itself.
- Direct patches retain the engine's complete atomic validation, exact-base
  conflict policy, bounded queue, idempotency, replay, extraction, and renderer
  behavior. The adapter does not mutate a world or renderer directly.
- Existing query and imagination request/response shapes and execution paths
  are unchanged. Tool-list order changes only by appending the new tool.
- `APPLY_PATCH_TOOL` and the engine-owned
  `LocalServiceError::is_patch_rejected_without_mutation` classifier are
  additive public Rust surfaces in the unpublished `0.0.0` workspace. No
  external dependency, feature, lockfile package, protocol version, listener,
  socket, credential, model, deployment, or release surface changes.
- Direct patch authority is intentionally broader than semantic imagination.
  The trusted local parent remains responsible for choosing and authorizing
  complete patches; the adapter supplies validation and isolation, not policy.

## Status

Accepted and implemented by CF046. Ordinary tests cover exact discovery,
deterministic top-level schema metadata and annotations, authoritative typed
core validation, malformed/invalid/over-limit rejection before lazy service
creation, queued/replayed/busy and invalid-service-output mapping, and exact
receipt-role validation. Controlled official-client and CLI child tests on an
approved adapter cover camera creation, exact camera-component query,
application, exact replay without a second revision, conflicting-key and
stale-base rejection, subsequent imagination compatibility, protocol-pure
stdout, and clean EOF.
