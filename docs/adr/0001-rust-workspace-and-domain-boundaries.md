# ADR 0001: Rust workspace and domain boundaries

- Status: Accepted
- Date: 2026-08-02
- Task: CF000

## Context

Cogniform must keep authoritative scene mutation, GPU ownership, and service I/O
from blocking or mutating one another implicitly. A full game-engine framework
would introduce broader lifecycle semantics than the agent-to-world-to-
observation loop requires, while raw platform APIs would create premature unsafe
and portability work.

## Decision

Use a Rust 2024 workspace pinned to Rust 1.97.1. Establish six initial packages:

```text
cogniform-protocol
  <- cogniform-world <- cogniform-replay
  <- cogniform-renderer
  <- cogniform-engine
  <- cogniform-cli
```

The manifest graph is more precise than the compact diagram: `engine` composes
the protocol, world, replay, and renderer packages; `cli` depends only on engine
and protocol. World never depends on renderer, and renderer never receives
mutable world access. Backend-neutral extraction types will live in protocol or
another dependency-neutral package if later evidence justifies one.

The workspace forbids unsafe Rust and starts without external crates. `hecs`,
`wgpu`, `glam`, and other dependencies enter only in the approved slice that
exercises their boundary.

## Consequences

- Contributors can validate architecture and tooling before runtime behavior
  exists.
- Empty skeleton crates are intentionally honest: they expose no provisional
  APIs that later work would need to preserve.
- New cross-domain dependencies require architecture review and, when
  consequential, a superseding ADR.
- The unsafe prohibition may be revised only through an approved task and ADR
  that defines the smallest audited boundary.
