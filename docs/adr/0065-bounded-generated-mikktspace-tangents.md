# ADR 0065: Generate bounded default MikkTSpace tangents

- Status: Accepted
- Date: 2026-08-22
- Task: CF065

## Context

ADR 0055 required an explicit source `NORMAL`, `TANGENT`, and `TEXCOORD_0`
basis before a normal texture could render. Core glTF instead recommends
generating default MikkTSpace tangents when `TANGENT` is absent. When `NORMAL`
is absent, glTF requires flat normals and requires any supplied tangent to be
ignored for rendering.

The existing expanded triangle stream can generate per-corner tangents without
changing topology or the 64-byte CPU/GPU vertex ABI. The generator still has
input-dependent grouping work and an explicitly quadratic search that lets
degenerate corners inherit a neighboring good tangent. Vertex and byte limits
alone do not bound those paths tightly enough for adversarial local input.

## Decision

Admit a normal-textured triangle primitive with `POSITION` and `TEXCOORD_0`
when `TANGENT` is absent. Also generate when `NORMAL` is absent, after using the
existing flat-normal path; validate any declared source tangent completely and
then overwrite it. Preserve the explicit `NORMAL` plus `TANGENT` path exactly.
Material, coordinate, source-accessor, and unsupported-attribute validation
must finish before generation so fallback cannot mask malformed input.

Use exact-pinned `bevy_mikktspace` 1.0.0 with default features disabled and
only `std`, `corrected-edge-sorting`, and `corrected-vertex-welding` enabled.
The registry package checksum is
`bff34eb29ff4b8a8688bc7299f14fb6b597461ca80fec03ed7d22939ab33e48f`
and its source commit is `9de78a281bca0505142da521eb5152146da28656`.
The corrected welding path removes the legacy equal-position scan. The
corrected edge sort repairs the reference implementation's last-triangle
off-by-one and recursive hostile-input path; it can therefore differ on that
last triangle from the broken reference. Cogniform promises deterministic
behavior for its declared toolchain and platform class, not byte identity with
the broken reference or across architectures.

Before library entry:

- count each exact finite expanded-corner key over position, normal, and
  `TEXCOORD_0` bits in a checked ordered map and require the sum of cubed key
  multiplicities to be at most `268435456`; and
- classify triangles exactly as the library does by adjacent repeated position
  using finite f32 equality, including signed-zero equality, and require
  `9 * degenerate_faces * good_faces` to be at most `16777216`.

Drop the temporary map before generation. Checked overflow or either exceeded
limit rejects with `CollectionLimitExceeded` at
`glb.decoded.generated_tangent_work` and adopts no asset.

Write directly into the uncommitted expanded vertex vector after replacing all
tangent fields with an invalid zero sentinel. An absent callback result stays
invalid. A present result must be finite, have W exactly `-1` or `1`, have
non-zero XYZ renormalized through the existing f64 helper, and keep one W sign
per triangle. A future library error or unsuitable result rejects with
`InvalidTangent` at `glb.decoded.generated_tangents`; no partial vector reaches
an `Arc` or upload job.

Do not generate coordinates, add a primitive mode, change material/shader
behavior, alter hashing/replay/lifecycle semantics, or add topology, protocol,
transport, persistence, release, or deployment authority.

## Consequences

- Supported normal-textured triangle GLBs can omit source tangents, including
  indexed input, mirrored UV seams, and neighboring degenerate faces that have
  a valid inherited result.
- Isolated degenerates without a generated value reject instead of silently
  using the disabled fallback tangent.
- The exact 64-byte `AssetVertex`, renderer layout, shader, material flags,
  role accounting, eviction, and rehydration behavior remain unchanged.
- The dependency adds one pure-Rust, dependency-free, build-script-free
  package. Its manifest declares Rust 1.85.0; Cogniform continues to require
  its pinned Rust 1.97.1 workspace toolchain. The package forbids unsafe code
  and is licensed `Zlib AND (MIT OR Apache-2.0)`.
- A controlled release-mode maximum-bound import covering 262,143 separated
  corners, 262,143 same-position/distinct-UV corners, and maximum overlapping
  rejection completed in 0.32 seconds; a 5 ms process poll observed a peak
  working set of 82,296,832 bytes (78.48 MiB). This is one-machine
  informational evidence, not a portable performance threshold.

## Status

Accepted and implemented by CF065. Focused CPU tests cover exact source
compatibility and malformed-source precedence, indexed and non-indexed
generation, missing-normal overwrite, mirrored seams, neighboring and isolated
degenerates, exact work-budget boundaries, all-unique adversarial input, and
signed-zero degeneracy parity, and maximum resource cases. A controlled Vulkan
test proves explicit and generated
tangent fixtures produce equal output through the unchanged renderer path. The
focused plane and mirrored-seam fixtures are repository-authored under the
project license rather than copied from the Khronos sample corpus.
