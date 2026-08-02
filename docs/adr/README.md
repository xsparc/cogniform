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

New records use four sections: context, decision, consequences, and status.
