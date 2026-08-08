# ADR 0041: Bounded caller-driven local-session executor

- Status: Accepted
- Date: 2026-08-08
- Task: CF041

## Context

CF040 defines bounded direction-specific local-session messages but deliberately
does not execute them. Leaving each future endpoint to invent hello sequencing,
limit intersection, service-error mapping, correlation retention, command
advancement, observation polling, or close behavior would move protocol state
and resource safety into an ambient I/O boundary.

The existing observation poll also returns a request-specific asynchronous
failure without its `ObservationId`. Once a session accepts more than one
observation, an executor cannot safely release the exact outer correlation for
such a failure.

## Decision

Add a separate `cogniform-local-executor` crate over the engine, local-session,
local-transport, and protocol boundaries. One `LocalSessionExecutor` owns one
initially quiescent `LocalService`. It requires exactly one hello, intersects
the peer's advertised receive limits with the configured local frame limits
and the service's runtime limits, and then decodes and emits under that common
effective configuration.

The executor retains bounded ordered maps from live outer correlations to
queued patch keys and outstanding observation IDs. `Queued` patch admission
stays live until one explicit `advance`; `AlreadyQueued`, `Dropped`, and
`Replayed` are terminal for the new correlation. `Superseded` terminates the
discarded correlation with a redacted `CommandRejected`, keeps the replacement
at the same queue position, and retains only the replacement correlation.
Every service error maps to a stable session failure code without copying its
diagnostic string.

One `advance` processes at most one admitted command and polls at most one
observation completion, returning at most two frames. An accepted observation
may emit one `ObservationPending`; completion returns one CF039 observation
frame under the originating correlation. A completed value that exceeds
negotiated output bounds becomes a redacted `LimitExceeded` control response.
Close succeeds only after executor and service command/observation state is
quiescent, and the owned service can be recovered only after that orderly
close.

Add correlated `ObservationDelivery` polling to the engine and local service.
It retains `ObservationId` on request-specific asynchronous failure while the
existing `try_receive_observation` API preserves its prior result shape.

The executor creates no stream, process, pipe, file, shared memory, listener,
socket, thread, timer, or runtime loop. It does not authenticate or authorize a
peer, provide confidentiality or freshness, prevent remote replay, select an
endpoint, retry work, define deadlines or cancellation, deploy, version, or
release anything.

## Consequences

- Protocol state and service mapping are reusable and testable without endpoint
  authority or hidden scheduling.
- Live correlations, queue order, pending reports, output count, and every
  configured or negotiated byte/value dimension are explicitly bounded.
- Exact terminal release is preserved across applied, replayed, superseded,
  rejected, oversized, and asynchronous-failure paths.
- The schema crate remains execution-neutral; the engine remains unaware of
  session framing and correlation values.
- A later stdio or pipe composition root must explicitly own framing I/O,
  partial-write recovery, shutdown, peer identity, authorization, rate policy,
  timeouts, cancellation, and process lifecycle.
- Remote, shared-memory, deployment, versioning, and release work remain
  separately approved decisions.
