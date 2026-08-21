# Local stdio session

CF042 composes the [bounded local frame](local-stream-framing.md),
[local-session messages](local-session-messages.md), and
[caller-driven executor](local-session-executor.md) into one executable
`cogniform-cli serve-stdio` command. It is a fixed local child-process
composition root. CF064 adds a closed launch-time profile allowlist without
changing the stream protocol. It is not a listener, daemon, remote protocol, or process
supervisor.

## Invocation and stream ownership

The invocation grammar is:

```text
cogniform-cli serve-stdio [--profile <default-local-64x64|local-256x256|local-480x270>]
```

Omission selects `default-local-64x64`. The flag, one exact Unicode allowlist
name, and no following value are the only accepted arguments. Missing,
unknown, non-Unicode, reordered, duplicate, or extra values reject with a
stable allowlist error that does not echo input. Help is available only through
the ordinary top-level `cogniform-cli --help` command. Argument validation
finishes before standard-stream inspection, runtime creation, or adapter
selection.

The command owns only its inherited stdin and stdout for the session lifetime.
Both must be redirected or piped; if either is an interactive terminal, the
command fails before reading input or creating a service. Stderr is reserved
for one stable textual runtime diagnostic. A successful run writes no text,
adapter identity, path, payload, or log line to stdout.

## Startup and negotiation

1. Lock inherited stdin and stdout.
2. Read one frame with `LocalFrameConfig::default()`.
3. If EOF occurs before any header byte, exit successfully without constructing
   a service or selecting an adapter.
4. Otherwise create exactly one `LocalService` with the selected immutable
   profile dimensions and wrap it in one `LocalSessionExecutor`.
5. Pass the already-decoded frame to the executor. Exactly one valid hello must
   precede ordinary work.
6. After a successful hello, install
   `executor.negotiated_limits().to_frame_config()` before encoding or writing
   the server hello. Use that effective configuration for every later read and
   write.

Truncation after any frame byte, invalid framing, corruption, allocation
failure, or input I/O failure is fatal. A complete pre-hello request may receive
one protocol failure, but EOF after it is nonzero because a frame was already
received and adapter work began.

## Half-duplex scheduling

Every immediate executor output is encoded completely through CF039, physical
short writes are completed within that one logical write attempt, and stdout
is flushed before the next output or input. After an accepted patch,
version-two imagination, or observation, the
command reads no next request while `live_correlations` is nonzero. It drives
the executor to a terminal result first:

- the first `advance` is immediate;
- if work remains, the next poll is separated by a positive sleep of at most
  2 milliseconds;
- the final sleep is capped by the remaining deadline;
- the fixed deadline is 15 seconds per live operation;
- `observation_pending` is nonterminal and appears at most once under the
  executor contract;
- terminal completion or failure releases the correlation before another
  frame is read.

The polling deadline is not an I/O, initialization, session-lifetime, or idle
deadline. It cannot interrupt one synchronous `advance`, blocked read, blocked
write, blocked flush, or adapter creation. Timeout terminates the process; it
does not cancel or retry admitted work and does not emit an additional protocol
frame.

## Close, EOF, and failures

A valid quiescent close produces one `closed` frame. The command flushes it and
exits successfully without another read. Clean EOF after hello without close is
nonzero. Clean EOF while still awaiting hello is successful only when no frame
was ever received; EOF after a complete pre-hello failure is nonzero.

The following stderr messages are the complete runtime diagnostic vocabulary,
prefixed by the CLI with `error: `:

| Category | Stable message |
|---|---|
| terminal misuse | `serve-stdio requires redirected standard input and output` |
| input framing/read | `serve-stdio input frame rejected` |
| service initialization | `serve-stdio local service unavailable` |
| fatal service/worker/device response | `serve-stdio local service failed` |
| executor invariant/configuration | `serve-stdio session executor failed` |
| physical frame output | `serve-stdio output frame failed` |
| physical flush | `serve-stdio output flush failed` |
| live-operation polling deadline | `serve-stdio operation timed out` |
| EOF after a pre-hello frame | `serve-stdio session ended before hello` |
| EOF after successful hello | `serve-stdio session ended before close` |

`service_unavailable` or `internal` executor responses are themselves complete
protocol frames. The command flushes the frame, then exits nonzero without
reading again. A partial physical write cannot be repaired or authenticated;
the peer must discard the failed process and stream. No whole-frame retry,
resynchronization scan, close attempt, or secondary error frame follows an
output failure.

## Security and non-goals

The frame digest detects accidental corruption. It does not authenticate or
authorize the peer, establish freshness, prevent replay, or encrypt stdout.
Observation and scene values can be sensitive; the parent process owns pipe
endpoints, permissions, retention, and disclosure.

The command does not create a process, path, named pipe, file, socket,
listener, shared-memory allocation, daemon, service installation, telemetry
exporter, or persistent record. It supports one client, one service,
half-duplex operation, and the versioned local
patch/imagination/query/observation/close schema. Version-two imagination uses
the existing deterministic compiler and gateway; the stream driver does not
compile, apply, replay, or schedule independently. Procedures, asset,
recovery, administrative operations, remote
transport, authentication, confidentiality, tenancy, network rate policy,
deployment, stable versioning, and release publication are outside these
bounded profiles.

See [ADR 0042](../adr/0042-bounded-fixed-profile-stdio-session.md),
[ADR 0064](../adr/0064-bounded-named-stdio-profiles.md), the
[version-two mapping decision](../adr/0044-versioned-local-imagination-session-mapping.md), the
[getting-started guide](../getting-started/local-stdio-session.md), the
[failure guide](../operations/failure-and-recovery.md), and the
[threat model](../threat-model/mvp.md).
