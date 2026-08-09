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

## Existing serialization reuse

CF033 adds direct `cogniform-cli` edges to the workspace's existing
exact-pinned and vendored `serde` 1.0.229 and `serde_json` 1.0.151 packages for
one CLI-private versioned recovery-inspection report. It enables no new feature
and adds no package, version, checksum, build script, unsafe code, native code,
runtime download, network, telemetry, or paid-service requirement. Encoding
stays in the composition root; the engine and recovery protocol remain typed
and encoding-free.

## Existing local-framing reuse

CF039 adds one workspace-local `cogniform-local-transport` crate and reuses the
workspace's existing `cogniform-protocol`, `cogniform-observation`, `sha2`,
`serde`, and `serde_json` graph. It adds no external package, version,
checksum, build script, unsafe code, native code, runtime download, network,
telemetry, or paid-service requirement. The adapter owns only framing and
caller-supplied `std::io`; endpoint and session policy remain outside the
crate.

## Existing local-session reuse

CF040 adds one workspace-local `cogniform-local-session` crate over the
existing workspace-local protocol and local-transport crates. It reuses the
exact-pinned vendored `serde` and `serde_json` packages already in the lockfile
and adds no external package, version, checksum, build script, unsafe code,
native code, runtime download, network, telemetry, or paid-service
requirement. Its test-only observation edge constructs an existing frame kind;
the production crate owns only typed values, canonical bounded JSON, and frame
adaptation without I/O or execution.

## Existing local-executor reuse

CF041 adds one workspace-local `cogniform-local-executor` crate over the
existing engine, protocol, local-session, and local-transport crates. It adds
no external package, version, checksum, build script, unsafe code, native code,
runtime download, network, telemetry, or paid-service requirement. The engine
adds an additive correlated observation-delivery value, and local transport
adds one constructor for its existing payload-bound configuration so the
session crate preserves its production dependency boundary. The executor owns
only bounded in-memory state and one supplied local service; endpoint I/O and
runtime scheduling remain outside the crate.

## Existing stdio composition reuse

CF042 adds workspace-local `cogniform-local-executor`,
`cogniform-local-session`, and `cogniform-local-transport` edges to the
existing `cogniform-cli` composition root. It adds no external package,
version, checksum, feature, build script, unsafe code, native code, runtime
download, network, telemetry, or paid-service requirement. Standard-library
I/O locking, terminal detection, flushing, monotonic time, and sleep supply the
fixed local runtime policy; reusable protocol, transport, executor, engine,
and renderer ownership remains unchanged.

## Existing compilation-result reuse

CF043 adds one workspace-local `cogniform-compilation` crate over the existing
workspace-local `cogniform-protocol` crate and reuses the exact-pinned vendored
`serde` 1.0.229 and `serde_json` 1.0.151 packages. `cogniform-compiler` gains
only that local value dependency. No external package, version, checksum,
feature, build script, unsafe code, native code, runtime download, network,
telemetry, paid service, model, endpoint, or persistence requirement is added.
The crate owns bounded typed values and canonical JSON only; compiler execution
remains in `cogniform-compiler`.

## Existing imagination-session reuse

CF044 adds workspace-local `cogniform-compilation` edges to
`cogniform-local-session` and `cogniform-local-executor`, plus a test-only edge
from `cogniform-cli`. It adds no external package, version, checksum, feature,
build script, unsafe code, native code, runtime download, network, telemetry,
paid service, model, endpoint, or persistence requirement. The session crate
owns only the typed version-two mapping and bounded canonical validation; the
executor delegates execution to the existing local service, and the CLI edge
constructs controlled test values only.

## Approved MCP SDK and async runtime

CF045 admits exact-pinned `rmcp` 2.2.0 and Tokio 1.53.1 for the isolated
`cogniform-mcp` adapter. `rmcp` is the Apache-2.0 official Rust MCP SDK and the
selected version implements stable MCP 2025-11-25 under the workspace's Rust
1.97.1 toolchain. Default SDK features are disabled. Production enables only
`server`, which requires schema support and the async read/write codec; the
adapter deliberately does not enable macros, built-in stdio, child process,
HTTP client/server, reqwest, TLS, OAuth/auth, UUID, random, worker, or network
transport features. The project-owned transport supplies the stronger
incremental line and nesting bounds. The SDK `client` feature is added only for
official-client tests. That development edge also names
`transport-async-rw`, which production already inherits from `server`;
production does not use the SDK stdio wrapper.

Tokio enables only inherited stdio, I/O utilities, macros, a current-thread
runtime, and synchronization. No time, signal, filesystem, process, socket,
network, or multi-thread runtime feature is requested directly. The 30 new
external lock entries are the SDK/runtime plus their async, schema, tracing,
proc-macro, and platform-time support: `rmcp`, `tokio`, `tokio-macros`,
`tokio-util`, `tokio-stream`, `bytes`, the Futures crates, `async-trait`,
`chrono`, `iana-time-zone`, `iana-time-zone-haiku`, `core-foundation-sys`,
`schemars`, `schemars_derive`, `serde_derive_internals`, `dyn-clone`,
`ref-cast`, `ref-cast-impl`, `pastey`, `tracing`, `tracing-core`,
`tracing-attributes`, `cc`, `find-msvc-tools`, and `shlex`. All are permissive
under the existing allow-list; exact checksums and sources are committed in
`Cargo.lock` and `vendor/`.

The graph adds proc macros and reviewed unsafe internals in the established
Tokio/Chrono/Tracing/Futures ecosystem, but no first-party unsafe code or
native runtime library. Target-only IANA/Haiku support accounts for the `cc`
tooling edge; it is not built on the declared Windows/Linux profile. `rmcp`
and `ref-cast` contain build scripts. `ref-cast` writes one generated private
module and queries the configured compiler version. The byte-exact vendored
`rmcp` script conditionally invokes `git config` only when both `.git` and
`.githooks` exist two directories above its manifest. Cogniform has no
`.githooks`, and the public-tree policy rejects unapproved hidden root state;
adding that directory or changing SDK source/features requires renewed supply-
chain review before build execution.

The SDK and runtime perform no paid call, telemetry, runtime download, or
ambient network operation in the enabled adapter. They remain behind one local
single-user inherited-stdio boundary; broader MCP capability or transport
features require a new ADR and dependency review.

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
