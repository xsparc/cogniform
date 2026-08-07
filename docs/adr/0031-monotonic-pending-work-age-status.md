# ADR 0031: Monotonic pending-work age status

- Status: Accepted
- Date: 2026-08-07
- Task: CF031

## Context

The software design requires every bounded queue to expose depth, age, and
overload evidence. The local service already reports command depth and outcome
counters, outstanding observation count, pending asset imports, and pending
renderer uploads, but it cannot distinguish healthy queued work from a stalled
caller-driven scheduler. ADR 0007 explicitly deferred command age until a
service boundary existed.

Four transient work lifecycles now meet at `LocalService`: mutating commands,
observations, CPU asset imports, and GPU asset uploads. Their payloads and
identities may be caller-sensitive, and none belongs in replay, recovery,
logical hashing, persistence, or a background telemetry system.

Three approaches were considered:

1. add command age alone at the gateway;
2. introduce a background metrics worker or transport exporter; or
3. retain one monotonic admission instant with each bounded lifecycle object
   and derive aggregate age only when an existing status method is called.

The first leaves the same observability gap in the other service-owned queues.
The second introduces scheduling, shutdown, privacy, and deployment boundaries
that the local API does not otherwise need. The third completes the existing
bounded status contract without changing ownership or progress semantics.

## Decision

Each successfully admitted command, asset import, and renderer upload retains
one `std::time::Instant` beside its existing transient queue entry. An
observation permit retains its reservation instant in the same bounded shared
permit state from successful reservation through queued, active, and
completed-awaiting-delivery phases. Dropping the permit removes that record on
submission failure, successful delivery, delivery of a renderer/readback
error, channel failure, or queue destruction.

Existing aggregate status values add optional elapsed-microsecond fields:

- `GatewayQueueStats::oldest_pending_age_micros`;
- `LocalServiceStatus::oldest_outstanding_observation_age_micros`;
- `AssetStoreStats::oldest_pending_import_age_micros`; and
- `RendererAssetStats::oldest_pending_upload_age_micros`.

An empty lifecycle reports `None`. Otherwise an explicit status call samples
the monotonic clock and returns the greatest wait among retained work as a
saturating `u64` microsecond count. `LocalService::status` uses one sample for
its command and observation values. The asset status remains a bounded
composition of the independently owned CPU and renderer status contracts.

Admission and removal semantics stay exact:

- an identical queued command or upload and an already-known asset retain the
  original age;
- `LatestWins` replacement retains queue position but starts the replacement's
  age at its own admission;
- replayed, conflicting, dropped, invalid, failed, and capacity-rejected work
  does not add or alter a retained timestamp;
- processing or explicit eviction removes the selected command, import, or
  upload timestamp while preserving unrelated FIFO work; and
- observation capacity and age share the permit lifecycle, so release cannot
  leave a timestamp or consume an extra slot.

The instants are process-local implementation state. Public status and debug
output contain only optional aggregate elapsed microseconds, never a system
timestamp, queued payload, new request identifier, or automatic emission.
Instants are excluded from canonical encoding, command fingerprints, asset
identity, world state, logical hashes, replay, recovery, persisted files, and
observation metadata. Private injected-time seams make the lifecycle rules and
saturation deterministic in unit tests.

## Consequences

- Operators can detect the oldest stalled local work without inspecting
  payloads or enabling an exporter.
- Status sampling is bounded linear work over already bounded collections; it
  performs no I/O, allocation proportional to payload size, or background
  scheduling.
- The observation permit's bounded age registry uses a short standard-library
  lock to keep count, admission, status, and release consistent across the
  existing worker boundary. It does not change the fixed global capacity.
- The new public struct fields are additive but can break downstream exhaustive
  struct construction. Cogniform remains unpublished at `0.0.0`; no version or
  release action is taken.
- Active synchronous operation spans, logging, tracing, exporters, alerts,
  remote transport, authentication, persistent metrics, and production SLOs
  remain separate approved work.
