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

## Approved PNG decoder

CF029 admits exact-pinned `png` 0.18.1 as a runtime dependency only inside
`cogniform-assets` for the strict embedded base-color image boundary;
`cogniform-renderer` and `cogniform-engine` use it only to generate test
fixtures. It is dual licensed `MIT OR Apache-2.0`, uses the pure-Rust miniz
backend with the direct crate's default features disabled, and requires no
runtime filesystem, network, telemetry, paid service, native library, or proc
macro. The locked seven-package addition is `png`, `crc32fast`, `fdeflate`,
`flate2`, `miniz_oxide`, `adler2`, and `simd-adler32`; the already-reviewed
`bitflags` and `cfg-if` packages are reused. All exact registry checksums and
sources are committed in `Cargo.lock` and `vendor/`.

The review records one transitive build script: `crc32fast` invokes the
configured Rust compiler with `--version` and emits only the ARM-intrinsic cfg
for supported compiler versions. The `png`, `fdeflate`, `miniz_oxide`, and
`adler2` crates forbid unsafe code. `crc32fast`, `simd-adler32`, and the
pure-Rust `flate2` compatibility layer contain guarded unsafe SIMD or pointer
implementations; no native backend is selected and no first-party unsafe code
is admitted. These optimized checksum/decompression paths remain inside the
same exact-pinned parser boundary.

Decoder identity transformations, ignored text/ICC payloads, checked PNG
framing/checksums, a fixed RGB/RGBA subset, and project-owned dimension, pixel,
working-memory, decoded-byte, and residency limits constrain the parser. The
optional `zlib-rs` feature is not enabled. Wider image formats, decoder
features, or backend changes require a new review.

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
