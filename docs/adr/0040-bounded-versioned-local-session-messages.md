# ADR 0040: Bounded versioned local-session control messages

- Status: Accepted
- Date: 2026-08-08
- Task: CF040

## Context

CF039 safely frames opaque control bytes and complete observations, but two
local peers still need one interoperable meaning for those bytes before a
standard-input/standard-output driver can be implemented. Letting each driver
invent request, response, version, correlation, limit, or failure shapes would
move compatibility and resource admission into an ambient I/O boundary.

Observation requests were also engine-owned and named only an observation and
camera. A queued request could therefore observe a later authoritative scene
revision than its caller intended. Capacity and renderer work must not occur
before an exact-revision precondition is checked.

## Decision

Add `cogniform-local-session` over `cogniform-protocol` and
`cogniform-local-transport`. It defines separate schema-version-one client and
server message enums for hello/limits, patch submission/admission/completion,
exact-revision query/result, exact-revision observation acceptance/pending,
stable payload-redacted failure, and orderly close. Completed observation data
remains a CF039 observation frame under the originating outer non-zero
correlation ID; JSON never duplicates correlation identity.

Control messages are compact canonical JSON followed by one LF. Decode checks
the effective complete-message limit and JSON nesting before deserialization,
rejects unknown fields and unsupported versions, validates every nested core
value, re-encodes, and requires exact byte equality. Encoding uses a bounded
writer, so an over-limit value or failed allocation does not first build an
unbounded intermediate JSON vector. The effective ceiling is the minimum of
runtime encoded bytes, frame control bytes, and available complete-frame body
bytes. Hello limits are explicit, non-zero, and self-consistent.

Move `ObservationRequest` into `cogniform-protocol`, retaining an engine
re-export. Add mandatory core `SchemaVersion` and exact `SceneRevision` fields
plus bounded canonical JSON. The engine validates both before observation
capacity reservation and renderer submission, and verifies the completed
frame still represents the requested revision.

The crate interprets values but executes none of them. It opens no stream,
process, pipe, file, shared memory, listener, or socket and owns no session
state machine, scheduler, polling loop, timeout, cancellation, authentication,
authorization, confidentiality, replay protection, tenancy, or rate policy.

## Consequences

- Local peers have byte-stable direction-specific control messages and one
  outer correlation rule before endpoint ownership is introduced.
- Core patch, receipt, query, result, and observation invariants remain owned
  by `cogniform-protocol`; the session layer validates rather than duplicates
  them.
- Patch admission is distinct from committed completion, and replay receipts
  cannot be substituted for newly applied completion receipts.
- Stable failure codes intentionally omit arbitrary parser, input, endpoint,
  and internal strings.
- Version-one compatibility includes exact LF bytes, variant names, field
  order, unknown-field behavior, nesting and limit checks, and nested semantic
  validation. Incompatible changes require a new session schema version.
- A later executor must map these values to `LocalService` explicitly; a later
  stdio composition root must own frame I/O and shutdown explicitly.
- Remote, shared-memory, deployment, versioning, and release work remain
  separately approved decisions.
