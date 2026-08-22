# ADR 0064: Select bounded named stdio profiles at launch

- Status: Accepted
- Date: 2026-08-22
- Task: CF064

## Context

The inherited-stream binary and MCP composition roots used one fixed 64x64
local service. That size preserves a cheap default, but it prevents a trusted
local parent from choosing a more useful image shape even though the renderer,
observation envelope, binary frame, and MCP output contracts already have
larger fixed bounds.

Arbitrary dimensions would create a new capacity-planning and denial-of-
service surface. Environment-derived configuration or per-request negotiation
would also make launch behavior less explicit and could grant an untrusted
protocol peer authority over GPU and output allocation.

## Decision

Supersede only ADR 0042's no-subargument/fixed-64x64 composition clauses and
ADR 0045's fixed-64x64 local-service clause. Their stream ownership,
scheduling, lifecycle, failure, transport, and authority decisions remain
accepted.

Give both `cogniform-cli serve-stdio` and `cogniform-cli serve-mcp-stdio` the
same CLI-private launch grammar:

```text
[--profile <default-local-64x64|local-256x256|local-480x270>]
```

Omitting the option is exactly `default-local-64x64`. Accept the flag once, in
that order, with one exact Unicode profile name and no other argument. Reject
missing, unknown, non-Unicode, reordered, duplicate, or extra values before
standard-stream inspection, async runtime creation, adapter selection, or
local-service construction. The stable error names only the allowlist and
never echoes caller input.

Each accepted name maps directly to immutable `(width, height)` constructor
values. The stream and MCP protocols do not expose a profile field, dimension
override, or negotiation path. The profile is fixed for the child lifetime.

The widest profile has 129,600 pixels. Its maximum current EntityId payload is
2,203,260 canonical envelope bytes and 2,937,680 base64 bytes. Those exact
values fit the existing runtime pixel limit, 4 MiB observation-envelope and
binary-bulk limits, and 8 MiB MCP output limit. Tests derive the header size
from the observation codec, encode the worst payload, exercise both optimized
child transports, and read the MCP resource with an official client.

Do not add arbitrary dimensions, environment configuration, config files,
per-request profile authority, protocol negotiation, new endpoints,
subscriptions, templates, history, dependencies, or release action.

## Consequences

- Existing zero-argument launches and their 64x64 protocol behavior remain
  unchanged.
- Operators may select a bounded square or 16:9 local observation shape before
  any GPU work. Larger profiles increase GPU work and sensitive output volume,
  so the parent still owns rate policy, timeout, supervision, and retention.
- Profile names are CLI launch policy, not new public Rust, persisted, world,
  session, observation, or MCP schema.
- Rollback removes the optional argument and restores the fixed zero-argument
  composition. No data migration is required.

## Status

Accepted and implemented by CF064. Ordinary tests cover the complete argument
grammar, no-service preflight, immutable mappings, and cross-layer bounds.
Controlled optimized Windows/Vulkan tests cover both transports at 480x270 and
retain the existing zero-argument flows. The official MCP client validates the
exact wide resource metadata, decoded size, base64 size, and payload count.
