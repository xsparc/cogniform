# ADR 0043: Bounded transport-neutral compilation results

- Status: Accepted
- Date: 2026-08-09
- Task: CF043

## Context

ADR 0007 keeps semantic imagination compilation pure and returns a normalized
patch, ordered decisions, or ordered unresolved constraints. Those result
types previously lived inside `cogniform-compiler` without a public schema,
canonical encoding, or report-specific resource limits. Making a later session
schema depend on the compiler would give a value boundary execution
dependencies; duplicating the result in a session crate would split compiler
semantics and compatibility.

A compilation outcome can contain nested protocol values and repeated scene
text. Bounding only the input imagination or optional patch does not prove
that the complete report is bounded, canonical, correctly ordered, or
semantically consistent.

## Decision

Add `cogniform-compilation` over `cogniform-protocol`, Serde, and JSON only. It
owns schema-version-one `CompilationResult`, `CompilationDecision`, and
`UnresolvedConstraint` values plus their stable snake-case codes. The compiler
continues to re-export every moved public name for source-path compatibility.

`CompilationLimits` contains independent non-zero encoded-byte, logical-byte,
JSON-depth, aggregate-text, decision-count, and unresolved-count limits plus
the core runtime limits applied to an optional patch. Its default derivation
admits every version-one compiler result allowed by the supplied imagination
and patch limits.

Encoding is compact declaration-order JSON followed by exactly one LF.
Decoding checks complete encoded bytes and JSON nesting before Serde
allocation, rejects unknown or duplicate fields and unknown codes, validates
the typed value, re-encodes it through a bounded writer, and requires exact
byte equality. No external schema or reference is fetched.

A compiled result has exactly one valid patch, no unresolved entries, the same
schema version, and a patch base revision equal to the immutable scene revision
used for compilation. An unresolved result has no patch and at least one
issue. Decision and unresolved optional fields are fixed by their code. Both
collections are strictly canonical ordered and unique. Aggregate patch/report
text and deterministic logical bytes must fit the result limits.

## Consequences

- Compilation outcomes can cross future adapters without importing compiler
  execution or duplicating result semantics.
- `cogniform-compiler` gains only a workspace-local value dependency and
  validates every completed result before return; normalization, IDs, patch
  bytes, unresolved behavior, gateway admission, world state, replay, and
  rendering are unchanged.
- Compiler re-exports preserve the moved names' source paths, but the mandatory
  result schema field and explicit compiler result limits make exhaustive
  construction source-breaking, while the new validation error changes
  exhaustive `CompileError` matching within the unpublished `0.0.0` workspace
  API.
- `ScenePatch` exposes read-only aggregate text and logical-byte measurements
  so an enclosing bounded value can account for the exact nested patch rather
  than estimating it.
- The crate performs no compilation, service/world/render mutation, I/O,
  endpoint creation, model call, authentication, deployment, version change,
  or publication.
- Local-session schema changes and executor/stdio imagination mapping require
  a separate decision and milestone.

## Status

Accepted and implemented by CF043. Exact compiled and unresolved fixtures,
canonical round trips, malformed-input rejection, all configured bounds,
code-field roles, ordering, uniqueness, outcome/revision binding, and compiler
re-export compatibility are covered by CPU-only tests.
