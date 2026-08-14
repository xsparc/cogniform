# ADR 0053: Bound MCP cancellation to one terminal request lifecycle

- Status: Accepted
- Date: 2026-08-13
- Task: CF053

## Context

ADR 0047 deliberately admitted one MCP request through complete response flush
so pipelined resource reads could not accumulate SDK handler tasks, retained
payload clones, or encoded output buffers. That transport stopped reading while
the active handler owned its permit. The exact MCP `2025-11-25`
`notifications/cancelled` message was therefore unreachable during the only
long-running operation: the observation tool's fixed 15 second poll. That poll
also used a blocking thread sleep on the current-thread Tokio runtime.

The exact-pinned `rmcp` 3.1.2 dependency already maps a matching cancellation
to the active request token and drops the eventual response. Allowing the SDK
to dispatch an unrestricted second request would make cancellation observable,
but would reopen the handler-task and bulk-response amplification that ADR 0047
closed. Treating cancellation as an ordinary post-flush message would preserve
the bound but provide no useful cancellation. Rollback cannot solve the
problem: direct patches and semantic commands already use atomic core
semantics, while an observation may already have been submitted when its wait
is cancelled.

## Decision

Retain one semantically dispatched MCP request. While its handler is active,
the project-owned transport may decode exactly one additional bounded input
message. If that message is `notifications/cancelled` with the exact active
numeric or string request ID, the transport delivers it immediately to RMCP,
marks the session terminal, suppresses any response or error for that ID, and
returns end-of-stream on the next receive. Input after that cancellation is
never dispatched. The optional caller-supplied reason is discarded before SDK
handling so diagnostics cannot echo it. The inherited-stdio child exits
successfully after RMCP's bounded cleanup; the parent must reap and replace it.

Any other decoded message is retained as the sole pending input and remains
undispatched until the active response is completely encoded, written, and
flushed. The transport reads no further line while that pending slot is
occupied, leaving later bytes under the fixed reader buffer and operating-
system pipe backpressure. A cancellation with a missing or different ID is
therefore an ordinary late/nonmatching notification. A cancellation received
after response writing has begun also remains backpressured until flush and is
late. This preserves the previously accepted response bytes and admits no
second handler or response buffer.

Observation polling uses cooperative Tokio time waits. The tool checks its RMCP
request token before submission, at each poll boundary, and while waiting for
the next fixed 2 millisecond poll. Cancellation after observation admission
poisons and drops the local service, does not replace the last fully completed
resource, and produces no tool response because the transport suppresses the
cancelled ID. The child is terminal, so callers cannot use retained state to
infer whether any synchronous effect completed. Cancellation does not roll
back an admitted operation or preempt synchronous core/GPU work.

The exact MCP `2025-11-25` initialization, four tools, resource model, schemas,
stable uncancelled results, byte/nesting limits, dependency graph, and local
trusted-parent authority remain unchanged. MCP `2026-07-28`, Tasks, general
deadlines, remote transport, and process restart supervision remain separate
work.

## Consequences

- A well-behaved parent can cancel a pending observation without waiting for
  its 15 second deadline, receives no response for the cancelled ID, and must
  treat the child as consumed.
- The adapter retains one active request, at most one decoded pending message,
  one bounded response, and fixed reader/pipe backpressure. Cancellation does
  not enable general full-duplex request execution.
- Parents should send a matching cancellation as the next message. If another
  message already occupies the pending slot, later cancellation bytes are not
  read until the active response completes.
- Cancellation is a lifecycle boundary, not an effect receipt. The parent must
  neither infer success/failure nor retry a possibly mutating call in the same
  child.
- Clean terminal cancellation is not a transport failure and does not add a
  structured tool error. Its optional reason text is ignored and never enters
  adapter diagnostics. Malformed, oversized, write-failed, or truncated traffic
  keeps the existing stable failure behavior.
- The public `serve_io` composition remains suitable only for controlled local
  use. The CLI-owned current-thread runtime supplies the intended process-
  terminal cleanup when the inherited-stdio session returns.

## Status

Accepted and implemented by CF053. Deterministic transport, cooperative poll,
official-client, stalled-writer, late/nonmatching-ID, response-suppression, and
retained-resource tests define the boundary. No MCP 2026 activation, new tool,
dependency change, remote authority, release action, or publication is part of
this decision.
