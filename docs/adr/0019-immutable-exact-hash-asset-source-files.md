# ADR 0019: Persist exact-hash asset sources through immutable bounded files

- Status: Accepted
- Date: 2026-08-05
- Task: CF019

## Context

CF015 deliberately restores logical asset references without CPU-decoded or
GPU-resident state. Exact matching source bytes must be supplied again before
dependent rendering resumes. CF018 can persist a complete recovery envelope,
but it intentionally excludes asset sources. After a process restart, an
embedder therefore needs an independently safe way to retain those exact bytes
without moving filesystem authority into the engine or asset decoder.

Bundling recovery and asset bytes would create a second recovery format and
couple world history to potentially large bulk content. Discovering content by
hash, maintaining a mutable cache, or selecting a "latest" snapshot would also
require catalog, retention, replacement, freshness, and crash-durability
policies that the local source profile has not established.

## Decision

Extend `cogniform-storage` with `AssetFileStore`, an opt-in adapter for one
caller-selected file containing the exact source bytes of one known
`ContentHash`. Recovery envelopes and asset files remain independent. The
caller owns the association between logical hashes and authorized paths and
explicitly drives load, import, and upload after service restoration.

`AssetFileStore::create_new` checks the configured non-zero source-byte limit
and computes the exact SHA-256 identity before opening the target. A mismatch
or oversized source therefore performs no filesystem operation. It then uses
the same private create-new, complete-write, `sync_all`, Unix `0600`, and
best-effort partial-cleanup machinery as recovery files. An existing target is
never overwritten. The path-redacted receipt reports only the content hash and
source byte count.

`AssetFileStore::load` rejects a final path component that is a symlink or not
a regular file at inspection time. It checks handle metadata against the
smaller of the configured source bound and platform-addressable capacity,
reserves only that snapshot size, reads through a fixed stack buffer, and
probes one additional byte to detect growth. It returns no bytes until the
complete bounded source hashes to the caller-supplied identity. Loading does
not decode, import, upload, or mutate a service.

## Consequences

- A caller can preserve exact GLB source bytes across process lifetime and use
  them to rehydrate a restored logical reference without changing revision,
  replay bytes, or logical hash.
- Recovery and bulk asset persistence retain separate limits and failure
  domains; one recovery point may reference zero or many independently managed
  asset files.
- Callers own path authorization, parent-directory trust, the hash-to-path
  mapping, permissions, confidentiality, authenticity, freshness, retention,
  cleanup of any reported partial file, and rehydration scheduling.
- Successful `sync_all` is not a portable directory-entry or power-loss
  guarantee. The digest detects substitution or corruption against an expected
  public identity; it does not authenticate the writer or encrypt the source.
- Directory creation, overwrite/rename/delete, hash discovery, catalogs,
  mutable caches, eviction, automatic checkpoint/startup/rehydration,
  background work, path-race guarantees, directory synchronization,
  encryption/signing/key management, remote/object storage, wider asset
  formats, deployment, and release publication remain separate work.
