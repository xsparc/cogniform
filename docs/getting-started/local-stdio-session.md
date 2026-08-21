# Run a local stdio session

`cogniform-cli serve-stdio` is the first executable agent session. A parent
process launches Cogniform with both stdin and stdout piped and exchanges
binary [CF039 frames](../protocol/local-stream-framing.md). Do not invoke it in
an interactive terminal or attempt to read stdout as text.

The exact child command is:

```text
cogniform-cli serve-stdio
```

That omission selects `default-local-64x64`. A trusted parent may select one
larger immutable launch profile instead:

```text
cogniform-cli serve-stdio --profile local-256x256
cogniform-cli serve-stdio --profile local-480x270
```

No other name, flag, ordering, duplication, or positional argument is
accepted. Select the profile before launching the child; the binary session
does not negotiate or change dimensions.

When running from a source checkout, the executable can be built offline with:

```text
cargo build -p cogniform-cli --locked --offline
```

The parent owns process creation and should configure the equivalent of piped
stdin and piped stdout. Stderr may be captured or inherited for stable textual
failure diagnostics. Cogniform itself creates no process, path, named pipe,
socket, listener, shared memory, or daemon.

## Session flow

1. Send one client hello framed under the default local frame configuration.
   Use schema version one for the original patch flow, or schema version two
   and include `compilation_receive_limits` for semantic imagination.
2. Read and validate a schema-version-two server hello using the compilation
   limits advertised by the client. The effective limits returned by that
   hello then govern every later client frame and server frame. Schema version
   one retains its original default-limit flow.
3. Send one patch, a version-two `submit_imagination`, exact-revision query, or
   exact-revision observation request.
4. Read every immediate response. If the request remains live, continue reading
   until its terminal completion before sending the next request. An
   `observation_pending` response is not terminal. A queued imagination first
   yields `imagination_admission`, then `imagination_completed`. `queued` and
   `superseded` retain the new imagination correlation; `already_queued`,
   `dropped`, and `replayed` are terminal for the new correlation. A replayed
   admission already carries the cached completion.
5. When no work is live, send close and wait for the flushed `closed` response.
6. Close or reap the child process. A successful run has empty stderr and stdout
   that decodes completely as CF039 frames.

Continuously drain stdout while the child runs. The 480x270 EntityId result
contains a 2,203,260-byte envelope; waiting for process exit before reading can
fill an operating-system pipe and block both processes.

The command creates one service with the selected profile. Each live
operation has a fixed 15 second completion-polling deadline and polls no more
often than every 2 milliseconds after its immediate advance. Neither value is
configurable by any profile.

Immediate EOF before the first frame exits successfully without selecting an
adapter. Once a frame has started, truncation or corruption is fatal. EOF after
a complete pre-hello frame or after hello without negotiated close is also
fatal. On a partial write, flush failure, executor failure, device/worker
failure, or timeout, discard the child and its streams; do not retry or scan for
another frame boundary.

The polling deadline cannot interrupt adapter creation, a blocked stream
operation, or one synchronous executor call. It is not cancellation, a session
lifetime, or a production shutdown SLA.

## Trust boundary

This command is local and single-client. The parent is responsible for peer
identity, authorization, pipe permissions, confidentiality, freshness, replay
policy, sensitive output retention, signals, and process supervision. Frame
hashes detect corruption only. Do not expose this command directly as a remote
or multi-tenant service.

See the exact [stdio session contract](../protocol/local-stdio-session.md),
[local-session message schema](../protocol/local-session-messages.md), and
[MVP threat model](../threat-model/mvp.md).
