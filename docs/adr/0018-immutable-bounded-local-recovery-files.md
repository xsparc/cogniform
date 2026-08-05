# ADR 0018: Persist recovery through immutable bounded local files

- Status: Accepted
- Date: 2026-08-05
- Task: CF018

## Context

CF013 defines a deterministic bounded envelope that keeps replay state and the
renderer frame frontier together. CF012, CF014, and CF017 prove complete fresh
restoration, historical forks, and quiescent live replacement. Those contracts
remain memory-only unless each embedder independently implements file bounds,
overwrite policy, error redaction, synchronization, and corruption handling.

Putting path selection or filesystem I/O inside `cogniform-engine` would grant
the world/render composition root service-domain authority. A mutable
"latest" file would additionally require a cross-platform atomic-replacement,
directory-durability, retention, permissions, and freshness policy that the
current local source profile has not established.

## Decision

Add `cogniform-storage` as a separate opt-in service-domain adapter. It depends
on the public `EngineRecoveryPoint` envelope contract and `ReplayConfig`; the
engine exposes the maximum envelope byte count implied by those bounds.

`RecoveryFileStore::create_new` encodes and validates the complete envelope
before opening the caller-selected target. It uses create-new semantics, never
creates parent directories or overwrites an existing path, writes every byte,
and calls file `sync_all`. A write or sync failure closes the handle and tries
to remove the exact file created by that call. The typed path-redacted error
reports the failing operation, standard error kind, and whether cleanup removed
the partial file or it may remain. Unix creation requests mode `0600` subject
to the process umask; Windows permissions inherit the parent ACL.

`RecoveryFileStore::load` rejects a final path component that is a symlink or
not a regular file at inspection time. It checks the handle's metadata length
against the smaller of the configured envelope bound and platform addressable
capacity before reservation. It reserves for that snapshot length, reads
through a fixed stack buffer, probes one additional byte to reject growth, and
returns no point until exact length and envelope digest validation succeed.
Error and debug values retain no path or file contents.

## Consequences

- Callers can explicitly carry verified recovery state across process lifetime
  without granting filesystem authority to engine, world, renderer, or replay.
- Existing targets cannot be replaced accidentally, and corrupt, truncated,
  extended, growing, non-file, or over-limit inputs cannot become a recovery
  point.
- Successful `sync_all` is evidence about the file handle, not a portable
  guarantee that its directory entry or storage hardware survives power loss.
- Callers own parent-directory creation and trust, path authorization,
  permissions review, confidentiality, authenticity, freshness, retention,
  cleanup of any reported partial file, and selection of a point to restore.
- Automatic checkpoints/startup/rollback, overwrite or rename, snapshot
  catalogs, crash-atomic latest pointers, directory synchronization,
  encryption/signing/key management, remote/object storage, symlink-race
  guarantees, transient/asset persistence, deployment, and release publication
  remain separate work.
