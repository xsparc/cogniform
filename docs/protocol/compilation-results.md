# Compilation result contract

Status: schema version one implemented by CF043 for typed and canonical
in-process use. CF044 carries the same value through the separately versioned
local-session schema without adding transport concerns to this contract.

`cogniform-compilation` separates deterministic compiler output values from
the `cogniform-compiler` execution implementation. It depends only on core
protocol values and the existing exact-pinned Serde graph. It owns no world,
gateway, service, renderer, stream, endpoint, model, or persistence state.

## Result shape

One `CompilationResult` declares these fields in canonical order:

1. `schema_version`, currently exactly `1`;
2. `imagination_id`, the source request identity;
3. `scene_revision`, the exact immutable scene view compiled;
4. `patch`, one normalized `ScenePatch` or `null`;
5. `decisions`, stable ordered compiler choices;
6. `unresolved`, stable ordered failures to resolve a relation or constraint.

A compiled outcome has one valid patch and an empty `unresolved` collection.
The patch schema equals the result schema and its `base_revision` equals
`scene_revision`. An unresolved outcome has no patch and at least one issue.
There is no partial-patch outcome.

## Stable entry roles

Every decision contains `code`, `entity_key`, `relation_index`, and
`entity_id`. The stable version-one codes and optional-field roles are:

| Rank | Code | `relation_index` | `entity_id` |
|---:|---|---:|---:|
| 0 | `generated_entity_id` | absent | present |
| 1 | `preferred_entity_id_substituted` | absent | present |
| 2 | `default_name` | absent | absent |
| 3 | `default_transform` | absent | absent |
| 4 | `default_material` | absent | absent |
| 5 | `parent_relation_applied` | present | absent |
| 6 | `above_relation_applied` | present | absent |
| 7 | `right_of_relation_applied` | present | absent |

Every unresolved entry contains `code`, `relation_index`,
`constraint_index`, optional local/related keys, and optional stable entity
identity. The stable version-one codes and roles are:

| Rank | Code | `relation_index` | `constraint_index` | keys | `entity_id` |
|---:|---|---:|---:|---:|---:|
| 0 | `unknown_entity_reference` | present | absent | both present | absent |
| 1 | `self_relation` | present | absent | both present | absent |
| 2 | `conflicting_relation` | present | absent | both present | absent |
| 3 | `hierarchy_cycle` | present | absent | both present | absent |
| 4 | `placement_cycle` | present | absent | both present | absent |
| 5 | `required_entity_missing` | absent | present | both absent | present |
| 6 | `required_entity_present` | absent | present | both absent | present |
| 7 | `non_finite_placement` | present | absent | both present | absent |
| 8 | `unsupported_spatial_rotation` | present | absent | both present | absent |

Here, “keys” means both `entity_key` and `related_key`. Other substitutions
are invalid.

Canonical comparison is ascending. Text uses unsigned lexicographic UTF-8
bytes, code uses the explicit rank above, numeric fields and stable identities
use their unsigned numeric value, and every optional field orders `null`
before a present value. Decisions compare the exact tuple
`(entity_key, code_rank, relation_index)`. `entity_id` does not distinguish two
entries within that tuple, so a second identity there is a conflicting
substitution rather than another sortable decision. Different decision codes
for one entity key remain distinct entries.

Unresolved entries compare
`(relation_index, constraint_index, code_rank, entity_key, related_key,
entity_id)` under the same rules. Consequently constraint entries with a null
relation index precede relation entries. Equal adjacent tuples are duplicates
and reject. The public `canonical_cmp` methods implement these comparisons for
Rust callers.

## Canonical encoding and limits

`to_canonical_json` emits compact declaration-order JSON followed by exactly
one LF. `from_canonical_json` accepts only those exact bytes. Leading or
trailing whitespace, alternate field order, additional LF bytes, unknown or
duplicate fields, unsupported versions/codes, trailing documents, and
noncanonical nested values reject before return.

`CompilationLimits` explicitly bounds:

- complete encoded bytes;
- pre-decode JSON nesting;
- deterministic logical decoded bytes;
- aggregate scene-text bytes across the optional patch and all report entries;
- decision count;
- unresolved count;
- the nested patch through ordinary `RuntimeLimits`.

The encoder uses a bounded writer. The decoder checks bytes and nesting before
deserialization, validates every typed invariant, then bounded-re-encodes and
compares the complete input. Errors retain stable categories and field paths,
not input payloads or unbounded parser strings.

`CompilationLimits::default()` derives from ordinary
`RuntimeLimits::default()` and currently yields:

| Result limit | Default |
|---|---:|
| encoded bytes | 3,900,416 |
| logical decoded bytes | 4,658,211 |
| JSON nesting depth | 33 |
| aggregate text bytes | 393,216 |
| decisions | 1,536 |
| unresolved constraints | 768 |
| nested patch | ordinary `RuntimeLimits::default()` |

For custom runtime limits, `for_runtime_limits` derives decisions as four per
imagination entity plus one per relation, unresolved entries as relations plus
constraints, text as six times the runtime text limit, and nesting as the
larger of nine or the nested patch depth plus one. Encoded and logical limits
then add deterministic per-entry and fixed overheads to the supplied patch
limits.

## Compatibility and non-scope

`cogniform-compiler` re-exports the moved types, errors, limits, and schema
constant. Its default configuration derives report limits from its existing
runtime limits and validates the complete canonical report, including encoded
bytes and nesting, before returning it. This does not
change deterministic normalization, identities, patch bytes, gateway
admission, world/replay state, or rendering.

The contract does not itself define a session message, correlation identity,
executor action, stdio behavior, model response, external JSON Schema,
transport endpoint, authentication boundary, or release format. CF044's
separate [local-session schema](local-session-messages.md) maps the value under
explicit negotiated limits; other adapters still require separate approval.
See [ADR 0043](../adr/0043-bounded-transport-neutral-compilation-results.md),
[ADR 0044](../adr/0044-versioned-local-imagination-session-mapping.md), and the
[local gateway guide](local-gateway-and-imagination.md).
