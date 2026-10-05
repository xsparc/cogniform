# ADR 0083: Add bounded MCP observation image content

- Status: Accepted
- Date: 2026-09-30
- Task: CF083
- Extends: [ADR 0047](0047-bounded-mcp-observation-resource.md),
  [ADR 0054](0054-bounded-dual-era-mcp-stdio-lifecycle.md),
  [ADR 0081](0081-create-new-rendered-observation-examples.md), and
  [ADR 0082](0082-create-new-canonical-scenario-render-bundle.md)

## Context

`cogniform.observe_scene` retains an exact causal `COGOBS01` resource, but an
MCP client must fetch and decode that binary envelope before it can inspect a
render. The CLI diagnostics already prove useful PNG transformations for color,
depth, world normals, and stable entity IDs. Duplicating those transformations
inside the MCP adapter would let CLI and protocol views drift.

MCP `2026-07-28` defines base64 `ImageContent` as a tool-result content block,
while resources remain the mechanism for URI-addressable binary data. The
same image block is representable by the pinned SDK in Cogniform's accepted
legacy lifecycle. An image derivative can therefore improve direct agent and
human inspection without replacing the exact resource or adding another tool.

## Decision

Add optional `presentation: "png"` to `cogniform.observe_scene`. Omission keeps
the previous content sequence and structured output exactly: one text block,
one canonical resource link, and no presentation descriptor. Color, depth,
normal, and entity-ID opt-in calls return the text block, one base64
`ImageContent` with media type `image/png`, and the same canonical resource
link. Structured output adds only
`presentation: {mime_type, size, diagnostic_only}`. Visibility has no image
projection and rejects as `invalid_observation` before lazy service creation.

Move the CF081/CF082 visualization and PNG encoding into
`cogniform-observation` as a pure borrowed-input operation. Preserve linear
RGBA8 color and its gamma marker, inverted normalized grayscale depth,
world-space normal remapping with transparent absence, and deterministic
identity colors with transparent background. The CLI uses the same operation
and its existing artifacts remain byte-identical.

The complete encoded PNG is limited to 1,048,576 bytes independently of the
4 MiB canonical envelope. Dimension, item-count, checked-size, and allocation
validation precede completion. The adapter prepares and base64-encodes the
canonical resource first, then prepares the optional PNG, then builds the
complete result. It replaces the retained resource only when all three steps
succeed. Presentation size rejection is `observation_too_large`; allocation or
encoding failure is `output_unavailable`; impossible service shape is
`invalid_service_output` and poisons the child.

The PNG is diagnostic-only. Numeric depth/normals, stable identities, metadata,
and causality remain authoritative only in `COGOBS01`. This decision adds no
renderer behavior, file output, resource history, templates, subscriptions,
prompts, tasks, model access, listener, network authority, or persistence.

The MCP references consulted for this decision are the official
[2026-07-28 general-availability announcement](https://github.com/modelcontextprotocol/modelcontextprotocol/blob/main/blog/content/posts/2026-07-28-spec-ga/index.md),
[tool result specification](https://github.com/modelcontextprotocol/modelcontextprotocol/blob/main/docs/specification/2026-07-28/server/tools.mdx),
and [resource specification](https://github.com/modelcontextprotocol/modelcontextprotocol/blob/main/docs/specification/2026-07-28/server/resources.mdx).

## Consequences

- Agent clients can inspect rendered appearance, occlusion, orientation, and
  entity segmentation directly in one tool result.
- The canonical resource remains available in the same result for exact
  numeric or identity reasoning and later `resources/read`.
- One shared implementation prevents CLI and MCP visualization drift.
- Base64 expands the image on the wire, but the independent PNG bound and
  existing 8 MiB output-line bound contain the result.
- A failed optional presentation preserves the previous retained resource.

## Status

Accepted and implemented by CF083. Unit tests cover exact transforms, gamma,
shape and encoded-size bounds, the widest named profile, omission
compatibility, visibility rejection, prior-resource preservation, all four
renderable kinds, and both accepted MCP eras. Controlled legacy and modern CLI
child tests cover real rendered image content, and the CLI graphics bundles
retain their exact pre-change hashes.
