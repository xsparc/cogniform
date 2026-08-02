# ADR 0010: Source-first release profile

- Status: Accepted
- Date: 2026-08-02
- Task: CF009

## Context

Cogniform has completed the local MVP flow, but a release format must not imply
platform, API, deployment, or operational guarantees that have not been
demonstrated. The workspace is still versioned `0.0.0`, every package has
`publish = false`, there is one controlled full-runtime platform result, and no
remote service or persistent recovery composition exists.

The design also contains a stale acceptance phrase about normal observations.
The renderer design names normals as a later output, while the MVP success flow
requires color, exact entity identity, and visibility. The implemented baseline
also exposes depth with numeric validation. Treating normal output as already
supported would be a false release claim.

Three packaging choices were considered:

1. publish the internal crates to crates.io;
2. ship prebuilt binaries, installers, or containers for several platforms; or
3. prepare a source-first release candidate for the one validated local profile
   and defer publication to a separately approved release action.

Crate publication would prematurely freeze internal package boundaries and
versions. Prebuilt artifacts would imply an unsupported binary and driver
matrix. A source-first candidate preserves the current honest support boundary.

## Decision

The first eligible release candidate is source-first. A future explicitly
approved release task may change the workspace version to `0.1.0-rc.1`, create
an annotated maintainer tag, and attach one source archive plus its SHA-256
checksum. The archive includes the pinned toolchain declaration, lockfile,
vendored dependency sources, license, documentation, and tests. No crate,
binary, installer, container, package-manager formula, or deployment image is
published in the initial profile.

The validated full-runtime profile is Windows 11 x86_64 with Rust 1.97.1 and a
Vulkan discrete GPU that passes the renderer's capability negotiation. The
current evidence device is an NVIDIA GeForce RTX 5070 reporting WebGPU-compliant
downlevel capabilities. This is a validation entry, not a vendor-exclusive
requirement. Ubuntu on the standard GitHub runner is build, lint, unit-test, and
documentation evidence only. Linux GPU runtime, Windows DX12, software fallback,
macOS, mobile, and browser execution are not release-supported until separately
reproduced.

The MVP observation profile is color, normalized depth, exact entity ID, and
structured visibility. Normal output remains a documented post-MVP addition.
The software design acceptance matrix is corrected to match that explicit
profile rather than claiming an absent attachment.

Performance measurements are informational. The versioned
`world-create-empty-v1` fixture, sampling method, build profile, hardware, and
raw min/median/p95/max summary are recorded. Research targets do not become a
merge or release gate from one machine result.

Release publication remains manual and separately authorized. CF009 prepares
the evidence and checklist but does not bump the version, create a tag, build an
archive, publish a GitHub release, or upload any artifact.

## Consequences

- A clean source archive can build and test offline after the pinned Rust
  toolchain is installed.
- Consumers must build locally and accept the early API and platform limits.
- `publish = false` remains on every crate; there is no crates.io compatibility
  promise or semver-stable library surface.
- The initial support statement is narrow and evidence-based. Additional
  operating systems, backends, and GPUs require controlled reproduction before
  the matrix changes.
- A failed or superseded release candidate is followed by a new candidate tag;
  published tags and archives are not silently replaced.
- Binary signing, provenance attestations, SBOM publication, installers,
  containers, remote authentication, and production deployment remain future
  decisions rather than implicit MVP features.
