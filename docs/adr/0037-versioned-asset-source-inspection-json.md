# ADR 0037: Versioned asset-source inspection JSON at the CLI boundary

- Status: Accepted
- Date: 2026-08-08
- Task: CF037

## Context

The offline asset-source command has an intentionally small human-readable
report, but scripts and local operators need a stable format that does not
depend on labels, whitespace, or line parsing. Moving this presentation into
storage, the engine, or the public protocol would broaden those typed
boundaries for one local diagnostic. A general diagnostics crate would add a
new public abstraction before another consumer requires it.

The CLI already owns presentation and already depends on the pinned, vendored
Serde packages. CF036 established the complete bounded identity check and the
privacy boundary that every output mode must preserve.

## Decision

`cogniform-cli inspect-asset --json <content-hash> <path>` emits a CLI-private
version-one JSON object after the same strict hash parsing, bounded regular-file
load, growth probe, and complete SHA-256 comparison as the human report. The
default command remains byte-for-byte unchanged. Because the hash has a fixed
position, a path whose exact filename is `--json` remains ordinary positional
input in either mode.

Version one is one compact object followed by exactly one line-feed byte. Its
fields appear in this order:

1. numeric `schema_version` equal to `1`;
2. lowercase hexadecimal string `content_hash`; and
3. numeric `source_bytes`.

The report contains no path, payload, format claim, or service state. Report
construction and serialization finish in memory before one stdout write.
Failures remain nonzero, leave stdout empty, and retain path- and payload-
redacted diagnostics.

The CLI reuses its existing direct `serde` and `serde_json` dependencies. No
package, version, vendor source, lockfile entry, public protocol, storage API,
engine encoding, renderer, or asset format changes.

## Consequences

- Local scripts can consume exact bounded byte-identity evidence without
  parsing the human report.
- Field names, types, order, meaning, and framing are compatibility-tested;
  changing them requires a new schema version and explicit review.
- The hash can correlate private content and should not be published by
  default even though neither path nor payload is present.
- JSON input, general diagnostics schemas, format validation, catalogs,
  automatic import/upload/rehydration, remote transport, authentication,
  deployment, versioning, and release remain separate approved work.
