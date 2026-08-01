# Dependency Policy

Cogniform keeps its dependency graph small, reviewable, and compatible with the
repository's Apache-2.0 distribution terms.

## Admission criteria

A dependency needs:

- a concrete architectural boundary or capability that is impractical to own;
- active maintenance and a security history proportionate to its role;
- bounded features with default features disabled when they add unused surface;
- a compatible SPDX license and no field-of-use restriction;
- a crates.io release and locked checksum, unless a reviewed ADR approves a
  specific immutable source revision;
- no hidden paid service, telemetry, runtime download, or network requirement.

Direct dependencies must not use wildcard versions. Git dependencies and
unknown registries are denied by default. Native code, build scripts, proc
macros, unsafe code, parsers, and GPU-facing packages receive additional review.

## License baseline

`deny.toml` permits a conservative set of permissive licenses commonly
compatible with Apache-2.0: Apache-2.0 (including the LLVM exception), MIT,
BSD-2-Clause, BSD-3-Clause, ISC, Zlib, Unicode-3.0, and CC0-1.0. A new license
requires explicit review; an exception must name the exact package and reason.
Strong copyleft, source-available, non-commercial, and custom terms are not
accepted by default.

Automated license output is evidence, not legal advice. Contributors must also
inspect unusual license files, notices, and bundled native or generated code.

## Review and verification

`Cargo.lock` is committed. Manifest, lockfile, or policy changes trigger the
normal quality job and should additionally run:

```text
cargo deny check advisories bans licenses sources
```

The dependency audit is risk-triggered rather than installed on every ordinary
pull request. This avoids repeated tool downloads and advisory-index traffic
while there is no changing external dependency graph. It becomes a required
check when the dependency surface and measured update rate justify it.
