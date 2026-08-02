# Run the canonical headless scenario

The canonical scenario is the smallest unattended proof of Cogniform's MVP.
It runs locally without a visible window, network service, model, credential,
or external data.

## Prerequisites

- the pinned Rust toolchain selected by `rust-toolchain.toml`;
- the checked-in dependency sources under `vendor/`; and
- a Windows DX12 or Linux/Windows Vulkan adapter accepted by the current
  headless renderer.

From the repository root, run:

```text
cargo run -p cogniform-cli --locked --offline -- scenario
```

The command exits successfully only after it has:

1. created a room, table, light, and camera in one atomic patch;
2. moved and changed the table's material in one second atomic patch;
3. queried the exact committed revision and found all four entities;
4. observed the updated table in color and entity-ID images;
5. linked non-zero table visibility to the same revision and camera; and
6. verified the accepted-event chain and replayed it to the same logical hash.

Successful output reports revision `2`, four entities, stable table and camera
IDs, three monotonic frame IDs, the center color and entity ID, visible table
pixels, matching 64-character logical hashes, two replay entries, and the
bounded replay byte count. Wall-clock timestamps and local paths are not
printed.

## Failure behavior

The command fails closed with a concise diagnostic when adapter creation,
capacity validation, patch admission/application, exact-revision query,
observation production, payload validation, chain verification, or hash
comparison fails. Each observation has a ten-second default deadline and is
consumed before the next request. The deadline is configurable from one
nanosecond through sixty seconds, so it cannot overflow or become an unbounded
wait. The default two-slot observation pool is never overcommitted.

The scenario requires a newly initialized empty service. It is a conformance
flow, not a scene import command. Its room and table are built-in cuboids, and
the current reference renderer displays flat material colors rather than
lighting the scene. Pixel coverage can vary within the renderer's declared
visual tolerance; stable identity, revision causality, and logical replay hash
remain exact.

The controlled integration equivalent is intentionally ignored by ordinary
workspace tests because standard hosted CI does not promise a compatible GPU.
On an approved adapter it can be run explicitly with:

```text
cargo test -p cogniform-engine --test canonical_mvp --locked --offline -- --ignored
```

See the [local service contract](../protocol/local-service.md) for the API,
bounds, and deferred transport/persistence work.
