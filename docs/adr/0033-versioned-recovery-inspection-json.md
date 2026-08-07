# ADR 0033: Versioned recovery-inspection JSON at the CLI boundary

- Status: Accepted
- Date: 2026-08-07
- Task: CF033

## Context

The offline recovery command has an intentionally human-readable aggregate
report, but scripts and local agents need a stable format that does not depend
on labels, whitespace, or line parsing. Adding encoding to the engine would
violate its typed, persistence-free boundary. Promoting this one diagnostic
report into the public protocol would also couple a local CLI concern to the
core schema before a broader diagnostics contract exists.

A separate diagnostics crate would add another public boundary and dependency
surface for one composition-root output. The CLI already owns presentation and
the workspace already pins and vendors `serde` and `serde_json`.

## Decision

`cogniform-cli inspect-recovery --json <path>` emits a CLI-private version-one
JSON object after the same bounded storage load and complete CPU restoration
preflight as the human report. The default command remains byte-for-byte
unchanged. `--` ends option parsing so a path whose exact filename is `--json`
can be inspected.

Version one is one compact object followed by exactly one line-feed byte. Its
fields appear in this order:

1. numeric `schema_version` equal to `1`;
2. string `profile`;
3. numeric `replay_entries`;
4. numeric `replay_bytes`;
5. numeric `scene_revision`;
6. numeric `next_frame`;
7. lowercase hexadecimal string `logical_hash`; and
8. lowercase hexadecimal string `final_entry_hash`.

The view is private to the CLI and derives only from `RecoveryInspection`. It
contains no path, replay payload, world value, or mutable service state. Both
output modes finish load and semantic preflight before writing stdout. Failure
is nonzero, leaves stdout empty, and retains the existing path- and payload-
redacted stderr behavior.

The CLI adds direct edges to the existing exact-pinned workspace `serde` and
`serde_json` dependencies. No package, version, vendor source, core protocol,
engine encoding, or recovery-file format changes.

## Consequences

- Local scripts can consume an exact versioned report without parsing the
  human presentation.
- Field order and compact line framing are compatibility-tested even though
  JSON object consumers should select fields by name.
- Changing field names, types, meaning, ordering, or framing requires a new
  schema version and explicit compatibility review; version one remains fixed.
- Aggregate counts, revisions, frame identities, and hashes can still be
  sensitive correlation data and must not be published by default.
- JSON input, general diagnostics schemas, profile selection, discovery,
  automatic restoration, asset validation, remote transport, authentication,
  deployment, versioning, and release remain separate approved work.
