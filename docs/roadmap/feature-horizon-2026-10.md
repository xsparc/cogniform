# Feature horizon: October 2026

Status: accepted planning evidence; program implementation authorized for
dependency-ordered bounded delivery on 2026-10-06

This brief records ten research-backed feature candidates beyond the current
CF083 baseline. The maintainer authorized implementing the complete program,
but that authority does not collapse the dependency graph or evidence gates.
Every candidate still enters its own bounded task and pull request, and every
consequential contract still requires an ADR. F05 remains measurement-gated
and F10 remains threat-model-and-ADR-gated before their implementation tasks.

## DirectionBriefV1

- `schema_version`: `1`
- `baseline_commit`: `11de27aa4f14bbd23bfbf804559ea28d75383154`
- `scanned_at`: `2026-10-06`
- `coverage`:
  - Cogniform's software design, implementation plan, accepted ADRs through
    ADR 0083, current asset/renderer/protocol boundaries, tests, and recent
    history;
  - the current Khronos glTF 2.0 core specification, ratified extension
    registry, and official conformance/sample assets;
  - current standards for scene hierarchy, cameras, material variants,
    visibility, instancing, animation, selection, metadata, and declarative
    interactivity; and
  - the maintainer need for concrete rendered examples and agent-oriented use
    cases without broadening the headless-first product direction.

Source dates below distinguish dated registry snapshots from living
specifications. Where an official extension page does not restate its original
ratification date, the brief says so instead of inventing one.

## Ten feature proposals

### F01. Bounded glTF scene-graph import

- `finding`: Import one bounded default glTF scene, including node hierarchy,
  transforms, repeated mesh instances, and deterministic source-node identity,
  instead of accepting only one isolated mesh primitive.
- `source_event_date`: `2026-09-18` for the current glTF 2.0 registry build.
- `citations`: The glTF core specification defines scenes, nodes, hierarchy,
  transforms, meshes, cameras, and their composition in the runtime asset
  format ([glTF 2.0 specification](https://registry.khronos.org/glTF/specs/2.0/glTF-2.0.html)).
- `confidence`: high.
- `relevance`: This closes the largest gap between Cogniform's strong material
  support and ordinary authored GLB content.
- `direction_alignment`: Strong when imported nodes become immutable,
  content-addressed render instances and map to stable public identities
  without exposing loader or GPU handles.
- `opportunity_or_risk`: The opportunity is whole-product, assembly, and room
  import. Risks are hierarchy bombs, ambiguous instance identity, unsupported
  node payloads, and accidental mutation authority.
- `engineering_cost`: high; it crosses asset validation, extraction, renderer
  residency, identity, replay, eviction, and diagnostics.
- `disposition`: adopt.
- `roadmap_delta`: Add a staged scene-import feature family before every other
  imported-node proposal in this brief. [ADR 0084](../adr/0084-bounded-gltf-scene-import-identity.md)
  selects pure default-scene-to-patch compilation, deterministic
  instance-scoped node identities, positive TRS, explicit
  scene/node/depth/instance limits, and meshes already inside the accepted
  subset. Decode/retention, pure patch compilation, and service/render proof
  remain separate dependency-ordered slices.
- `verification_needed`: Negative hierarchy and limit fixtures; repeated-mesh
  identity tests; exact replay/hash tests; eviction and rehydration tests; and
  controlled renders from Khronos multi-node sample assets.
- Rendered example: import a furnished room GLB in which chairs reuse one mesh
  at several node transforms while every chair remains separately identifiable
  in the entity-ID observation.
- Use cases: digital-twin inspection, product assemblies, architectural rooms,
  and agents reasoning about authored object groupings.

### F02. Synchronized multi-camera observation sets

- `finding`: Let one bounded observation request name several cameras and
  attachment kinds, producing an aggregate whose outputs all share one exact
  scene revision and render epoch.
- `source_event_date`: `2026-09-18` for the current glTF 2.0 registry build.
- `citations`: glTF assets may define multiple perspective or orthographic
  cameras attached to scene nodes, providing a portable source of authored
  viewpoints
  ([glTF camera specification](https://registry.khronos.org/glTF/specs/2.0/glTF-2.0.html)).
- `confidence`: high.
- `relevance`: Cogniform can make one exact-camera observation today, but an
  agent assembling a spatial picture must issue independent renders that may
  no longer describe one shared moment.
- `direction_alignment`: Very strong if the aggregate has fixed camera, pixel,
  attachment, byte, and in-flight-work caps and retains per-output causality.
- `opportunity_or_risk`: It improves spatial coverage and dataset generation;
  risks are aggregate GPU/readback pressure, partial completion, cancellation
  complexity, and accidental claims that separately submitted frames are
  simultaneous.
- `engineering_cost`: medium-high; observation contracts, scheduling,
  readback, cancellation, envelopes, local/MCP presentation, and limits change.
- `disposition`: adopt.
- `roadmap_delta`: Define one transport-neutral batch contract before exposing
  it through any adapter. No arbitrary camera count, dimensions, streaming, or
  implicit camera creation belongs in the first slice.
- `verification_needed`: Same-revision/extraction proof, deterministic camera
  ordering, all-or-error completion policy, aggregate byte/work preflight,
  cancellation, resource preservation, and four-view controlled renders.
- Rendered example: one request returns front, side, top, and perspective color
  plus entity-ID images of the same mechanical assembly revision.
- Use cases: object reconstruction, inspection coverage, synthetic datasets,
  before/after comparison, and agent verification of hidden-side edits.

### F03. Agent-selectable material variants

- `finding`: Retain bounded `KHR_materials_variants` mappings and let an asset
  instance select one named authored variant through an ordinary revisioned
  patch, without duplicating geometry or source assets.
- `source_event_date`: Official ratified-extension state checked `2026-10-06`;
  the linked page carries a `2018-2020` copyright but no exact ratification day.
- `citations`: The ratified extension defines a finite set of premade holistic
  material variants and primitive mappings optimized for low-latency runtime
  switching
  ([`KHR_materials_variants`](https://github.com/KhronosGroup/glTF/blob/main/extensions/2.0/Khronos/KHR_materials_variants/README.md)).
- `confidence`: high.
- `relevance`: Cogniform already renders a broad material subset, so variants
  turn that depth into a practical product-configuration feature.
- `direction_alignment`: Strong when variant names are bounded data, selection
  enters the canonical scene through the existing patch gateway, and unsupported
  mapped materials fail before mutation.
- `opportunity_or_risk`: Compact product choices and A/B renders versus
  variant-name ambiguity, material residency spikes, lazy-resource behavior,
  and a combinatorial validation surface.
- `engineering_cost`: medium-high; it needs F01 multi-primitive identity plus
  importer, asset lifecycle, scene component, extraction, renderer, and replay
  changes.
- `disposition`: adopt.
- `roadmap_delta`: Depend on F01. Split read-only variant discovery from
  revisioned selection; configuration rules, SKU logic, and variant authoring
  remain application concerns.
- `verification_needed`: Duplicate/empty name rejection, mapping precedence,
  unsupported-material atomicity, residency accounting, eviction/rehydration,
  replay, and byte-identical default-versus-selected renders.
- Rendered example: one shoe asset switches among authored red, black, and
  orange variants while camera, geometry, stable IDs, and scene revision remain
  auditable.
- Use cases: commerce configurators, design alternatives, finish selection,
  accessibility themes, and agent-generated comparison sheets.

### F04. Revisioned node visibility and inspection layers

- `finding`: Add a visibility component that hides a hierarchy from rendering
  without deleting its stable identity, and retain bounded
  `KHR_node_visibility` state on imported scene nodes after F01.
- `source_event_date`: `2025` copyright and official ratified-extension state
  checked `2026-10-06`; the linked page does not state an exact ratification
  day.
- `citations`: `KHR_node_visibility` defines inherited visibility over node
  hierarchies and requires hidden nodes' visual features, including meshes and
  lights, to be omitted from rendering
  ([Khronos extension](https://github.com/KhronosGroup/glTF/blob/main/extensions/2.0/Khronos/KHR_node_visibility/README.md)).
- `confidence`: high.
- `relevance`: Agents currently must delete entities or change unrelated
  material/geometry state to isolate an assembly layer in an observation.
- `direction_alignment`: Strong if visibility is canonical, revisioned,
  hierarchy-aware, and separate from ownership, existence, and selectability.
- `opportunity_or_risk`: Non-destructive inspection and render layers versus
  ambiguity in extraction, visibility observations, light suppression, and
  hidden-parent inheritance.
- `engineering_cost`: medium-high; protocol, world hierarchy, logical hash,
  extraction, renderer filtering, compiler, query, replay, and import change.
- `disposition`: adopt.
- `roadmap_delta`: Specify authorable visibility independently; imported node
  visibility follows F01. Layer/group convenience procedures may compose
  ordinary patches later rather than entering the core component.
- `verification_needed`: Parent/child inheritance, hidden light/camera policy,
  entity-ID and visibility output, query discoverability, replay/hash, imported
  extension validation, and hide/show round-trip renders.
- Rendered example: hide a car body hierarchy to expose the unchanged engine,
  suspension, and wiring underneath, then restore the identical full render.
- Use cases: exploded inspection, architectural layers, occlusion debugging,
  assembly instructions, and agents isolating relevant scene regions.

### F05. Stable-ID GPU instancing

- `finding`: Render bounded repeated instances of one mesh through shared GPU
  resources and fewer draws while retaining a stable selectable identity and
  entity-ID output for every instance; later admit `EXT_mesh_gpu_instancing`.
- `source_event_date`: Official ratified-extension state checked `2026-10-06`;
  the linked page does not state an exact ratification day.
- `citations`: `EXT_mesh_gpu_instancing` carries per-instance translation,
  rotation, and scale specifically to render many copies of one mesh with few
  draw calls, while documenting culling and per-instance-feature tradeoffs
  ([Khronos extension](https://github.com/KhronosGroup/glTF/blob/main/extensions/2.0/Vendor/EXT_mesh_gpu_instancing/README.md)).
- `confidence`: high for ecosystem and performance value; medium for preserving
  Cogniform's per-entity observation semantics efficiently.
- `relevance`: Repeated shelves, seats, trees, fasteners, or markers currently
  scale as independent draws even when geometry and material are identical.
- `direction_alignment`: Conditional on a stable CPU identity map, exact
  aggregate instance limits, deterministic grouping, explicit fallbacks, and
  unchanged public semantics.
- `opportunity_or_risk`: Larger scenes and fewer submissions versus batch
  rebuild cost, poor culling, adapter-limit variance, instance-ID packing, and
  pressure to leak backend grouping into public contracts.
- `engineering_cost`: high.
- `disposition`: monitor.
- `roadmap_delta`: First measure repeated-draw extraction and GPU baselines.
  If justified, add internal same-mesh batching before F01-dependent extension
  import; do not make a performance hypothesis a compatibility requirement.
- `verification_needed`: Reference hardware and fixture, deterministic batch
  keys, add/remove/update extraction, per-instance ID and normal output,
  culling behavior, adapter preflight, limits, eviction, and fallback parity.
- Rendered example: render thousands of warehouse pallet positions in a small
  draw set while an entity-ID pixel still identifies one exact pallet.
- Use cases: warehouses, forests, seating plans, manufacturing fasteners,
  point markers, and large synthetic scenes.

### F06. Explicit-time transform and property animation

- `finding`: Evaluate bounded glTF animation clips at a caller-supplied rational
  time to produce a deterministic render snapshot, initially covering node
  transforms and a narrow `KHR_animation_pointer` property allowlist without a
  wall-clock game loop.
- `source_event_date`: Core glTF registry build `2026-09-18`; the ratified
  animation-pointer page carries a `2024` copyright.
- `citations`: Core glTF defines animation channels and samplers, while
  `KHR_animation_pointer` extends targets to mutable properties such as
  material factors and camera field of view
  ([`KHR_animation_pointer`](https://github.com/KhronosGroup/glTF/blob/main/extensions/2.0/Khronos/KHR_animation_pointer/README.md)).
- `confidence`: high for explicit-time snapshots; medium for preserving logical
  authority across animated imported state.
- `relevance`: Agents need to inspect authored mechanisms and appearance over
  time, but continuous simulation would conflict with caller-driven,
  replayable execution.
- `direction_alignment`: Strong only as fixed-input evaluation with bounded
  clips, channels, keys, interpolation work, and a documented separation
  between authoritative scene state and transient render evaluation.
- `opportunity_or_risk`: Animated assembly and material inspection versus time
  ambiguity, floating-point interpolation variance, huge key streams, pointer
  authority, and accidental mutation outside the patch log.
- `engineering_cost`: high and dependent on F01.
- `disposition`: adopt.
- `roadmap_delta`: Start with read-only transform snapshots at exact requested
  times. Add a small property-pointer allowlist later. Skeletal animation,
  morph targets, free-running clocks, audio, physics, and animation-authored
  world mutation remain out of scope.
- `verification_needed`: Time canonicalization, interpolation vectors, channel
  and key limits, invalid-pointer rejection, unchanged source asset, repeated-
  time image equality, observation causality, and deterministic clip fixtures.
- Rendered example: request eight exact times across a robot-arm motion and
  receive eight revision-bound images without advancing any global clock.
- Use cases: mechanism review, animation QA, temporal synthetic data, authored
  camera moves, material transitions, and agent selection of key poses.

### F07. Revision-bound spatial queries and ray picking

- `finding`: Add bounded raycast and region-query results over stable scene
  entities, tied to an exact revision and optional camera pixel, without adding
  rigid-body physics or exposing acceleration-structure handles.
- `source_event_date`: `2026` copyright and official ratified-extension state
  checked `2026-10-06`; exact ratification day is not stated.
- `citations`: `KHR_node_selectability` standardizes selection rays, first-hit
  selectable objects, hierarchy propagation, and a world-space intersection
  point
  ([Khronos extension](https://github.com/KhronosGroup/glTF/blob/main/extensions/2.0/Khronos/KHR_node_selectability/README.md)).
- `confidence`: high for product fit; medium for the initial acceleration and
  imported-node semantic boundary.
- `relevance`: Agents can inspect entity-ID pixels today but cannot ask a
  structured question such as "what is under this pixel?" or "which entities
  intersect this bounded volume?".
- `direction_alignment`: Very strong when results use stable IDs, exact
  revision/camera causality, bounded hit counts, deterministic ordering, and
  caller-owned query scheduling.
- `opportunity_or_risk`: A direct perception-to-action loop without image
  parsing; risks include stale geometry, floating-point tie behavior, hidden
  unbounded acceleration work, and confusion with collision/physics authority.
- `engineering_cost`: high.
- `disposition`: adopt.
- `roadmap_delta`: Define a transport-neutral CPU query contract first. A
  bounded acceleration structure follows measured need; imported
  `KHR_node_selectability` semantics depend on F01 and remain a later slice.
- `verification_needed`: Independent triangle/AABB vectors, exact-revision
  rejection, stable equal-distance tie-breaking, hierarchy/visibility policy,
  bounded hit and work limits, replay consistency, and pixel-to-ray camera
  fixtures.
- Rendered example: an agent names a pixel on a rendered table leg, receives
  the exact entity, hit point, normal, and distance, then submits an ordinary
  material patch to that stable entity.
- Use cases: object selection, visual grounding, placement checks, inspection
  probes, measurement tools, and embodied-agent interaction without a physics
  engine.

### F08. Bounded asset provenance and semantic metadata

- `finding`: Retain a safe, query-only subset of `KHR_xmp_json_ld` metadata for
  assets and imported scene objects, with strict byte/depth/count limits and no
  network resolution of contexts, identifiers, or URLs.
- `source_event_date`: The ratified extension carries a `2018-2021` copyright;
  official state checked `2026-10-06`.
- `citations`: `KHR_xmp_json_ld` carries attribution, licensing, creation date,
  titles, subjects, and related metadata for assets, scenes, nodes, meshes,
  materials, images, and animations using a restricted JSON-LD representation
  ([Khronos extension](https://github.com/KhronosGroup/glTF/blob/main/extensions/2.0/Khronos/KHR_xmp_json_ld/README.md)).
- `confidence`: high for provenance value; medium for the useful initial
  allowlist and stable query representation.
- `relevance`: Content hashes prove byte identity but do not tell an agent what
  an asset represents, who created it, or how it may be used.
- `direction_alignment`: Strong when metadata remains untrusted inert data,
  canonical query output, and never becomes instructions, code, automatic
  authorization, network access, or implicit scene mutation.
- `opportunity_or_risk`: Better attribution, discovery, and semantic grounding
  versus prompt injection in text fields, privacy leakage, namespace bombs,
  external-reference fetching, and false trust in self-asserted metadata.
- `engineering_cost`: medium-high and dependent on F01 for node-level metadata.
- `disposition`: adopt.
- `roadmap_delta`: Begin with bounded offline inspection of a conservative
  provenance allowlist. Rendering and mutation must ignore metadata; richer
  application vocabularies require separate review.
- `verification_needed`: No-network proof, recursive and aggregate bounds,
  canonical namespace/value output, redaction policy, malicious-text fixtures,
  metadata precedence, source-byte/hash invariance, and query authorization.
- Rendered example: an agent renders a machine part, then obtains its title,
  creator, license, and source identifier alongside the stable selected node.
- Use cases: asset catalogs, license compliance, provenance audits, semantic
  search, multilingual labels, and grounding agent reports in source metadata.

### F09. Deterministic auto-framing and turntable capture

- `finding`: Add pure built-in procedures that compute a camera framing patch
  from bounded world-space entity bounds and generate a fixed list of orbit
  viewpoints for use with F02, without adding an editor or hidden camera
  mutation.
- `source_event_date`: `2026-09-18` for the current glTF 2.0 registry build.
- `citations`: glTF defines portable perspective and orthographic projection
  conventions and accessor bounds that can ground predictable asset framing
  ([glTF 2.0 specification](https://registry.khronos.org/glTF/specs/2.0/glTF-2.0.html)).
- `confidence`: high.
- `relevance`: The renderer can produce excellent diagnostic images, but every
  client must currently calculate camera placement and coverage itself.
- `direction_alignment`: Very strong because a pure procedure can return an
  ordinary reviewable patch, while capture remains an explicit observation
  request with existing causality.
- `opportunity_or_risk`: Immediately useful graphics with less client math;
  risks are degenerate/huge bounds, inconsistent padding, camera-up
  singularities, and mistaking a convenience procedure for editor authority.
- `engineering_cost`: medium; world/asset bounds, pure procedures, compiler
  vocabulary, examples, and observation composition change.
- `disposition`: adopt.
- `roadmap_delta`: Deliver focus-one-entity framing first, then a fixed-count
  turntable procedure after F02. Backgrounds, lighting rigs, arbitrary paths,
  video, and filesystem writes stay separate.
- `verification_needed`: Independent bounds/frustum vectors, degenerate and
  extreme inputs, stable orbit order, no I/O/time/entropy authority, ordinary
  patch admission/replay, and golden contact-sheet examples.
- Rendered example: select a chair entity and produce a centered eight-angle
  contact sheet with identical padding and scale on every run.
- Use cases: product catalogs, pull-request visual review, asset QA, dataset
  generation, issue reports, and fast human inspection of agent-created scenes.

### F10. Sandboxed glTF interactivity preview

- `finding`: Monitor a future opt-in evaluator for a tightly allowlisted,
  deterministic subset of `KHR_interactivity`, executed at caller-supplied
  fixed steps inside one asset-local state sandbox with no direct authority over
  the Cogniform world, filesystem, network, process, or clock.
- `source_event_date`: The Khronos registry lists the extension as ratified and
  was checked `2026-10-06`; official sample assets carry a `2025` copyright.
- `citations`: Khronos describes `KHR_interactivity` as graph-based interactive
  3D behavior and publishes self-checking conformance assets, including
  pointer and selection cases
  ([test-asset repository](https://github.com/KhronosGroup/glTF-Test-Assets-Interactivity),
  [ratified extension registry](https://github.com/KhronosGroup/glTF/blob/main/extensions/README.md)).
- `confidence`: high that the ecosystem feature is relevant; low that a useful
  subset can yet satisfy Cogniform's replay, authority, and bounded-execution
  requirements.
- `relevance`: Interactive assets are becoming portable runtime documents, but
  Cogniform currently treats imported assets as immutable visual resources.
- `direction_alignment`: Conditional and unresolved. Fixed-step pure evaluation
  fits; an asset graph that mutates authoritative world state or gains ambient
  capabilities conflicts with existing architecture.
- `opportunity_or_risk`: Portable buttons, mechanisms, and guided demos versus
  a new interpreter, denial-of-service loops, hidden mutable state, event-order
  nondeterminism, cross-asset communication, and confused-deputy authority.
- `engineering_cost`: very high; it depends on F01 and likely F04, F06, and F07.
- `disposition`: monitor.
- `roadmap_delta`: Do not schedule implementation. First author a threat model
  and ADR comparing reject, read-only inspection, pure bounded evaluation, and
  a Wasmtime-adjacent plugin boundary. Cross-document messaging and host calls
  remain excluded from any initial proposal.
- `verification_needed`: Normative subset selection, instruction/event/state
  budgets, fixed-step semantics, complete capability denial, deterministic
  conformance fixtures, cancellation, malformed graphs, replay model, and
  proof that world mutation still uses ordinary patches.
- Rendered example: at explicit step values, a sandboxed asset-local button
  opens a product lid and changes an indicator color without mutating the
  authoritative scene or accessing host services.
- Use cases: interactive manuals, product demos, training content, portable
  mechanism previews, and agent testing of authored interaction sequences.

## Overall recommendation

Adopt the horizon as a dependency graph, not a batch:

1. Establish F01 scene hierarchy and identity first.
2. Deliver F02, F04, F07, and F09 as independent agent-facing capabilities;
   make each imported-node portion depend on F01.
3. Add F03 and F06 only after multi-primitive identity and transient imported
   state boundaries are explicit.
4. Admit F08 as inert query-only evidence after its security model is accepted.
5. Keep F05 behind measured performance evidence and F10 behind a dedicated
   threat model and execution-authority ADR.

This order grows visible capability while preserving Cogniform's distinctive
value: deterministic, revision-linked graphics that an unattended client can
observe, understand, and manipulate. It does not prioritize a general editor,
physics, audio, remote deployment, or autonomous feature execution.

## Evidence gaps

- No maintainer/user interviews or usage telemetry rank these candidates by
  demand; the ordering is architectural and standards-based.
- No prototype establishes aggregate GPU/readback cost for F02 or batching
  benefit for F05 on versioned reference hardware.
- Imported scene identity and the relationship between asset nodes and mutable
  world entities need a consequential ADR before F01 implementation.
- Animation evaluation state, material-variant selection, and visibility need
  explicit logical-hash and replay decisions before implementation.
- Metadata trust/redaction and interactivity execution authority need dedicated
  security review; neither standard conformance nor popularity is sufficient.

Review this brief when one of those evidence gaps closes, a relevant standard
changes, or the maintainer selects a proposal for implementation. A review may
reorder, split, monitor, or reject a proposal; it must not silently authorize
code.
