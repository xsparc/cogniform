# ADR 0042: Bounded fixed-profile stdio session

- Status: Accepted
- Date: 2026-08-09
- Task: CF042

## Context

CF039 defines bounded synchronous frames, CF040 assigns versioned control
semantics, and CF041 executes those messages against one local service without
owning an endpoint. An agent still cannot exercise that complete path through
an executable process. Adding a listener, named pipe, daemon, configurable
profile, or full-duplex runtime would introduce independent identity,
authorization, shutdown, compatibility, and operational decisions.

Standard streams are the smallest useful composition boundary, but the frame
format is arbitrary binary. Interactive console handles are not a reliable
binary channel, and a successful logical write is not guaranteed to be atomic
at the operating-system boundary. The composition root must therefore own
stream mode, sequencing, flushing, failure, and bounded polling explicitly.

## Decision

Add exactly one `cogniform-cli serve-stdio` command with no subarguments. The
complete invocation and the terminal status of both stdin and stdout are
validated before any adapter or service is created. Both streams must be
redirected or piped; stderr remains the stable textual diagnostic channel.

The command locks its inherited stdin and stdout and reads the first CF039
frame under the default local receive policy before constructing one
`default-local-64x64` `LocalService`. Immediate clean EOF before any frame byte
is a successful no-session exit with no adapter work. Any started frame that is
truncated, malformed, or corrupt fails before service construction.

One successful hello is required. Immediately after the executor handles it,
the command converts the negotiated limits to the effective frame
configuration and uses that configuration before writing the server hello and
for every later read and write. Each returned logical frame is encoded once,
written, and flushed individually. A physical write or flush failure terminates
the command without retrying the frame or attempting an in-band recovery.

The loop is physically half duplex. It reads no next request while the
executor reports a live correlation. It calls `advance` immediately, then at
most once per 2 millisecond poll interval while work remains live, writes and
flushes each result, and stops advancing only at terminal correlation release.
Each live operation has a fixed 15 second monotonic polling deadline. The final
sleep is capped by the remaining duration. The deadline does not preempt a
blocking frame read, write, flush, service initialization, or individual
synchronous executor call, and it does not cancel admitted engine work.

Clean EOF after a complete pre-hello frame or after a successful hello without
negotiated close is a nonzero protocol-state failure. A flushed `closed` frame
exits successfully without another read. Executor errors are fatal. Complete
`service_unavailable` and `internal` protocol failures are flushed once and
then cause a nonzero exit without another request. Other schema, revision,
capacity, or request failures remain terminal responses for their correlation.
Runtime stderr categories are fixed, payload-free, path-free, and do not
include nested parser, service, renderer, or I/O text. Successful stdout
contains only CF039 frames.

## Consequences

- A caller can run one local patch/query/observation agent loop through an
  ordinary child process without a new dependency or public runtime crate.
- CLI gains workspace-local executor, session, and transport dependencies; no
  external package or version changes.
- One fixed service, one client, inherited redirected streams, half-duplex
  request completion, fixed polling, and fixed profile keep resource and
  shutdown behavior reviewable.
- A failed physical write may leave a frame prefix. The command never claims
  atomic OS output, resynchronization, or whole-frame retry.
- The caller owns process creation, pipe lifecycle, peer identity,
  authorization, confidentiality, freshness, replay policy, signal handling,
  and disposal of failed streams.
- The production command creates no process, path, named pipe, file, socket,
  listener, shared memory, daemon, service installation, telemetry exporter,
  or automatic persistence.
- Full duplex, multiple clients, configurable profiles or deadlines, remote
  transport, authentication, tenancy, deployment, versioning, and release
  publication remain separately approved work.

## Status

Accepted and implemented by CF042. Controlled CPU tests prove argument and
terminal preflight, first-frame-before-adapter behavior, negotiation ordering,
half-duplex read exclusion, per-frame flush, bounded polling, fatal service
classification, EOF and stream-fault behavior, and redacted failures. A
controlled Windows release-mode child process proves the complete local
hello/patch/query/visibility/close path on an approved headless adapter.
