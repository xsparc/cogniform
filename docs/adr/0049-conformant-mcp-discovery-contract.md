# ADR 0049: Close MCP tool output schemas and bound workflow instructions

- Status: Accepted
- Date: 2026-08-11
- Task: CF049

## Context

CF045 introduced query and imagination tools with deterministic success output
schemas. Their runtime paths also return stable structured error envelopes, but
those envelopes were absent from the advertised schemas. CF046 and CF047 used
closed mutually exclusive success/error schemas for patch and observation, so
the four-tool discovery surface was inconsistent.

MCP 2025-11-25 requires a tool result's structured content to conform when the
tool advertises an output schema. A client that validates discovery metadata
could therefore reject every query or imagination error even though the server
returned its documented stable envelope. The initialization result's short
instruction did not provide the complete cross-tool workflow guidance, leaving
an agent client to reconstruct revision, retry, camera, resource, serialization,
and uncertain-effect policy from separate documentation.

## Decision

Advertise query and imagination output schemas as closed `oneOf` alternatives.
The success branch retains its existing top-level roles. The error branch is
exactly `{schema_version, error}` with `schema_version` fixed to one and `error`
fixed to the complete stable vocabulary for that tool. Patch and observation
retain their equivalent closed alternatives. Recursive Cogniform types remain
authoritative through deserialization and bounded canonical validation; MCP
discovery does not duplicate their nested schemas.

Return this exact 508-byte ASCII/UTF-8 server instruction during initialization:

```text
Fresh child: call query_scene with scene_revision 0. Thereafter use exact revisions from receipts or metadata. Use submit_imagination for semantic changes or apply_patch for direct changes; reuse transaction_id and idempotency_key only for an exact retry. Add a Camera before observe_scene, then read its cogniform:// resource. Calls are serialized. Discard the child after service_failed, invalid_service_output, observation_timeout, or mutating output_unavailable; never infer or retry an uncertain effect.
```

The text is self-contained inside the first 512 bytes and summarizes existing
tool and failure semantics. It grants no additional authority and does not
replace typed arguments, structured outcomes, or parent-owned supervision.

## Consequences

- Conformant clients can validate every query, imagination, patch, and
  observation structured result against advertised discovery metadata.
- Query and imagination output discovery intentionally changes from
  success-only metadata to mutually exclusive success/error alternatives.
  Runtime tool result bytes, stable error codes, and execution are unchanged.
- Official-client tests pin all four output-schema branches and the exact
  instruction bytes. A raw CLI child test independently pins the initialization
  result without constructing the service or selecting a GPU adapter.
- No MCP revision, capability, method, tool, resource, core Rust type,
  dependency, transport bound, service authority, deployment, release, or
  workspace version changes.

## Status

Accepted and implemented by CF049. Ordinary official-client and raw CLI tests
cover the exact instructions, closed schemas, stable error vocabularies, tool
order, and representative structured outcomes. Existing controlled
production-service evidence continues to cover successful query and imagination
execution.
