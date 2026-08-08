# Validation and compatibility baseline

Status: controlled CF009 evidence collected on 2026-08-02 and CF011 normal,
CF012 restoration, CF013 recovery-envelope, CF014 historical-fork, plus CF015
service-asset rehydration and CF016 service-procedure composition evidence
collected on 2026-08-04, plus CF017 quiescent live-revert evidence collected on
2026-08-05, CF018 immutable recovery-file evidence, and CF019 immutable
exact-hash asset-source-file evidence collected on 2026-08-05.
CF020 optional vertex-normal decode, accounting, fallback, and controlled GPU
evidence was collected on the same validated profile on 2026-08-05.
CF021 fixed plane layout, selection, fallback, and controlled GPU evidence was
collected on that profile on 2026-08-05.
CF022 fixed sphere topology, selection, and controlled curved-surface evidence
was collected on that profile on 2026-08-05.
CF023 fixed-capacity directional diffuse lighting and exact unlit-compatibility
evidence was collected on that profile on 2026-08-05.
CF024 fixed-capacity point diffuse lighting, inverse-square attenuation, and
mixed-light compatibility evidence was collected on that profile on
2026-08-05.
CF025 outward cuboid topology, exterior-normal, and corrected canonical
Point-light evidence was collected on that profile on 2026-08-06.
CF026 bounded direct metallic-roughness response, exact unlit compatibility,
and updated canonical Point-light evidence was collected on that profile on
2026-08-06.
CF027 imported GLB material retention, override precedence, and controlled
direct-light evidence was collected on that profile on 2026-08-06.
CF028 primary-coordinate retention, exact 32-byte layout, and controlled
whole-frame equivalence evidence was collected on that profile on 2026-08-06.
CF029 bounded embedded PNG decode, shared texture residency, sampled
base-color-factor/override behavior, and exact-hash textured rehydration
evidence was collected on that profile on 2026-08-06.
CF030 exact content-hash asset eviction, submitted-frame preservation, and
explicit rehydration evidence was collected on that profile on 2026-08-07.
CF031 deterministic monotonic pending-work age and controlled
command/observation/import/upload lifecycle evidence was collected on that
profile on 2026-08-07.
CF032 CPU-only recovery-file inspection, fixed-profile semantic preflight,
path redaction, and read-only CLI evidence was collected on 2026-08-07.
CF033 deterministic schema-version-one recovery JSON, unchanged human output,
pre-output validation, and failure redaction evidence was collected on
2026-08-07.
CF034 fixed-layout schema-version-one controlled-measurement JSON, unchanged
human report structure, pre-output preparation, exact argument rejection, and
informational-only evidence was collected on 2026-08-08.
CF035 fixed-layout schema-version-one canonical-scenario JSON, unchanged human
bytes, pre-GPU argument rejection, complete-before-output behavior, and
controlled cross-mode GPU evidence was collected on 2026-08-08.
CF036 CPU-only immutable asset-source inspection, exact lowercase-hash
argument handling, file immutability, bounded complete identity validation,
and path/payload-redacted failure evidence was collected on 2026-08-08.
CF037 fixed-layout schema-version-one asset-source inspection JSON, unchanged
human bytes, complete-before-output serialization, option-like path behavior,
and failure redaction evidence was collected on 2026-08-08.
CF038 bounded observation-payload framing, canonical all-kind layouts,
metadata binding, corruption rejection, and independent resource-limit
evidence was collected on the CPU profile on 2026-08-08.
CF039 fixed-header local stream framing, header-first independent bounds,
short/interrupted I/O, canonical observation composition, corruption rejection,
and payload-redacted failure evidence was collected on the CPU profile on
2026-08-08.
CF040 canonical direction-specific local-session messages, effective pre-decode
bounds, exact-revision observation admission, and endpoint/executor separation
evidence was collected on the CPU profile on 2026-08-08.
This document names
what was reproduced and what remains unsupported; it is not a promise for
untested hardware.

## Compatibility profile

| Environment | Evidence | Classification |
|---|---|---|
| Windows 11 Pro 10.0.26200, x86_64 | Full release-mode engine, gateway, observation, replay, GLB render, four-buffer readback pressure, and canonical scenario tests passed | Validated local source profile |
| NVIDIA GeForce RTX 5070, Vulkan, discrete GPU, WebGPU-compliant downlevel report | Exact entity ID, exact unlit and tolerant directional/point direct-material color and depth, distinct scene/imported/overridden metallic-roughness response, bounded sRGB base-color texture orientation/factor/override and shared-residency response, content-hash eviction with submitted-readback safety and exact reupload, outward cuboid and positive-Z plane quantized unit normals, sphere curved-depth/radial-normal output, position-only GLB winding, imported-normal inverse-transpose, and normal-causality probes passed at 64x64 | Validated adapter entry, not a vendor minimum |
| `ubuntu-latest` x86_64 standard GitHub runner | Offline format, Clippy, workspace tests, public-tree safeguards, and rustdoc pass in the single PR job | CPU build/test evidence only; no GPU runtime claim |
| Windows DX12 | Backend is compiled, but CF009 did not force and reproduce this adapter path | Not release-supported yet |
| Linux Vulkan | Code and unit tests compile on the standard runner; no controlled GPU result is recorded | Not release-supported yet |
| Software/fallback adapter | Selection and unavailable-backend errors are typed; no full scenario result is recorded | Not release-supported yet |
| macOS/Metal, mobile, browser/WebGPU | Backend features and controlled results are absent from this workspace profile | Unsupported |

The renderer is capability based. An adapter must satisfy the configured target
dimensions, buffer bounds, attachment count, and render/copy usage for RGBA8
color, Depth32Float depth, R32Uint identity, and RGBA8 signed normal targets
before device creation, plus copy-destination, sampled binding, and filterable
sampling for sRGB RGBA8 asset textures. The normal path requires three color
attachments and twelve color-attachment bytes per sample.
The validated GPU above is evidence that one adapter meets the contract; it does
not impose a specific GPU model or driver version on future entries.

The pinned compiler used for this baseline was:

```text
rustc 1.97.1 (8bab26f4f 2026-07-14)
host: x86_64-pc-windows-msvc
LLVM 22.1.6
cargo 1.97.1 (c980f4866 2026-06-30)
```

## Controlled GPU commands

The following commands ran in the optimized profile with checked-in dependency
sources and no visible window or network service:

```text
cargo test --release -p cogniform-renderer --tests --locked --offline -- --ignored
cargo test --release -p cogniform-engine --tests --locked --offline -- --ignored
cargo run --release -p cogniform-cli --locked --offline -- scenario
```

The renderer suite passed the built-in cube, extracted plane and sphere,
front/back directional response, near/far/back-facing point response,
dielectric/metallic/roughness response and exact unlit compatibility,
bounded four-buffer readback pressure, renderer-drop retirement, position-only
GLB winding fixture, imported numeric material response/override, and
imported-normal fixture under non-uniform scale. The engine
suite passed gateway/idempotency, normal-aware revision causality, complete
service restoration, and the canonical scenario. The scenario selected Vulkan,
committed revision 2,
reported frames 1-3, found the table at the center color/entity-ID pixels,
reported 72 visible table pixels, and replayed two entries to logical hash
`db23b22d98da433d6050c0cd863f3a736832c7bae2ca674cdbee3dae8ed25106`.
The corrected outward table face and direct material response produced center
color `#371e0bff` (`[55, 30, 11, 255]`) on this
profile under the existing two-unit-per-channel tolerance.
Pixel coverage is visual evidence for this adapter, not a cross-GPU exact value.

## Controlled canonical-scenario JSON command

CF035 retained the existing human scenario command and added an explicit
machine-readable mode. The release-mode CLI regression runs both modes as
separate complete scenarios on the controlled adapter and compares their
adapter, revision, query, identity, pixel, logical-hash, and replay evidence:

```text
cargo test --release -p cogniform-cli --test scenario_output --locked --offline -- --ignored --exact human_and_json_modes_prove_the_same_canonical_scenario
cargo run --release -p cogniform-cli --locked --offline -- scenario --json
```

Both commands passed on 2026-08-08. The JSON command selected the existing
NVIDIA GeForce RTX 5070 Vulkan discrete-GPU profile and reported WebGPU
compliance, revision 2, four queried entities, frames 1-3, center table identity
and `#371e0bff`, 72 visible table pixels, two replay entries, and matching
live/replayed logical hash
`db23b22d98da433d6050c0cd863f3a736832c7bae2ca674cdbee3dae8ed25106`.
Schema version one is one compact line-feed-terminated object with fixed
top-level and adapter field order and typed integer counters. Invalid arguments
are rejected before adapter selection with empty stdout. Report construction
and serialization finish before stdout is written.

This records no new adapter, platform, scenario, tolerance, performance, or
release support claim. Adapter identity and run evidence can fingerprint or
correlate the host and remain local opt-in output with no upload, exporter, or
background collection.

## Controlled pending-work age commands

CF031 ran the focused CPU contracts, every existing release-mode renderer and
engine controlled test, and the canonical release scenario:

```text
cargo test -p cogniform-assets -p cogniform-renderer -p cogniform-engine --all-features --locked --offline
cargo test --release -p cogniform-renderer --tests --locked --offline -- --ignored
cargo test --release -p cogniform-engine --tests --locked --offline -- --ignored
cargo run --release -p cogniform-cli --locked --offline -- scenario
```

Deterministic injected-time unit tests prove empty status, exact microseconds,
`u64` saturation, duplicate retention, `LatestWins` reset, drop/capacity
rejection neutrality, FIFO-preserving process/eviction removal, and exact
observation permit release. The 14 renderer and 10 engine controlled tests
then prove pending/resident upload behavior, command queue drain and replay,
observation reservation through delivery, service asset import/upload,
restoration, historical fork, quiescent revert, and canonical empty status on
the declared Vulkan adapter. Status-state comparisons retain exact equality
for every stable field while requiring nondecreasing age for an unchanged
pending lifecycle. The canonical scenario remains revision 2, three ordered
frames, center `#371e0bff`, 72 table pixels, two replay entries, and matching
live/replayed logical hash
`db23b22d98da433d6050c0cd863f3a736832c7bae2ca674cdbee3dae8ed25106`.
No timing threshold, exporter, logging, transport, deployment, or additional
supported platform is introduced.

## Controlled recovery-inspection commands

CF032 ran the focused engine, storage, and CLI contracts without a GPU adapter:

```text
cargo clippy -p cogniform-engine -p cogniform-storage -p cogniform-cli --all-targets --all-features --locked --offline -- -D warnings
cargo test -p cogniform-engine -p cogniform-storage -p cogniform-cli --all-features --locked --offline
```

Engine tests prove valid empty and nonempty reconstruction, exact aggregate
revision/frame/logical/replay-hash evidence, complete semantic replay rejection,
and a frame frontier behind accepted evidence. Storage tests retain regular-
file, metadata/allocation, growth, corruption, truncation, extension, and
oversize rejection. CLI black-box tests prove exact one-path/help behavior,
valid fixed-profile output, nonzero storage and semantic failures, unchanged
file bytes, and path/payload redaction. These ordinary tests invoke the
synchronous CPU inspection function and never construct `HeadlessRenderer` or
select an adapter; no additional GPU compatibility claim is made.

Inspection uses `default-local-64x64`. It does not validate asset residency,
writer authenticity, freshness, authorization, automatic startup, or later GPU
service initialization, and it adds no performance threshold or supported
platform.

CF033 repeated the focused commands above after adding the CLI-only JSON view.
Black-box tests pin the exact compact object, field order and JSON types,
single-line-feed framing, lowercase hashes, fixed profile, reserved filename
escape, unchanged human bytes, file immutability, empty failure stdout, and
path/payload redaction. The lockfile adds only direct CLI edges to the existing
exact-pinned vendored `serde` and `serde_json` packages; no package, version,
vendor source, engine, renderer, or recovery-file format changed. Because the
slice is entirely at the CPU CLI presentation boundary, no GPU test or new
adapter claim is required.

## Controlled asset-source inspection commands

CF036 ran the focused CLI and storage contracts without a GPU adapter:

```text
cargo check -p cogniform-cli --locked --offline
cargo test -p cogniform-cli --bin cogniform-cli --locked --offline
cargo test -p cogniform-cli --test asset_inspection --locked --offline
cargo test -p cogniform-storage --test asset_file --locked --offline
```

CLI unit and black-box tests pin strict public `ContentHash` parsing, exact
one-hash/one-path argument handling, byte-for-byte success output, ordinary
option-like filenames, unchanged input files, empty failure stdout, and
path/payload-redacted hash-mismatch and filesystem diagnostics. The existing
storage contract retains the default 16 MiB bound, final regular-file check,
snapshotted allocation, fixed-buffer read, growth probe, and complete SHA-256
comparison before bytes return. The CLI drops those bytes after recording the
checked length and never constructs a decoder, service, or `HeadlessRenderer`.
No GPU test or additional adapter claim is required.

CF037 repeated the focused commands above after adding the CLI-only JSON view.
Black-box tests pin the exact compact object, field order and JSON types,
single-line-feed framing, lowercase hash, positional `--json` path behavior,
unchanged human bytes, file immutability, empty failure stdout, and path/payload
redaction. Report construction and serialization finish before the single
stdout write. Existing CLI Serde dependencies are reused; no manifest,
lockfile, package, version, vendor source, engine, renderer, protocol, storage,
or asset format changed.

Passing either output mode proves only that one trusted local hash-to-path
mapping contains the expected bytes under the default source limit. It does
not prove format validity, importer acceptance, renderability, authenticity,
freshness,
authorization, recovery association, or GPU readiness, and it schedules no
import, upload, or rehydration. Schema version one remains CLI-private and
establishes no general diagnostics contract.

## Controlled observation-payload envelope commands

CF038 ran the dependency-neutral payload codec and its engine compatibility
edge without a GPU adapter or transport:

```text
cargo fmt --all --check
cargo clippy -p cogniform-observation -p cogniform-engine --all-targets --all-features --locked --offline -- -D warnings
cargo test -p cogniform-observation -p cogniform-engine --all-features --locked --offline
```

The 11 codec integration tests pin every payload kind's round trip, fixed
header and big-endian item layouts, one exact full-envelope fixture, canonical
metadata, presence/float/identity/visibility rules, metadata substitution, every
truncated prefix, trailing bytes, every single-byte mutation, and independent
envelope, visibility-entry, and runtime-pixel limits. The 25 engine unit tests
retain the existing import path through a public re-export and prove explicit
encoding of one completed bound payload. Existing adapter-backed engine tests
remain ignored under their documented hardware gate; no renderer behavior or
GPU support claim changed.

The decoder receives an already-buffered borrowed slice. These tests prove its
in-memory bound and allocate-after-integrity behavior, not network framing,
authentication, confidentiality, session rate limits, shared-memory safety,
or automatic delivery. A future stream adapter must enforce its own declared
length cap before buffering.

## Controlled local stream-framing commands

CF039 ran the dependency-neutral caller-owned stream adapter without creating
an endpoint, listener, session, service loop, renderer, or GPU adapter:

```text
cargo fmt --all --check
cargo clippy -p cogniform-local-transport --all-targets --all-features --locked --offline -- -D warnings
cargo test -p cogniform-local-transport --all-features --locked --offline
```

The nine integration tests pin one exact full control-frame fixture, both
version-one frame kinds, canonical observation composition, back-to-back
frames, clean EOF, short and interrupted reads/writes, every truncated prefix,
trailing bytes, malformed headers, every single-byte mutation, nested metadata
substitution and corruption, validation before writes, header-limit rejection
before body reads, and stable payload-redacted I/O categories. Complete,
control, and bulk limits are independent and are enforced from the fixed header
before either body section is allocated.

The adapter reads from and writes to caller-owned `std::io` values. These tests
do not prove endpoint identity, authentication, authorization, confidentiality,
freshness, replay protection, session rate limits, timeout/cancellation policy,
shared-memory safety, automatic delivery, or writer atomicity after an I/O
failure. The SHA-256 digests detect accidental corruption and substitution;
they do not authenticate a writer.

## Controlled local-session message commands

CF040 ran the in-memory message and frame-adaptation boundary without opening a
stream, creating an endpoint, executing a service, starting a scheduler, or
initializing a renderer:

```text
cargo fmt --all --check
cargo clippy -p cogniform-protocol -p cogniform-local-session -p cogniform-engine --all-targets --all-features --locked --offline -- -D warnings
cargo test -p cogniform-protocol -p cogniform-local-session -p cogniform-engine --all-features --locked --offline
```

The session integration tests pin one exact LF-terminated schema-version-one
fixture for every client/server variant, every variant's CF039 control-frame
round trip, outer-only correlation,
control-versus-observation frame separation, and direction, version, unknown,
noncanonical, nested, substitution, truncation, trailing, effective byte,
advertised-limit, core-value, and receipt-role rejection. The protocol fixture
pins the migrated exact-revision `ObservationRequest`. The controlled adapter
causality test requests a stale revision while observation capacity is full and
uses an invalid camera, proving the typed revision error precedes both capacity
and renderer work; default CPU runs compile but do not execute that ignored GPU
test.

These tests do not prove lifecycle sequencing, service error mapping, automatic
queue processing, stdin/stdout handling, partial-write recovery, peer identity,
authentication, authorization, confidentiality, freshness/replay protection,
rate/tenancy policy, timeout/cancellation, or remote safety. Those remain the
responsibility of separately reviewed executor and endpoint milestones.

## Controlled vertex-normal commands

The focused CF020 adapter and service checks passed in the optimized profile:

```text
cargo test --release -p cogniform-renderer --test asset_fixture --locked --offline -- --ignored
cargo test --release -p cogniform-renderer --test headless_reference --locked --offline -- --ignored --exact reference_cube_produces_exact_ids_and_tolerant_color_depth_normals
cargo test --release -p cogniform-engine --test service_assets --locked --offline -- --ignored --exact local_service_imports_renders_and_explicitly_rehydrates_one_glb_asset
```

The first command proves that the original position-only fixture still emits
its winding-derived normal and that an imported direction is transformed by
the model inverse-transpose under non-uniform scale before reaching the
quantized observation. The second protects the built-in cube's flat exterior-
normal output. CPU tests separately cover finite normalization, zero/non-finite/count/
range rejection, unsafe-proxy exclusion, exact 24-byte accounting, and
position/normal GPU interleaving. The service regression proves the unchanged
position-only import, observation, recovery, and explicit rehydration path.
This does not add another supported adapter.

## Controlled built-in-plane command

The focused CF021 adapter check passed in the optimized profile:

```text
cargo test --release -p cogniform-renderer --test headless_reference --locked --offline -- --ignored --exact extracted_plane_produces_color_depth_identity_and_plus_z_normal
```

It rendered the fixed centered XY plane through ordinary extracted-scene
submission and proved the requested RGBA8 color, finite foreground depth,
exact stable entity identity, positive-Z quantized normal, background identity,
and absent background normal. CPU tests separately prove two-triangle winding,
the exact 24-byte layout, all-axis primitive model scaling, cuboid/plane/asset
selection, resident-asset precedence, exact unavailable-asset plane fallback,
and the retained direct/fallback sphere error. This adds no supported adapter,
pipeline, observation format, or performance claim.

## Controlled built-in-sphere command

The focused CF022 adapter check passed in the optimized profile:

```text
cargo test --release -p cogniform-renderer --test headless_reference --locked --offline -- --ignored --exact extracted_sphere_produces_curved_depth_identity_and_radial_normals
```

It rendered the fixed centered sphere through ordinary extracted-scene
submission and proved the requested RGBA8 color, exact stable entity identity,
finite foreground depth, a deeper off-axis surface sample, smoothly changing
radial normals, background identity, and absent background normal. CPU tests
separately prove the exact 16-sector by 8-band topology, 224 outward triangles,
672 vertices, 16,128-byte payload, unit-diameter radius, radial normals,
all-axis bounding-diameter scaling, direct and unavailable-asset fallback
selection, and resident-asset precedence. This adds no supported adapter,
pipeline, observation format, or performance claim.

## Controlled directional-light command

The focused CF023 adapter check passed in the optimized profile:

```text
cargo test --release -p cogniform-renderer --test headless_reference --locked --offline -- --ignored --exact directional_light_modulates_front_and_back_facing_direct_color
```

It rendered one centered plane with a white half-intensity directional light.
An identity light produced half the material base RGB; rotating the light 180
degrees about Y produced black while alpha, exact stable identity, depth,
quantized world normal, and background remained unchanged. The complete
renderer conformance suite also preserved every prior no-directional-light
output. CPU tests separately prove stable-ID order, positive-Z normalization,
zero-intensity inactivity, the four-definition boundary, degenerate
active-direction rejection, and the exact zero-padded 304-byte directional
prefix. This
adds no supported adapter, pipeline, observation format, PBR claim, or
performance claim.

## Controlled point-light command

The focused CF024 adapter check passed in the optimized profile:

```text
cargo test --release -p cogniform-renderer --test headless_reference --locked --offline -- --ignored --exact point_light_applies_bounded_distance_and_facing_direct_shading
```

It rendered one centered positive-Z plane with a white half-intensity Point
source. Unit distance produced half the material base RGB; doubling the
distance produced one eighth of base RGB; moving the source behind the plane
produced black. Adding a half-intensity directional source to the far Point
frame produced their expected summed factor. Alpha, exact stable identity,
depth, quantized world normal, and background remained unchanged. A finite
maximum-f32 source position whose squared distance overflowed also produced
black without corrupting those auxiliary outputs. The complete
renderer conformance suite
also preserved every prior output, and the canonical engine scenario passed
with its existing Point definition and measured winding-normal response. CPU
tests separately prove stable-ID order, zero-intensity capacity accounting,
the independent four-definition boundary, active position f32 conversion,
and the exact zero-padded 448-byte uniform whose first 304 bytes preserve the
directional layout. This adds no supported adapter, pipeline, observation
format, range/radius, PBR claim, or performance claim.

## Controlled outward-cuboid commands

The focused CF025 adapter and canonical checks passed in the optimized profile:

```text
cargo test --release -p cogniform-renderer --test headless_reference --locked --offline -- --ignored --exact reference_cube_produces_exact_ids_and_tolerant_color_depth_normals
cargo test --release -p cogniform-engine --test canonical_mvp --locked --offline -- --ignored --exact canonical_scenario_preserves_revision_observation_and_replay_causality
```

The reference projection selected the cuboid's near negative-Z face and
reported an outward negative-Z unit normal while preserving unlit color,
depth, identity, and background. The canonical camera selected the table's
positive-Z exterior; its existing Point source produced `[175, 93, 33, 255]`
on this profile while revision 2, frames, stable table identity, 72 visible
pixels, two replay entries, and live/replayed hash equality remained intact.
CPU tests separately pin six faces, two non-degenerate triangles per face, 36
vertices, the 24-byte interleaved layout, exact 864-byte payload, coordinates
at plus or minus `0.5`, outward centroid/winding alignment, and the six exact
axis normals. Imported assets retain source winding. This adds no supported
adapter, culling/two-sided policy, pipeline, schema, or performance claim.

## Controlled direct-material commands

The focused CF026 adapter and canonical checks passed in the optimized profile:

```text
cargo test --release -p cogniform-renderer --test headless_reference --locked --offline -- --ignored --exact metallic_and_roughness_drive_distinct_bounded_direct_response
cargo test --release -p cogniform-renderer --test headless_reference --locked --offline -- --ignored
cargo test --release -p cogniform-renderer --test asset_fixture --locked --offline -- --ignored
cargo test --release -p cogniform-engine --tests --locked --offline -- --ignored
```

A centered positive-Z plane under the fixed half-intensity directional source
produced `[38, 22, 14, 255]` as a dielectric at roughness `0.5`,
`[129, 65, 32, 255]` as a metal at roughness `0.5`, and
`[12, 6, 3, 255]` as a metal at roughness `0.9`, each within the existing
two-unit-per-channel tolerance. Zero roughness exercised the distribution
floor and remained finite and bounded. Removing the source preserved exact unlit base
color `[204, 102, 51, 255]`. Depth, exact stable identity, quantized world
normal, alpha, and background were unchanged across those frames. CPU tests
pin finite camera/material preparation, the neutral missing-material values,
and the exact zero-padded 480-byte uniform with its complete 448-byte CF024
prefix. The full renderer and engine controlled suites passed, including both
imported-asset probes and the canonical center `[55, 30, 11, 255]`; this adds
no supported adapter, texture/IBL/shadow surface, schema, dependency, or
performance claim.

## Controlled imported-material command

The focused CF027 adapter check passed in the optimized profile:

```text
cargo test --release -p cogniform-renderer --test asset_fixture --locked --offline -- --ignored --exact imported_material_factors_drive_direct_light_and_scene_override
```

One explicitly materialed GLB retained base color `(0.8, 0.4, 0.2, 1.0)`,
metallic `1`, and roughness `0.5` through import, upload, and renderer
residency. With no active light it produced exact `[204, 102, 51, 255]`.
The fixed half-intensity directional source produced `[129, 65, 32, 255]`;
an explicit scene dielectric override at roughness `0.5` then produced
`[38, 22, 14, 255]`, each within the existing two-unit-per-channel tolerance.
Depth, exact stable identity, quantized world normal, alpha, and background
remained unchanged across frames; renderer revision advanced from 1 through 3
with the accepted world patches. CPU tests separately pin glTF factor defaults,
the existing material-free and proxy neutral defaults, unit-range rejection,
exact 24-byte vertex accounting, immutable upload metadata, and all-value override
precedence. This adds no supported adapter, GPU buffer, shader/pipeline,
texture surface, schema, dependency, or performance claim.

## Controlled primary-coordinate command

The focused CF028 adapter check passed in the optimized profile:

```text
cargo test --release -p cogniform-renderer --test asset_fixture --locked --offline -- --ignored --exact primary_texcoords_are_retained_without_changing_rendered_observations
```

One GLB retained three exact finite f32 `TEXCOORD_0` values, including values
outside the unit interval, through decode and the exact 32-byte upload layout.
Its frame matched the equivalent coordinate-free fixture at every pixel for
color, depth, stable identity, and world-space normal, including background.
CPU tests separately pin indexed expansion, exact zero defaults, validation of
an unused indexed source coordinate, non-finite/count/range rejection without
proxy, unsupported-encoding proxy behavior, renderer location 2, and the exact
1,152/192/21,504-byte built-in payloads. This adds no image decode, sampler,
texture, shader-sampling, schema, adapter, dependency, or performance claim.

## Controlled embedded base-color texture commands

The focused CF029 adapter and service-recovery checks passed in the optimized
profile:

```text
cargo test --release -p cogniform-renderer --test asset_fixture --locked --offline -- --ignored --exact embedded_base_color_texture_preserves_orientation_factor_override_and_residency
cargo test --release -p cogniform-engine --test service_assets --locked --offline -- --ignored --exact exact_hash_rehydration_restores_a_textured_asset_only_after_explicit_work
```

One source shared a 2x2 RGBA texture across two meshes. Before processing, the
renderer reserved one unique 16-byte texture; the first explicit upload created
it and the second reused it. Exact center probes pinned top-left then bottom-left
row orientation, sRGB-to-linear sampling, base-color RGB/alpha multiplication,
direct-light distinction, and full scene-material override. Depth, exact
identity, quantized normal, and background remained stable. The recovery check
proved a restored logical reference begins without CPU/GPU asset state and
resumes only after explicit exact-byte import and texture upload, with no
revision, logical-hash, or replay change. CPU tests separately pin RGB-to-RGBA
expansion, malformed/truncated PNG rejection, strict references and resource
shape, dimension/pixel/decoder/decoded/residency limits, unsupported-feature
proxy classification, exact byte accounting, and pre-allocation GPU texture
reservations. This adds no adapter, image-format, sampler, texture-role,
schema, performance, deployment, or release claim.

## Controlled explicit asset-eviction commands

The focused CF030 renderer and service checks passed in the optimized profile:

```text
cargo test --release -p cogniform-renderer --test asset_fixture --locked --offline -- --ignored --exact content_hash_eviction_cancels_partial_uploads_and_preserves_submitted_work
cargo test --release -p cogniform-engine --test service_assets --locked --offline -- --ignored --exact explicit_eviction_is_capacity_exact_and_logically_neutral_before_rehydration
```

A two-mesh textured asset was partially uploaded before a frame submission.
Eviction then released the remaining upload reservation, every resident mesh,
and the shared texture exactly once while the submitted readback remained
valid. The renderer's next draw used its authored cuboid fallback; the
no-fallback service draw returned `AssetUnavailable` without consuming a
frame. Exact reupload restored matching imported output. The service check
additionally pinned queued-source and decoded-CPU release, idempotent absent
eviction, unrelated FIFO preservation, and unchanged world reference,
revision, logical hash, replay, recovery evidence, and frame frontier. This is
explicit local
content-hash policy evidence, not per-mesh, LRU, reference-counted, background,
or automatic eviction, automatic rehydration, source-file deletion, device
recreation, deployment, or release evidence.

## Controlled service-restoration command

The following focused CF012 test passed in release mode on the validated
Windows/Vulkan profile:

```text
cargo test --release -p cogniform-engine --test service_restore --locked --offline -- --ignored --exact complete_recovery_restores_queries_observations_and_continuation
```

It captured state after revision 3, encoded and decoded one deterministic
bounded recovery envelope, rejected a frame marker behind replay evidence,
restored exact replay bytes and logical hash into a fresh service, started all
transient queues empty, reproduced an exact query, returned an old patch as an
idempotent replay, produced the next observation frame, appended a new revision
4 entry, and ended with matching live and replayed hashes. CPU unit tests also
rejected a one-byte mutation at every envelope position and covered typed
header, version, size, length, frame, and integrity failures. This is in-memory
restoration evidence, not authentication, encryption, filesystem crash
consistency, or automatic device-recreation evidence.

## Controlled historical-fork command

The following focused CF014 test passed in release mode on the validated
Windows/Vulkan profile:

```text
cargo test --release -p cogniform-engine --test service_restore --locked --offline -- --ignored --exact historical_recovery_fork_restores_exact_revision_and_continues
```

It retained a revision-2 point after the source advanced to revision 3,
confirmed the point used the source's current next frame identity, and proved
capture left source status, logical hash, and full replay bytes unchanged. A
newer revision returned the exact requested/latest typed error. After dropping
the source, the point restored revision 2 with exact prefix bytes, logical and
replayed hashes, query state, empty transient queues, and renderer revision;
the fork then produced the expected next observation frame and appended a new
revision 3 with matching live/replayed hash. CPU replay tests covered revision
zero, every retained revision, repeatable prefix encoding, full-stream prefix
relationships, and future-revision rejection. That CF014 case is
fresh-service branching evidence; CF017 live-revert evidence is recorded
below. Those cases do not by themselves prove persistence, branch management,
or rollback protection.

## Controlled service-asset command

The following focused CF015 test passed in release mode on the validated
Windows/Vulkan profile:

```text
cargo test --release -p cogniform-engine --test service_assets --locked --offline -- --ignored --exact local_service_imports_renders_and_explicitly_rehydrates_one_glb_asset
```

It proved hash-mismatch admission consumed no record or queue capacity; one
checked triangle GLB moved through explicit one-item CPU import and GPU upload;
and its exact stable entity identity rendered through `LocalService`. A fresh
restore retained the logical asset reference, replay bytes, revision, and hash
but began with empty CPU/GPU asset state. Observation then returned the exact
missing entity and mesh key. Reimporting the same bytes and uploading the mesh
restored the observation without a world mutation or replay change. This is
caller-supplied in-memory rehydration evidence, not filesystem/network
resolution, persistence, automatic eviction, automatic startup, or device
recreation.

## Controlled service-procedure command

The following focused CF016 test passed in release mode on the validated
Windows/Vulkan profile:

```text
cargo test --release -p cogniform-engine --test service_procedures --locked --offline -- --ignored --exact local_service_procedure_preserves_queue_query_replay_and_restore_idempotency
```

It rejected an over-budget request without queue or world mutation; admitted a
2x3 cuboid grid with six deterministic stable IDs; returned `AlreadyQueued` for
an exact queued repeat and `IdempotencyConflict` for changed output under the
same key; and applied one ordinary patch. Exact logical query, live/replayed
hash equality, and a single replay entry then matched. Same-service replay and
post-restoration resubmission both returned the retained receipt without
revision or replay growth. This is pure built-in synchronous preparation
evidence, not a plugin/Wasm host, user-code boundary, background scheduler, or
external procedure loader.

## Controlled in-place-revert command

The following focused CF017 test passed in release mode on the validated
Windows/Vulkan profile:

```text
cargo test --release -p cogniform-engine --test service_revert --locked --offline -- --ignored --exact local_service_revert_is_quiescent_atomic_and_branch_continuable
```

It proved current/future targets and pending commands, observations, imports,
or uploads reject without changing status, assets, logical hash, replay bytes,
or next frame identity. A revision-3 service with decoded and GPU-resident
assets then reverted to the exact revision-1 prefix through a restored
replacement. The receipt accounted for the two removed entries and cleared
gateway/CPU/GPU state; query, renderer revision, replay, and live/replayed hash
matched; the first observation used the source frame frontier; retained
idempotency added no event; and the removed revision-2 patch applied normally
on the new branch. This is quiescent local lifecycle evidence, not persistence,
automatic rollback, transient migration, asset preservation, authentication,
or device-loss recovery.

## Controlled recovery-file command

The following focused CF018 test passed in release mode on the validated
Windows/Vulkan profile:

```text
cargo test --release -p cogniform-storage --test recovery_file --locked --offline -- --ignored --exact persisted_recovery_restores_and_continues_exact_causality
```

It ran the canonical service to revision 2, encoded and created one new local
recovery file, synchronized it, dropped the source, loaded the exact complete
point, and restored matching world/renderer revisions, replay bytes, logical
and replayed hashes, query state, and source frame frontier. The restored
service returned an entity-ID observation at that frame and appended revision
3 with a valid replay chain and matching hashes.

CPU tests separately prove create-new non-overwrite, encode-before-I/O, no
implicit directory creation, regular-file and metadata-size checks, complete
digest rejection, path-redacted error/debug values, and injected write/sync
failure cleanup. This is explicit local-file evidence, not automatic
checkpoint/startup/rollback, mutable snapshot retention, encryption,
authentication, directory-entry crash consistency, actual disk-full, remote
storage, asset/transient persistence, or device-loss recovery.

## Controlled asset-source-file command

The following focused CF019 test passed in release mode on the validated
Windows/Vulkan profile:

```text
cargo test --release -p cogniform-storage --test asset_file --locked --offline -- --ignored --exact persisted_recovery_and_asset_sources_restore_renderable_state
```

It created separate immutable recovery and exact-hash GLB source files,
synchronized them, dropped the source service and in-memory bytes, and loaded
both within independent bounds. The restored service retained exact revision,
logical hash, replay bytes, query reference, and frame frontier while first
returning the expected `AssetUnavailable`. Explicit load, import, and upload
then restored the same triangle observation without revision, hash, or replay
change.

CPU tests separately prove source-size and hash rejection before I/O,
create-new non-overwrite, regular-file and metadata bounds, complete identity
validation, growth detection, path-redacted errors, and injected write/sync
cleanup. This is caller-mapped local-file evidence, not a bundle, content
discovery, asset catalog, automatic retention/eviction, automatic rehydration,
authentication, encryption, directory-entry crash consistency, remote storage,
or device-loss recovery.

## Controlled CPU performance fixture

The versioned fixture is `world-create-empty-v1` in `cogniform-cli
measure-world`. It prepares one validated patch containing 1,000 stable-ID
entity-create operations with no components. Each sample starts from a fresh
default `AuthoritativeWorld`. Timing excludes patch construction and world
construction, then measures:

1. complete decoded patch apply;
2. protocol validation plus world preflight, from receipt timing;
3. atomic commit, from receipt timing;
4. compact render extraction; and
5. canonical logical hash after extraction.

The optimized command is:

```text
cargo run --release -p cogniform-cli --locked --offline -- measure-world
```

Scripts may request the same completed measurement as compact versioned JSON:

```text
cargo run --release -p cogniform-cli --locked --offline -- measure-world --json
```

Schema version one emits exactly one line-feed-terminated object. It identifies
the `world-create-empty-v1` fixture, build profile, 1,000 operations, five
warmups, 30 measured samples, `nanoseconds` unit, and
`informational_only: true`. Its five distribution objects are `apply_total`,
`validate_and_preflight`, `atomic_commit`, `render_extraction`, and
`logical_hash`; each contains ordered integer `min`, `median`, `p95`, and `max`
values. JSON preparation is complete before stdout, and invalid arguments
leave stdout empty. The human labels, order, microsecond formatting, threshold
statement, and debug-profile warning remain unchanged.

CF034 ran the release JSON command as a smoke test but deliberately records no
new timing table: adding a presentation schema neither replaces the dated
baseline below nor creates a merge threshold. The report includes no hardware
identity or upload behavior, although local timing values can still reveal
host or process performance characteristics.

Two independent command invocations were recorded on 2026-08-02. Each used
five warmups followed by 30 measured samples in one process. Nearest-rank p95
and the upper middle sample for the median are reported. The machine had an AMD
Ryzen 7 7800X3D (8 cores/16 threads) and 33,462,239,232 bytes of physical
memory, running the Windows/toolchain profile above.

| Run | Span, microseconds | Min | Median | p95 | Max |
|---|---|---:|---:|---:|---:|
| A | Apply total | 1,031.000 | 1,079.800 | 1,422.700 | 1,486.400 |
| A | Validate and preflight | 704.000 | 749.000 | 995.000 | 1,158.000 |
| A | Atomic commit | 307.000 | 327.000 | 426.000 | 432.000 |
| A | Render extraction | 315.700 | 329.500 | 410.700 | 420.100 |
| A | Logical hash | 406.900 | 434.900 | 508.000 | 529.500 |
| B | Apply total | 1,044.100 | 1,198.000 | 2,128.300 | 2,166.200 |
| B | Validate and preflight | 736.000 | 881.000 | 1,500.000 | 1,527.000 |
| B | Atomic commit | 298.000 | 330.000 | 599.000 | 665.000 |
| B | Render extraction | 312.300 | 332.400 | 556.400 | 569.600 |
| B | Logical hash | 388.200 | 428.700 | 685.200 | 704.000 |

Both measured 1,000-operation apply p95 values are below the design's 8 ms
research target on this machine. That is an observation, not a general
threshold. The fixture uses empty entities, receipt subspans have microsecond
resolution, the
process was not CPU-pinned or isolated, power/thermal state was uncontrolled,
and the result excludes JSON decode, imagination compilation, GPU work,
observations, persistence, and transport. CI does not run this benchmark and no
merge gate is derived from it.

Future baselines append a dated table with the exact fixture version, commit,
profile, toolchain, OS, CPU, adapter when relevant, warmups, sample count, and
limitations. Existing numbers are never silently replaced or used to weaken a
threshold.
