# Local-session executor

CF041 maps the bounded [CF040 control schema](local-session-messages.md) to one
owned in-process [`LocalService`](local-service.md). The
`cogniform-local-executor` crate is a caller-driven state machine, not an
endpoint, stdio loop, scheduler, process supervisor, or remote security
boundary.

## Lifecycle

| Phase | Accepted input | Result |
|---|---|---|
| awaiting hello | exactly one client `hello` | field-wise effective limits and active state |
| awaiting hello | any other valid client message | `protocol_state` failure |
| active | patch, query, observation, or quiescent close | explicitly mapped service behavior |
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
admission responses release immediately. Live patch behavior is:

| Gateway admission or completion | Correlation behavior |
|---|---|
| `queued` | retain the new correlation and FIFO position |
| `already_queued` | terminal for the new correlation; retain the original live mapping |
| `dropped` | terminal for the new correlation |
| `replayed` | terminal for the new correlation with its idempotent receipt |
| `superseded` | reject and release the discarded correlation; retain the replacement at the same position |
| newly `applied` | emit `patch_completed` and release exactly once |
| late authoritative replay | emit terminal replayed admission and release exactly once |
| processing failure | emit one stable failure and release exactly once |

`advance` removes at most one locally ordered key and calls
`LocalService::process_next` exactly once. A missing, different-key, or
imagination response is an executor/service state invariant failure rather than
an incorrectly correlated protocol response.

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
that a peer is allowed to act. A future endpoint must add peer identity,
authentication, authorization, confidentiality, freshness and replay policy,
rate controls, deadlines, cancellation, partial-write recovery, and shutdown.
See [ADR 0041](../adr/0041-bounded-caller-driven-local-session-executor.md) and
the [MVP threat model](../threat-model/mvp.md).
