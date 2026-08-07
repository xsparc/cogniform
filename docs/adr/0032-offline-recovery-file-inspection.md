# ADR 0032: Offline recovery-file inspection

- Status: Accepted
- Date: 2026-08-07
- Task: CF032

## Context

An immutable recovery file already has bounded regular-file loading and an
integrity-protected envelope, but those checks alone do not prove that its
complete replay is semantically restorable under the intended engine limits.
The only existing complete preflight was embedded in fresh-service restoration,
so an operator had to reach adapter selection and GPU initialization after the
CPU checks merely to diagnose a local file.

Lower-level envelope or replay-prefix inspection is insufficient. It can miss
configuration, complete-tail, authoritative-world, logical-hash, or frame-
frontier failures that the restoration boundary must reject. Full service
construction is also the wrong diagnostic boundary because it adds GPU and
worker availability without improving recovery-file evidence.

## Decision

`cogniform-engine` exposes synchronous `inspect_recovery_point` and the
aggregate `RecoveryInspection` value. Inspection calls the same private
restoration preflight as `CogniformEngine::restore`: it validates engine
configuration, requires a complete replay stream, validates the next-frame
frontier, and restores the log into a fresh authoritative world. It performs
no adapter selection, GPU initialization, observation-worker construction,
asset loading, or service adoption.

The result contains only the verified replay entry and byte counts, final
scene revision, next frame identity, final logical scene hash, and final replay-
entry hash. It retains no replay payload, entity value, path, world, renderer,
or mutable service state.

`cogniform-cli inspect-recovery <path>` composes that CPU boundary with
`RecoveryFileStore::load`. The command accepts exactly one OS-native path and
uses the declared `default-local-64x64` engine profile. It is read-only and
prints only the aggregate result; both success and failure output omit the
path and replay payload. This diagnostic composition permits `cogniform-cli`
to depend on the public `cogniform-storage` interface as well as the engine.

## Consequences

- Operators can distinguish envelope/storage failure from complete semantic
  replay or frame-frontier failure without a compatible GPU.
- Inspection is deterministic and bounded by the same replay and world limits
  as restoration, but it deliberately reconstructs the complete world and
  verifies the log again to produce final aggregate evidence.
- A passing result means the file is restorable through the CPU preflight only.
  It does not prove GPU compatibility, asset residency, writer authenticity,
  freshness, authorization, or later service initialization.
- The command's fixed profile must match the recovery producer's limits. Profile
  selection, JSON output, file discovery, catalogs, latest pointers, automatic
  startup/restore, asset association/rehydration, remote transport, and
  authentication remain separate approved work.
- `RecoveryInspection` and the CLI's internal storage dependency are additive
  surfaces in the unpublished `0.0.0` workspace. No version, deployment, or
  release action is taken.
