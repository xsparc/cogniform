# Start the local MCP adapter

Build the exact locked offline workspace, then configure an MCP parent to
launch:

```text
cogniform-cli serve-mcp-stdio
```

Both stdin and stdout must be redirected pipes. Do not run the command in an
interactive terminal. Stdout is newline-delimited MCP JSON-RPC and must not be
mixed with application logs; stable payload-redacted failures use stderr.

The parent must request MCP `2025-11-25`. It can then list and invoke exactly
`cogniform.query_scene`, `cogniform.submit_imagination`,
`cogniform.apply_patch`, and `cogniform.observe_scene`, in that order. Tool
arguments are the complete snake-case Cogniform core objects described in the
[gateway guide](../protocol/local-gateway-and-imagination.md). Use one stable
idempotency key for retries: replay returns the retained compilation/receipt
without applying another revision.

Use `cogniform.apply_patch` when the caller already has one complete validated
atomic scene change, including components outside the current semantic
compiler subset such as `camera`. Supply the exact current `base_revision`,
`require_exact_base`, explicit delivery and declared budgets, and at least one
operation. A successful new call returns `queued` with an `applied` receipt; an
exact retry returns `replayed` with `idempotent_replay` and the same resulting
revision. Changed work requires a fresh transaction and idempotency key.

To observe a fresh scene, first use `cogniform.apply_patch` to add a camera.
Then call `cogniform.observe_scene` with a complete request whose revision is
the exact current scene revision. A successful call returns causal metadata
and a `cogniform://observations/...` resource link. `resources/list` exposes
only that latest resource; `resources/read` returns one base64 binary item with
media type `application/vnd.cogniform.observation-envelope`. Decode the bytes
with the version-one `COGOBS01`
[observation-payload contract](../protocol/observation-payload-envelope.md).
The server advertises no resource templates, subscriptions, list-change
notifications, history, or persistence.

This profile is local and single-user. The parent must protect scene,
compilation, observation, and resource values as sensitive data and supply identity, authorization,
confidentiality, freshness, rate limits, and process supervision before any
broader exposure. See the complete [protocol contract](../protocol/mcp-stdio-adapter.md).

The controlled end-to-end child proof is opt-in on an approved DX12 or Vulkan
adapter:

```text
cargo test --release -p cogniform-cli --test mcp_stdio --all-features --locked --offline -- --ignored
```
