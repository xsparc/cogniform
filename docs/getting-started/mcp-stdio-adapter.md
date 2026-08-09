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
`cogniform.query_scene` and `cogniform.submit_imagination`. Tool arguments are
the complete snake-case Cogniform core objects described in the
[gateway guide](../protocol/local-gateway-and-imagination.md). Use one stable
idempotency key for retries: replay returns the retained compilation/receipt
without applying another revision.

This profile is local and single-user. The parent must protect scene and
compilation values as sensitive data and supply identity, authorization,
confidentiality, freshness, rate limits, and process supervision before any
broader exposure. See the complete [protocol contract](../protocol/mcp-stdio-adapter.md).

The controlled end-to-end child proof is opt-in on an approved DX12 or Vulkan
adapter:

```text
cargo test --release -p cogniform-cli --test mcp_stdio --all-features --locked --offline -- --ignored
```
