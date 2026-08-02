# ADR 0002: Bounded canonical JSON contracts

- Status: Accepted
- Date: 2026-08-02
- Task: CF001

## Context

The first public contracts must round-trip patches, receipts, and observations,
reject incompatible input, preserve operation order, and produce byte-stable
fixtures. They also form an untrusted-input boundary and must not introduce a
network, ECS, GPU, or generated transport dependency. Default CI runs offline
on a clean standard runner.

A bespoke dependency-free encoder would keep the graph empty, but a complete
strict decoder would duplicate well-tested parser and data-model behavior. A
transport schema such as Protobuf would freeze an adapter before the in-process
semantics have been exercised.

## Decision

Schema version 1 uses Serde 1.0.229 and `serde_json` 1.0.151 through exact
workspace pins and `Cargo.lock`. Their locked transitive sources are checked
into `vendor/`, and `.cargo/config.toml` replaces crates.io with that directory.
This preserves clean-clone offline builds and keeps dependency downloads out of
ordinary pull-request checks.

Canonical messages are one compact JSON value followed by one LF byte. Struct
declaration order defines field order; canonical message types contain no map
fields. Opaque 128-bit identifiers use exactly 32 lowercase hexadecimal
characters, avoiding JSON integer interoperability problems. Floating-point
values must be finite and normalize negative zero before entering a message.
Unknown and duplicate fields fail closed. Schema version, transaction,
idempotency, base revision, conflict policy, delivery semantics, resource
budget, receipt revisions, and observation causality are explicit fields rather
than defaults.

Decoding checks encoded byte count and JSON nesting depth before parsing. Typed
post-decode validation checks deterministic logical decoded bytes plus
operation, component, text, diagnostic, queue, dimension, and pixel limits
before a message reaches world or render behavior. Logical decoded size counts
the versioned scalar widths, 16-byte opaque identifiers, four-byte collection
and string lengths, UTF-8 payload bytes, variant tags, and present optional
values; it deliberately excludes platform-dependent allocator overhead. The
encoded byte cap bounds parser allocation while collection caps bound container
overhead. Exact fixtures cover patches, receipts, and observation metadata.

## Consequences

- The protocol crate remains backend- and transport-neutral while providing an
  idiomatic Rust round-trip boundary.
- Wire changes must preserve schema version 1 fixtures or introduce a new
  explicitly supported schema version and compatibility tests.
- Serializer upgrades are deliberate dependency changes whose fixture diff,
  licenses, advisories, and vendored source changes must be reviewed.
- The repository carries roughly six megabytes of vendored source in exchange
  for deterministic offline CI. The normal quality workflow remains one job;
  dependency auditing is risk-triggered for manifest or lockfile changes.
- Public message fields are intentionally concrete. Asset references, bulk
  observation bytes, remote transport, and generated schemas remain later
  adapters.
