# Start the local MCP adapter

Build the exact locked offline workspace, then configure an MCP parent to
launch:

```text
cogniform-cli serve-mcp-stdio
```

Omission selects `default-local-64x64`. A trusted parent may append exactly
`--profile local-256x256` or `--profile local-480x270`. No other argument is
accepted, and the selected dimensions are immutable for the child lifetime;
MCP does not advertise or negotiate profile authority.

Both stdin and stdout must be redirected pipes. Do not run the command in an
interactive terminal. Stdout is newline-delimited MCP JSON-RPC and must not be
mixed with application logs; stable payload-redacted failures use stderr.
Continuously drain complete response lines while the child runs: the widest
EntityId resource read contains 2,937,680 base64 bytes before its JSON wrapper.

Choose either exact lifecycle:

- initialize a legacy session with MCP `2025-11-25`; or
- send `server/discover` or a direct supported request with exact MCP
  `2026-07-28` plus client capabilities in that request's `_meta`.

Repeat the modern protocol version and capabilities on every modern request;
discovery does not establish inherited request context. The connection cannot
switch eras. Both paths expose exactly `cogniform.query_scene`,
`cogniform.submit_imagination`, `cogniform.apply_patch`, and
`cogniform.observe_scene`, in that order. Tool arguments are the complete
snake-case Cogniform core objects described in the
[gateway guide](../protocol/local-gateway-and-imagination.md). Use one stable
idempotency key for retries: replay returns the retained compilation/receipt
without applying another revision.

Legacy initialization and modern discovery return one exact workflow
instruction. For a fresh child, query revision zero; thereafter use only
revisions returned by receipts or metadata. Choose `submit_imagination` for
semantic work and `apply_patch` for a complete direct change. Reuse both
transaction ID and idempotency key only for an exact retry. Add a camera before
observation, then read the returned `cogniform://` resource. Calls are
serialized. Discard the child after `service_failed`,
`invalid_service_output`, `observation_timeout`, or mutating
`output_unavailable`; never infer or retry an uncertain effect.

To stop an active request, send `notifications/cancelled` with that exact MCP
request ID as the next message. A matching cancellation produces no response,
terminates the child successfully after bounded cleanup, and leaves later
input undispatched. Reap and replace the child; cancellation is not a receipt
and does not prove whether synchronous work completed. Missing, different, or
post-response-write IDs are late/nonmatching and do not interrupt the active
call. Optional cancellation reason text is ignored and not logged. Observation
polling notices matching cancellation cooperatively, keeps
the prior completed resource until teardown, and never rolls back admitted
work.

Modern success results contain `resultType: "complete"` and informational
server identity. Discovery, `tools/list`, `resources/list`, and
`resources/read` use `ttlMs: 0` and private cache scope; do not reuse a prior
latest-resource listing as fresh state. The adapter advertises no extensions.
A client extension declaration alone is tolerated for core calls but grants no
authority. Tasks, multi-round-trip input, subscriptions, Apps, prompts,
sampling, models, and other SDK capabilities remain unsupported.

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

Every named profile is local and single-user. The parent must protect scene,
compilation, observation, and resource values as sensitive data and supply
identity, authorization, confidentiality, freshness, rate limits, and process
supervision before any broader exposure. The adapter supplies no general
deadline or restart policy; the parent still owns timeout, kill, and reap
fallbacks for blocked synchronous work. See the complete
[protocol contract](../protocol/mcp-stdio-adapter.md).

The controlled end-to-end child proof is opt-in on an approved DX12 or Vulkan
adapter:

```text
cargo test --release -p cogniform-cli --test mcp_stdio --all-features --locked --offline -- --ignored
```
