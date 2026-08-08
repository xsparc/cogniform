# Local-session executor

CF041 maps the bounded [CF040 control schema](local-session-messages.md) to one
owned in-process [`LocalService`](local-service.md). The
`cogniform-local-executor` crate is a caller-driven state machine, not an
endpoint, stdio loop, scheduler, process supervisor, or remote security
boundary.

CF042's separate [`serve-stdio` composition](local-stdio-session.md) supplies
one fixed caller that owns inherited redirected I/O, bounded polling, deadlines,
flushes, and shutdown without changing this crate's authority.
CF044 extends the same state machine with schema-version-two imagination
commands while preserving version-one behavior.

## Lifecycle

| Phase | Accepted input | Result |
|---|---|---|
| awaiting hello | exactly one client `hello` | field-wise effective limits and active state |
| awaiting hello | any other valid client message | `protocol_state` failure |
| active | patch, imagination in version two, query, observation, or quiescent close | explicitly mapped service behavior |
| active | another hello or a duplicate live correlation | `protocol_state` failure |
| closed | any input | `protocol_state` failure; no service work |

The executor constructor rejects a service that already owns queued commands or
outstanding observations because those values have no session correlation. An
orderly close requires both executor maps and service command/observation
counters to be empty. `into_service` returns the owned service only after that
close; otherwise it returns the still-live executor.

## Limit negotiation

The hello result is the field-wise minimum of:

- the peer's advertised receive limits;
- the caller-configured local frame and runtime limits; and
- the owned service's runtime limits.

For version two, compilation limits are independently intersected across the
peer advertisement, caller-configured executor policy, and the service
compiler's runtime-derived bounds. Version one omits compilation limits
exactly. The executor installs the effective result limits into the quiescent
service compiler before admitting semantic work, so encoded, nesting, logical,
text, count, and nested-patch limit failures occur before patch application.
The negotiated schema version is fixed for the session; later messages with
another version fail before service work.

The nested per-entity component count is also capped by the effective total
component count. The effective control ceiling is recapped by effective runtime
encoded bytes, and the resulting `LocalSessionLimits` converts to the exact
`LocalFrameConfig` used for all later input and output. A peer whose effective
limits cannot represent the hello response receives no over-limit substitute
frame; the executor returns a typed `OutputRejected` to its trusted caller.

Renderer dimensions are checked against negotiated observation width, height,
and pixel limits before observation submission. Completed observation frames
are fully encoded once in memory under the effective frame/envelope limits
before return. If the completed payload cannot fit, it is consumed and replaced
by a small redacted `limit_exceeded` response under the original correlation.

## Correlation and command semantics

Only the CF039 outer non-zero value is a session correlation. Query and terminal
admission responses release immediately. Patch and imagination commands share
one typed FIFO/correlation map so gateway ordering and cross-kind latest-value
supersession remain exact. Live command behavior is:

| Gateway admission or completion | Correlation behavior |
|---|---|
| `queued` | retain the new correlation and FIFO position |
| `already_queued` | terminal for the new correlation; retain the original live mapping |
| `dropped` | terminal for the new correlation |
| `replayed` | terminal for the new correlation with the cached completion; a compiled replay has an idempotent receipt and an unresolved replay has none |
| `superseded` | reject and release the discarded correlation; retain the replacement at the same position |
| newly applied patch | emit `patch_completed` and release exactly once |
| newly compiled imagination | emit `imagination_completed` with an `applied` receipt and release exactly once |
| newly unresolved imagination | emit `imagination_completed` without a patch or receipt and release exactly once |
| processing failure | emit one stable failure and release exactly once |

`advance` removes at most one locally ordered key and calls
`LocalService::process_next` exactly once. A missing, different-key, wrong-kind,
invalid compilation, or inconsistent receipt becomes one correlated redacted
`internal` failure and releases the live correlation rather than emitting an
incorrectly correlated completion.
Replayed imagination admission returns the exact retained compilation and a
replay-marked receipt only when a patch exists; it does not call
`process_next`, recompile, or mutate the world.

## Observation semantics

An accepted request reserves its outer correlation and unique live
`ObservationId`. With no completion, the first later `advance` emits one
`observation_pending`; further empty polls are silent. A correlated engine
`ObservationDelivery` then returns either the complete CF039 observation frame
or one stable failure and releases the exact ID and correlation.

One call returns at most two frames: one command terminal result and one
observation pending/completion result. The executor does not loop, wait, sleep,
retry, create a thread, or poll automatically.

## Failure and security boundary

Malformed, wrong-direction, wrong-kind, unsupported-version, over-limit, and
invalid-state inputs become stable session failure codes when that response
fits the active output policy. Service domain failures are classified as
revision, capacity, command, query, observation, unavailable, or internal
without retaining payloads or arbitrary error strings.

The executor validates lifecycle and resource behavior; it does not establish
that a peer is allowed to act. The fixed stdio caller adds a local operation
deadline and shutdown policy, but not peer identity, authentication,
authorization, confidentiality, freshness and replay policy, rate controls,
preemptive cancellation, or partial-write recovery. Any broader or remote
endpoint must add those controls explicitly.
See [ADR 0041](../adr/0041-bounded-caller-driven-local-session-executor.md) and
[ADR 0044](../adr/0044-versioned-local-imagination-session-mapping.md), plus
the [MVP threat model](../threat-model/mvp.md).
