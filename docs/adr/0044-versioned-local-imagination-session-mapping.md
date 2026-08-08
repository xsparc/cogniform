# ADR 0044: Versioned local imagination session mapping

- Status: Accepted
- Date: 2026-08-09
- Task: CF044

## Context

ADR 0043 made compilation outcomes bounded transport-neutral values, while
ADRs 0040-0042 deliberately kept the local session and inherited-stdio profile
at schema version one with patch, query, and observation work only. Carrying a
semantic request now requires an explicit version boundary, result-limit
negotiation, and correlation rules. Adding the variants to version one would
change accepted bytes and make old peers interpret a wider protocol silently.

The local gateway already owns queue admission, deterministic compilation,
optional atomic patch application, and retained idempotent responses. A
session adapter must expose those outcomes without compiling at admission,
duplicating gateway state, or applying a compiled patch twice.

## Decision

Retain every schema-version-one message and canonical byte unchanged. Add
local-session schema version two over the same direction-specific roots.
Version two accepts the version-one operation set and adds client
`submit_imagination` plus server `imagination_admission` and
`imagination_completed` variants. A negotiated session is locked to its hello
version; later cross-version messages fail without service work. Correlation
remains only in the CF039 frame header.

A version-two client hello must advertise `CompilationLimits`; the server
returns the field-wise intersection of peer policy, local executor policy, and
the service compiler's runtime-derived bounds. Version-one hello omits these
fields exactly. The executor installs the effective bounds into the quiescent
service compiler before semantic work is admitted. Version-two hello and
result-bearing server codecs require the negotiated compilation limits
explicitly, while the existing codec entry points remain available for
version-one and result-free pre-negotiation values.

Imagination admission reports queued, already queued, superseded, dropped, or
replayed work without invoking the compiler. A replay contains the exact
retained compilation and, when a patch was produced, its receipt marked
`idempotent_replay`. A new completion contains either one validated compiled
patch plus one matching `applied` receipt, or no patch, at least one unresolved
entry, and no receipt. Imagination identity, idempotency key, transaction,
revision, operation count, and receipt role must agree.

The executor owns one typed FIFO command map for patches and imaginations.
Supersession terminates the displaced outer correlation and preserves the
replacement's queue position. One `advance` processes at most one command and
one observation poll; every completion, replay, drop, supersession, or service
failure releases its correlation exactly once. The generic half-duplex stdio
loop is unchanged apart from accounting for pending imagination work.

## Consequences

- Existing version-one fixtures, decoder behavior, and stdio clients remain
  byte compatible.
- A version-two local parent can drive deterministic semantic compilation,
  mutation, query, observation, and idempotent replay through the existing
  `serve-stdio` endpoint.
- Result bounds and outer control-frame bounds both apply. If the complete
  wrapper cannot fit the negotiated frame policy, the executor returns the
  stable limit failure rather than emitting a partial result. Compilation
  encoded-byte, nesting, logical, text, count, and nested-patch limits are
  enforced before an optional normalized patch is applied.
- The local-session crate now depends on the dependency-neutral compilation
  value crate, not on compiler execution or engine state.
- The executor remains caller-driven and the CLI remains one fixed inherited
  half-duplex stream. No model, listener, socket, shared memory, process
  supervision, authentication, authorization, confidentiality, tenancy,
  deployment, version publication, or release action is added.
- Adding optional hello fields, imagination message variants, executor
  configuration/status fields, `LocalSessionValidationKind` variants, and
  `CompileError::InvalidCompilationEncoding` is source-breaking for exhaustive
  Rust construction or matching in the still unpublished `0.0.0` workspace,
  despite preserved version-one wire bytes.

## Status

Accepted and implemented by CF044. Exact version-one regression fixtures and
version-two hello, submission, compiled-completion, and unresolved-replay
fixtures cover canonical bytes. CPU tests cover mixed versions, malformed
roles and limits, every admission status, compiled and unresolved outcomes,
mixed-command supersession, service failure, replay without duplicate
processing, and exact correlation release. A controlled ignored child-process
test covers the complete version-two stdio flow on an approved adapter.
