# ADR 0013: Versioned recovery-point envelope

- Status: Accepted
- Date: 2026-08-04
- Task: CF013

## Context

CF012 made `EngineRecoveryPoint` the complete in-memory restoration input: it
owns accepted-event replay bytes and the source renderer's next unreserved
frame identity. Callers could persist those two values through `from_parts`,
but had to invent their own encoding and preserve the association correctly.
An accidental mismatch, truncation, or byte change might be detected only by
later replay or frame validation, and there was no single portable value for a
storage adapter to treat atomically.

Adding filesystem persistence would also decide path ownership, permissions,
atomic replacement, crash consistency, retention, and startup policy. Those
concerns do not belong in the current engine composition boundary.

## Decision

`EngineRecoveryPoint` can encode and decode one deterministic version-one
binary envelope. The format is:

1. six-byte `CNFRCP` magic;
2. unsigned 16-bit big-endian format version;
3. unsigned 64-bit big-endian non-zero next frame identity;
4. unsigned 32-bit big-endian replay byte length;
5. the complete replay bytes; and
6. a 32-byte SHA-256 digest over a domain separator and every preceding
   envelope byte.

The fixed overhead is 52 bytes. Encoding and decoding require a `ReplayConfig`
and enforce its total replay-byte bound. Decoding rejects oversized input before
field parsing, then validates the fixed header, supported version, declared
replay bound, exact total length, non-zero frame, and digest from the borrowed
slice before copying replay bytes. Failures are typed and report only stable
categories and byte counts, never replay contents.

The digest is corruption detection, not a message-authentication code,
signature, or encryption scheme. A caller that needs authenticity or
confidentiality must supply those controls outside Cogniform. Successful
envelope decoding also does not replace `LocalService::restore`: restoration
still validates the complete canonical replay stream, hash chain, world
transitions, and frame relationship before GPU initialization.

## Consequences

- Recovery bytes and frame continuity can be transported through one
  byte-exact caller-owned value without inventing an association format.
- Header, version, bounds, length, zero frame, and corruption failures are
  rejected before replay allocation or service/GPU work.
- The format is independently versioned from the replay stream nested inside
  it; future format changes require an explicit decoder and migration decision.
- A malicious writer can replace both payload and digest. The envelope makes no
  authenticity, authorization, secrecy, rollback-protection, or freshness
  claim.
- Filesystem I/O, atomic replacement, durable startup, retention, snapshots,
  revert, log rotation, asset state, queued work, device recreation, transport,
  deployment, and release publication remain separate approved work.
