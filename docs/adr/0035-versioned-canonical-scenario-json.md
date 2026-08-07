# ADR 0035: Versioned canonical-scenario JSON at the CLI boundary

- Status: Accepted
- Date: 2026-08-08
- Task: CF035

## Context

The canonical unattended scenario is the end-to-end proof of Cogniform's local
MVP, but its intentionally human-readable report requires scripts and CI
helpers to parse labels and lines. Adding serialization to the scenario engine
would violate its typed, encoding-free boundary. Promoting one executable's
proof report into the public protocol would also create a core compatibility
surface before a general diagnostics contract exists.

The CLI already owns presentation and directly uses the workspace's
exact-pinned, vendored `serde` and `serde_json`. A separate diagnostics crate
would add another public boundary for one composition-root view.

## Decision

`cogniform-cli scenario --json` emits a CLI-private version-one JSON object
only after adapter initialization, the complete canonical scenario, all three
observations, replay verification, report construction, and serialization
succeed. The command without `--json` retains its existing 19-line human
report byte for byte. The sole optional scenario argument is `--json`; invalid
or extra arguments fail before adapter selection with empty stdout.

Version one is one compact object followed by exactly one line-feed byte. Its
top-level fields appear in this order:

1. numeric `schema_version` equal to `1`;
2. string `scenario` equal to `canonical-mvp-v1`;
3. string `profile` equal to `default-local-64x64`;
4. boolean `passed` equal to `true`;
5. numeric `observation_width` and `observation_height`;
6. object `adapter`;
7. numeric `scene_revision` and `queried_entities`;
8. lowercase hexadecimal strings `table_id` and `camera_id`;
9. numeric `color_frame`, `entity_id_frame`, and `visibility_frame`;
10. lowercase `#rrggbbaa` string `center_color`;
11. lowercase hexadecimal string `center_entity_id`;
12. numeric `table_visible_pixels`;
13. lowercase hexadecimal strings `logical_hash` and
    `replayed_logical_hash`; and
14. numeric `replay_entries` and `replay_bytes`.

The nested `adapter` object contains string `name`, string `backend`, string
`device_type`, and boolean `webgpu_compliant` in that order. The view derives
only from the public backend-neutral adapter summary and successful
`CanonicalScenarioReport`; it does not change the engine, protocol, scenario,
profile, tolerances, or observation behavior. Existing CLI serialization
dependencies are reused, so no manifest, lockfile, package, or vendor source
changes.

## Consequences

- Local automation can consume the complete scenario proof without parsing
  the human presentation.
- Field order, JSON types, compact line framing, identifiers, colors, causal
  frame order, and matching live/replayed hashes are compatibility-tested even
  though JSON consumers should select fields by name.
- Changing field names, types, meaning, ordering, or framing requires a new
  schema version and explicit compatibility review; version one remains fixed.
- The adapter name and backend can fingerprint local hardware, while stable
  IDs, hashes, frame counters, colors, and pixel counts can correlate runs.
  Output is local and opt-in and must not be uploaded or published by default.
- JSON input, scenario selection/configuration, general diagnostic schemas,
  adapter selection, performance timings or thresholds, logging/exporters,
  telemetry, transport, authentication, CI expansion, deployment, versioning,
  and release remain separate approved work.
