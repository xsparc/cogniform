# Architecture Decision Records

Architecture decision records capture choices that constrain future Cogniform
work. Accepted records are not silently rewritten; a later record supersedes an
earlier one and links the history.

| ADR | Status | Decision |
|---|---|---|
| [0001](0001-rust-workspace-and-domain-boundaries.md) | Accepted | Establish the Rust workspace and exclusive domain boundaries |
| [0002](0002-bounded-canonical-json-contracts.md) | Accepted | Use bounded canonical JSON contracts with exact-pinned vendored Serde |
| [0003](0003-atomic-world-preflight-and-stable-identity.md) | Accepted | Preflight atomic patches and keep stable identity outside ECS handles |
| [0004](0004-stable-hierarchy-canonical-hash-and-replay-chain.md) | Accepted | Keep hierarchy stable-ID based and replay canonical state through a bounded hash chain |
| [0005](0005-bounded-headless-wgpu-baseline.md) | Accepted | Use an exact-pinned, bounded DX12/Vulkan wgpu core for offscreen reference rendering |
| [0006](0006-coalesced-extraction-and-bounded-observations.md) | Accepted | Drain coalesced render changes into revision-linked frames and bounded observations |
| [0007](0007-pure-imagination-compiler-and-bounded-local-gateway.md) | Accepted | Compile bounded primitive imaginations purely and admit local commands with explicit queue semantics |
| [0008](0008-content-addressed-assets-and-pure-built-in-procedures.md) | Accepted | Separate content-addressed CPU import, logical asset references, bounded GPU upload, and pure built-in procedures |
| [0009](0009-recorded-engine-and-local-typed-service.md) | Accepted | Record engine mutations and expose a bounded local typed service with a canonical unattended scenario |
| [0010](0010-source-first-release-profile.md) | Accepted | Prepare a narrow source-first candidate profile without publishing crates, binaries, or a release |

New records use four sections: context, decision, consequences, and status.
