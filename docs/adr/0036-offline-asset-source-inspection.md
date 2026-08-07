# ADR 0036: Offline exact-hash asset-source inspection

- Status: Accepted
- Date: 2026-08-08
- Task: CF036

## Context

Operators can persist and later bounded-load one immutable asset source through
`AssetFileStore`, but the CLI has no read-only way to verify a caller-maintained
hash-to-path mapping before choosing whether to import or upload it. Using the
asset importer for this question would combine byte-identity inspection with
GLB/PNG parsing, service mutation, and potentially later GPU work. A directory
scanner or catalog would introduce discovery, authorization, freshness,
retention, and lifecycle policy beyond one explicit local file.

The storage adapter already owns the required filesystem authority and exact
bounded-load contract. `ContentHash` already owns strict lowercase SHA-256
text parsing in the public protocol.

## Decision

`cogniform-cli inspect-asset <content-hash> <path>` accepts exactly one
lowercase 64-character `ContentHash` and one OS-native path. Argument count and
hash syntax validate before filesystem work. The command uses
`AssetFileStore::default()` to reject a final-component symlink or non-regular,
oversized, growing, unreadable, or hash-mismatched file through the existing
bounded load. It performs no GLB or PNG decoding and drops the verified bytes
without passing them to a service or renderer.

After complete verification, stdout receives this fixed human report:

```text
Cogniform asset source inspection passed
content hash: <64 lowercase hexadecimal characters>
source bytes: <bounded integer>
```

Failures are nonzero and leave stdout empty. Success and failure output omit
the caller path and source payload. Hash-mismatch diagnostics may include the
expected and computed hashes because they are the exact aggregate identity
evidence needed to repair the caller-owned mapping. A path named `--json` is
ordinary input; this slice defines no options or machine-readable schema.

The CLI moves its existing workspace `cogniform-protocol` edge from test-only
to regular use. No external dependency, package, lockfile entry, storage API,
asset format, engine, renderer, or protocol type changes.

## Consequences

- A trusted local operator can verify bounded byte identity without exposing
  the file contents or selecting a GPU adapter.
- Passing proves only that the complete regular file matches the supplied hash
  under the default source-byte bound. It does not prove format validity,
  importer acceptance, renderability, writer authenticity, freshness,
  confidentiality, authorization, or relevance to a recovery point.
- Paths and hash-to-path mappings remain caller authority. Hash values can
  correlate private assets and should not be published by default.
- JSON output, format inspection, content discovery/catalogs, manifests,
  automatic startup/import/upload/rehydration, remote transport,
  authentication, deployment, versioning, and release remain separate
  approved work.
