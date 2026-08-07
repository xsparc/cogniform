# ADR 0034: Versioned controlled-measurement JSON at the CLI boundary

- Status: Accepted
- Date: 2026-08-08
- Task: CF034

## Context

The controlled CPU world fixture has an intentionally human-readable report,
but scripts, CI helpers, and researchers need a stable representation that
does not depend on labels, whitespace, or decimal microsecond parsing. Adding
serialization to the engine would violate its typed, encoding-free boundary.
Promoting one local timing report into the public protocol would also couple a
CLI concern to the core schema before a broader diagnostics contract exists.

A separate diagnostics crate would add another public boundary for one
composition-root output. The CLI already owns presentation and already depends
directly on the workspace's exact-pinned, vendored `serde` and `serde_json`.

## Decision

`cogniform-cli measure-world --json` emits a CLI-private version-one JSON
object after the complete controlled measurement has finished. The default
human report retains its labels, ordering, microsecond formatting,
informational-only statement, and debug-profile warning.

Version one is one compact object followed by exactly one line-feed byte. Its
top-level fields appear in this order:

1. numeric `schema_version` equal to `1`;
2. string `fixture`;
3. string `profile`;
4. numeric `operations_per_sample`;
5. numeric `warmup_samples`;
6. numeric `measured_samples`;
7. string `unit` equal to `nanoseconds`;
8. boolean `informational_only` equal to `true`;
9. object `apply_total`;
10. object `validate_and_preflight`;
11. object `atomic_commit`;
12. object `render_extraction`; and
13. object `logical_hash`.

Each distribution object contains numeric `min`, `median`, `p95`, and `max`
fields in that order. Engine nanosecond values are checked before conversion
from `u128` to JSON `u64`. Serialization completes in memory before stdout is
written. Any invalid arguments or output preparation failure return nonzero
with no partial stdout. The only accepted measurement argument is the sole
optional `--json` flag.

The report identifies the fixed fixture, build profile, sample counts, and
unit, but contains no hardware identity, system metadata, threshold, baseline,
or upload destination. It reuses the CLI's existing serialization dependencies;
no manifest, lockfile, vendor source, engine encoding, or protocol schema
changes.

## Consequences

- Local automation can consume exact integer nanoseconds without parsing the
  human presentation.
- Field order, JSON types, compact line framing, distribution ordering, and
  informational-only status are compatibility-tested even though JSON object
  consumers should select fields by name.
- Changing field names, types, meaning, ordering, framing, or unit requires a
  new schema version and explicit compatibility review; version one remains
  fixed.
- Timing distributions can reveal characteristics of the local host or process
  and must not be uploaded or published by default.
- Values remain noisy observations, not release or merge thresholds. The
  existing dated baseline is not replaced by introducing this view.
- Arbitrary fixtures or sample counts, baseline management, JSON input, engine
  or protocol serialization, scenario JSON, logging, exporters, background
  sampling, hardware fingerprinting, transport, authentication, deployment,
  versioning, and release remain separate approved work.
