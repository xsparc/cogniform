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
This document names
what was reproduced and what remains unsupported; it is not a promise for
untested hardware.

## Compatibility profile

| Environment | Evidence | Classification |
|---|---|---|
| Windows 11 Pro 10.0.26200, x86_64 | Full release-mode engine, gateway, observation, replay, GLB render, four-buffer readback pressure, and canonical scenario tests passed | Validated local source profile |
| NVIDIA GeForce RTX 5070, Vulkan, discrete GPU, WebGPU-compliant downlevel report | Exact entity ID, tolerant unlit and directional/point-diffuse color and depth, cuboid and plane quantized unit normals, sphere curved-depth/radial-normal output, position-only GLB winding, imported-normal inverse-transpose, and normal-causality probes passed at 64x64 | Validated adapter entry, not a vendor minimum |
| `ubuntu-latest` x86_64 standard GitHub runner | Offline format, Clippy, workspace tests, public-tree safeguards, and rustdoc pass in the single PR job | CPU build/test evidence only; no GPU runtime claim |
| Windows DX12 | Backend is compiled, but CF009 did not force and reproduce this adapter path | Not release-supported yet |
| Linux Vulkan | Code and unit tests compile on the standard runner; no controlled GPU result is recorded | Not release-supported yet |
| Software/fallback adapter | Selection and unavailable-backend errors are typed; no full scenario result is recorded | Not release-supported yet |
| macOS/Metal, mobile, browser/WebGPU | Backend features and controlled results are absent from this workspace profile | Unsupported |

The renderer is capability based. An adapter must satisfy the configured target
dimensions, buffer bounds, attachment count, and render/copy usage for RGBA8
color, Depth32Float depth, R32Uint identity, and RGBA8 signed normal targets
before device creation. The normal path requires three color attachments and
twelve color-attachment bytes per sample.
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
front/back directional diffuse response, near/far/back-facing point diffuse
response,
bounded four-buffer readback pressure, renderer-drop retirement, position-only
GLB winding fixture, and imported-normal fixture under non-uniform scale. The engine
suite passed gateway/idempotency, normal-aware revision causality, complete
service restoration, and the canonical scenario. The scenario selected Vulkan,
committed revision 2,
reported frames 1-3, found the table at the center color/entity-ID pixels,
reported 72 visible table pixels, and replayed two entries to logical hash
`db23b22d98da433d6050c0cd863f3a736832c7bae2ca674cdbee3dae8ed25106`.
Pixel coverage is visual evidence for this adapter, not a cross-GPU exact value.

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
quantized observation. The second protects the built-in cube's flat-normal
output. CPU tests separately cover finite normalization, zero/non-finite/count/
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
cargo test --release -p cogniform-renderer --test headless_reference --locked --offline -- --ignored --exact directional_light_modulates_front_and_back_facing_diffuse_color
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
cargo test --release -p cogniform-renderer --test headless_reference --locked --offline -- --ignored --exact point_light_applies_bounded_distance_and_facing_diffuse_shading
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
resolution, persistence, eviction, automatic startup, or device recreation.

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
discovery, asset catalog, retention/eviction, automatic rehydration,
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
