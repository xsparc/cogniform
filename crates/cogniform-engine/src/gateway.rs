use core::num::NonZeroU32;
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    time::{Duration, Instant},
};

use cogniform_compiler::{
    CompilationLimits, CompilationResult, CompilationSceneView, CompilerConfig,
    DeterministicCompiler,
};
use cogniform_protocol::{
    ApplyReceipt, ApplyStatus, DeliverySemantic, IdempotencyKey, ImaginationEnvelope,
    RuntimeLimits, SceneEntityView, ScenePatch, SceneQuery, SceneQueryResult, SceneText,
    SchemaVersion,
};
use cogniform_world::WorldSnapshot;
use sha2::{Digest, Sha256};

use crate::{CogniformEngine, GatewayError};

const COMMAND_FINGERPRINT_DOMAIN: &[u8] = b"cogniform.gateway.command.v1\0";

/// Fixed capacities for one offline in-process command gateway.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GatewayConfig {
    /// Maximum uncommitted patch or imagination commands.
    pub command_capacity: NonZeroU32,
    /// Maximum queued plus accepted unique idempotency records.
    pub idempotency_capacity: NonZeroU32,
}

impl Default for GatewayConfig {
    fn default() -> Self {
        Self {
            command_capacity: NonZeroU32::new(64).expect("constant is non-zero"),
            idempotency_capacity: NonZeroU32::new(1_024).expect("constant is non-zero"),
        }
    }
}

/// One typed mutating command admitted by the local gateway.
#[derive(Debug, Clone, PartialEq)]
pub enum GatewayCommand {
    /// Explicit already-normalized scene patch.
    Patch(ScenePatch),
    /// Semantic primitive imagination compiled immediately before application.
    Imagination(ImaginationEnvelope),
}

impl GatewayCommand {
    fn idempotency_key(&self) -> IdempotencyKey {
        match self {
            Self::Patch(patch) => patch.idempotency_key,
            Self::Imagination(imagination) => imagination.idempotency_key,
        }
    }

    fn delivery(&self) -> &DeliverySemantic {
        match self {
            Self::Patch(patch) => &patch.delivery,
            Self::Imagination(imagination) => &imagination.delivery,
        }
    }

    fn supersession_key(&self) -> Option<&SceneText> {
        match self.delivery() {
            DeliverySemantic::LatestWins { supersession_key } => Some(supersession_key),
            DeliverySemantic::MustApply | DeliverySemantic::BestEffort => None,
        }
    }

    fn fingerprint(&self, limits: &RuntimeLimits) -> Result<[u8; 32], GatewayError> {
        let (kind, encoded) = match self {
            Self::Patch(patch) => (0_u8, patch.to_canonical_json(limits)),
            Self::Imagination(imagination) => (1_u8, imagination.to_canonical_json(limits)),
        };
        let encoded = encoded.map_err(GatewayError::InvalidCommandEncoding)?;
        let mut hasher = Sha256::new();
        hasher.update(COMMAND_FINGERPRINT_DOMAIN);
        hasher.update([kind]);
        hasher.update(encoded);
        Ok(hasher.finalize().into())
    }
}

/// Immediate result of attempting bounded command admission.
#[derive(Debug, Clone, PartialEq)]
pub enum GatewayAdmission {
    /// A new command was appended without exceeding capacity.
    Queued {
        /// Idempotency key reserved by the command.
        idempotency_key: IdempotencyKey,
    },
    /// An identical command is already queued and was not duplicated.
    AlreadyQueued {
        /// Existing queued idempotency key.
        idempotency_key: IdempotencyKey,
    },
    /// A latest-value command replaced an older uncommitted command in place.
    Superseded {
        /// New command idempotency key.
        idempotency_key: IdempotencyKey,
        /// Discarded uncommitted command idempotency key.
        superseded_idempotency_key: IdempotencyKey,
    },
    /// Best-effort work was intentionally dropped under pressure.
    Dropped {
        /// Idempotency key that was not reserved.
        idempotency_key: IdempotencyKey,
    },
    /// An accepted result was returned without queueing or applying again.
    Replayed {
        /// Recorded response marked as an idempotent replay.
        response: Box<GatewayResponse>,
    },
}

/// Result of processing one admitted command.
#[derive(Debug, Clone, PartialEq)]
pub enum GatewayResponse {
    /// Receipt for one explicit patch.
    PatchApplied {
        /// Atomic world receipt.
        receipt: ApplyReceipt,
    },
    /// Compiler explanation and optional apply receipt for one imagination.
    ImaginationProcessed {
        /// Pure deterministic compiler result.
        compilation: Box<CompilationResult>,
        /// Atomic receipt, absent when constraints were unresolved.
        receipt: Option<ApplyReceipt>,
    },
}

impl GatewayResponse {
    fn as_replayed(&self) -> Self {
        let mut replayed = self.clone();
        match &mut replayed {
            Self::PatchApplied { receipt }
            | Self::ImaginationProcessed {
                receipt: Some(receipt),
                ..
            } => receipt.status = ApplyStatus::IdempotentReplay,
            Self::ImaginationProcessed { receipt: None, .. } => {}
        }
        replayed
    }
}

/// Monotonic bounded-queue counters for overload inspection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GatewayQueueStats {
    /// Current uncommitted command depth.
    pub depth: u32,
    /// Monotonic elapsed microseconds for the oldest uncommitted command.
    pub oldest_pending_age_micros: Option<u64>,
    /// Commands appended without supersession.
    pub admitted: u64,
    /// Uncommitted latest-value commands replaced in place.
    pub superseded: u64,
    /// Best-effort commands dropped under pressure.
    pub dropped: u64,
    /// Durable or new-key latest-value commands rejected under pressure.
    pub rejected: u64,
}

/// Offline gateway that owns one engine and admits only bounded typed work.
pub struct LocalGateway {
    engine: CogniformEngine,
    compiler: DeterministicCompiler,
    queue: CommandQueue,
    completed: BTreeMap<IdempotencyKey, CompletedRecord>,
    idempotency_capacity: u32,
    limits: RuntimeLimits,
}

impl std::fmt::Debug for LocalGateway {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("LocalGateway")
            .field("revision", &self.engine.revision())
            .field("queue", &self.queue)
            .field("completed", &self.completed.len())
            .field("idempotency_capacity", &self.idempotency_capacity)
            .finish_non_exhaustive()
    }
}

impl LocalGateway {
    /// Wraps one engine with bounded command admission and deterministic compilation.
    pub fn new(engine: CogniformEngine, config: GatewayConfig) -> Result<Self, GatewayError> {
        let limits = engine.runtime_limits();
        let available_world_records = engine
            .max_idempotency_records()
            .saturating_sub(engine.idempotency_record_count());
        validate_config(config, &limits, available_world_records)?;
        Ok(Self {
            engine,
            compiler: DeterministicCompiler::new(CompilerConfig::new(limits)),
            queue: CommandQueue::new(config.command_capacity),
            completed: BTreeMap::new(),
            idempotency_capacity: config.idempotency_capacity.get(),
            limits,
        })
    }

    /// Rebinds future deterministic compilation to narrower transport bounds.
    ///
    /// The command queue must be empty so every admitted imagination has one
    /// stable compiler policy from admission through processing. Retained
    /// completed responses are unchanged and remain subject to adapter output
    /// validation when replayed.
    pub(crate) fn configure_compilation_limits(
        &mut self,
        compilation_limits: CompilationLimits,
    ) -> Result<(), GatewayError> {
        if self.queue.depth() != 0 {
            return Err(GatewayError::InvalidConfig {
                reason: "compilation limits require an empty command queue",
            });
        }
        let available = CompilationLimits::for_runtime_limits(self.limits);
        if !compilation_limits_fit(&compilation_limits, &available) {
            return Err(GatewayError::InvalidConfig {
                reason: "compilation limits exceed gateway runtime policy",
            });
        }
        let mut compiler_config = CompilerConfig::new(self.limits);
        compiler_config.compilation_limits = compilation_limits;
        self.compiler = DeterministicCompiler::new(compiler_config);
        Ok(())
    }

    /// Admits one explicit patch according to its delivery semantic.
    pub fn submit_patch(&mut self, patch: ScenePatch) -> Result<GatewayAdmission, GatewayError> {
        patch
            .validate_with_limits(&self.limits)
            .map_err(GatewayError::InvalidCommand)?;
        self.admit(CommandRecord::new(
            GatewayCommand::Patch(patch),
            &self.limits,
        )?)
    }

    /// Admits one semantic imagination according to its delivery semantic.
    pub fn submit_imagination(
        &mut self,
        imagination: ImaginationEnvelope,
    ) -> Result<GatewayAdmission, GatewayError> {
        imagination
            .validate_with_limits(&self.limits)
            .map_err(GatewayError::InvalidCommand)?;
        self.admit(CommandRecord::new(
            GatewayCommand::Imagination(imagination),
            &self.limits,
        )?)
    }

    /// Processes at most one admitted command in FIFO/supersession order.
    pub fn process_next(&mut self) -> Result<Option<GatewayResponse>, GatewayError> {
        let Some(record) = self.queue.pop_front() else {
            return Ok(None);
        };
        let idempotency_key = record.idempotency_key();
        let response = match &record.command {
            GatewayCommand::Patch(patch) => GatewayResponse::PatchApplied {
                receipt: self.engine.apply_patch(patch)?,
            },
            GatewayCommand::Imagination(imagination) => {
                let snapshot = self.engine.world().snapshot()?;
                let scene = CompilationSceneView::new(
                    snapshot.revision(),
                    snapshot
                        .entities()
                        .iter()
                        .map(cogniform_world::EntitySnapshot::entity_id),
                );
                let compilation = self.compiler.compile(imagination, &scene)?;
                let receipt = compilation
                    .patch
                    .as_ref()
                    .map(|patch| self.engine.apply_patch(patch))
                    .transpose()?;
                GatewayResponse::ImaginationProcessed {
                    compilation: Box::new(compilation),
                    receipt,
                }
            }
        };
        debug_assert!(count(self.completed.len()) + self.queue.depth() < self.idempotency_capacity);
        self.completed.insert(
            idempotency_key,
            CompletedRecord {
                fingerprint: record.fingerprint,
                response: response.clone(),
            },
        );
        Ok(Some(response))
    }

    /// Executes one exact-revision bounded logical query without queueing mutation work.
    pub fn query(&self, query: &SceneQuery) -> Result<SceneQueryResult, GatewayError> {
        query
            .validate_with_limits(&self.limits)
            .map_err(GatewayError::InvalidCommand)?;
        let snapshot = self.engine.world().snapshot()?;
        execute_query(&snapshot, query, &self.limits)
    }

    /// Returns bounded overload counters and current queue depth.
    #[must_use]
    pub fn queue_stats(&self) -> GatewayQueueStats {
        self.queue_stats_at(Instant::now())
    }

    pub(crate) fn queue_stats_at(&self, sampled_at: Instant) -> GatewayQueueStats {
        self.queue.stats_at(sampled_at)
    }

    /// Returns the number of retained accepted gateway results.
    #[must_use]
    pub fn completed_result_count(&self) -> u32 {
        count(self.completed.len())
    }

    /// Returns read-only access to the owned engine.
    #[must_use]
    pub const fn engine(&self) -> &CogniformEngine {
        &self.engine
    }

    pub(crate) const fn engine_mut(&mut self) -> &mut CogniformEngine {
        &mut self.engine
    }

    /// Consumes the gateway and returns its engine.
    #[must_use]
    pub fn into_engine(self) -> CogniformEngine {
        self.engine
    }

    fn admit(&mut self, record: CommandRecord) -> Result<GatewayAdmission, GatewayError> {
        let idempotency_key = record.idempotency_key();
        if let Some(recorded) = self.completed.get(&idempotency_key) {
            if recorded.fingerprint != record.fingerprint {
                return Err(GatewayError::IdempotencyConflict { idempotency_key });
            }
            return Ok(GatewayAdmission::Replayed {
                response: Box::new(recorded.response.as_replayed()),
            });
        }
        let has_idempotency_capacity =
            count(self.completed.len()) + self.queue.depth() < self.idempotency_capacity;
        self.queue.admit(record, has_idempotency_capacity)
    }
}

fn compilation_limits_fit(proposed: &CompilationLimits, available: &CompilationLimits) -> bool {
    proposed.max_encoded_bytes <= available.max_encoded_bytes
        && proposed.max_decoded_bytes <= available.max_decoded_bytes
        && proposed.max_json_nesting_depth <= available.max_json_nesting_depth
        && proposed.max_text_bytes <= available.max_text_bytes
        && proposed.max_decisions <= available.max_decisions
        && proposed.max_unresolved_constraints <= available.max_unresolved_constraints
        && runtime_limits_fit(&proposed.patch_limits, &available.patch_limits)
}

fn runtime_limits_fit(proposed: &RuntimeLimits, available: &RuntimeLimits) -> bool {
    proposed.max_encoded_bytes <= available.max_encoded_bytes
        && proposed.max_decoded_bytes <= available.max_decoded_bytes
        && proposed.max_json_nesting_depth <= available.max_json_nesting_depth
        && proposed.max_operations <= available.max_operations
        && proposed.max_components <= available.max_components
        && proposed.max_components_per_entity <= available.max_components_per_entity
        && proposed.max_text_bytes <= available.max_text_bytes
        && proposed.max_diagnostics <= available.max_diagnostics
        && proposed.max_queue_capacity <= available.max_queue_capacity
        && proposed.max_imagination_entities <= available.max_imagination_entities
        && proposed.max_imagination_relations <= available.max_imagination_relations
        && proposed.max_imagination_constraints <= available.max_imagination_constraints
        && proposed.max_query_entities <= available.max_query_entities
        && proposed.max_observation_width <= available.max_observation_width
        && proposed.max_observation_height <= available.max_observation_height
        && proposed.max_observation_pixels <= available.max_observation_pixels
}

pub(crate) fn validate_config(
    config: GatewayConfig,
    limits: &RuntimeLimits,
    available_world_records: u32,
) -> Result<(), GatewayError> {
    if config.command_capacity.get() > limits.max_queue_capacity.get() {
        return Err(GatewayError::InvalidConfig {
            reason: "gateway command capacity exceeds the active protocol queue limit",
        });
    }
    if config.command_capacity.get() > config.idempotency_capacity.get() {
        return Err(GatewayError::InvalidConfig {
            reason: "gateway idempotency capacity must cover command capacity",
        });
    }
    if config.idempotency_capacity.get() > available_world_records {
        return Err(GatewayError::InvalidConfig {
            reason: "gateway idempotency capacity exceeds available world result retention",
        });
    }
    Ok(())
}

#[derive(Clone)]
struct CommandRecord {
    command: GatewayCommand,
    fingerprint: [u8; 32],
    queued_at: Instant,
}

impl CommandRecord {
    fn new(command: GatewayCommand, limits: &RuntimeLimits) -> Result<Self, GatewayError> {
        let fingerprint = command.fingerprint(limits)?;
        Ok(Self {
            command,
            fingerprint,
            queued_at: Instant::now(),
        })
    }

    #[cfg(test)]
    fn new_at(
        command: GatewayCommand,
        limits: &RuntimeLimits,
        queued_at: Instant,
    ) -> Result<Self, GatewayError> {
        let fingerprint = command.fingerprint(limits)?;
        Ok(Self {
            command,
            fingerprint,
            queued_at,
        })
    }

    fn idempotency_key(&self) -> IdempotencyKey {
        self.command.idempotency_key()
    }

    fn supersession_key(&self) -> Option<&SceneText> {
        self.command.supersession_key()
    }

    fn delivery(&self) -> &DeliverySemantic {
        self.command.delivery()
    }
}

#[derive(Clone)]
struct CompletedRecord {
    fingerprint: [u8; 32],
    response: GatewayResponse,
}

struct CommandQueue {
    capacity: u32,
    entries: VecDeque<CommandRecord>,
    admitted: u64,
    superseded: u64,
    dropped: u64,
    rejected: u64,
}

impl std::fmt::Debug for CommandQueue {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("CommandQueue")
            .field("capacity", &self.capacity)
            .field("stats", &self.stats())
            .finish_non_exhaustive()
    }
}

impl CommandQueue {
    fn new(capacity: NonZeroU32) -> Self {
        Self {
            capacity: capacity.get(),
            entries: VecDeque::with_capacity(
                usize::try_from(capacity.get()).expect("u32 queue capacity fits usize"),
            ),
            admitted: 0,
            superseded: 0,
            dropped: 0,
            rejected: 0,
        }
    }

    fn admit(
        &mut self,
        record: CommandRecord,
        has_idempotency_capacity: bool,
    ) -> Result<GatewayAdmission, GatewayError> {
        let idempotency_key = record.idempotency_key();
        if let Some(existing) = self
            .entries
            .iter()
            .find(|existing| existing.idempotency_key() == idempotency_key)
        {
            if existing.fingerprint == record.fingerprint {
                return Ok(GatewayAdmission::AlreadyQueued { idempotency_key });
            }
            return Err(GatewayError::IdempotencyConflict { idempotency_key });
        }

        if let Some(supersession_key) = record.supersession_key()
            && let Some(position) = self
                .entries
                .iter()
                .position(|existing| existing.supersession_key() == Some(supersession_key))
        {
            let superseded_idempotency_key = self.entries[position].idempotency_key();
            self.entries[position] = record;
            self.superseded = self.superseded.saturating_add(1);
            return Ok(GatewayAdmission::Superseded {
                idempotency_key,
                superseded_idempotency_key,
            });
        }

        let under_pressure = self.depth() >= self.capacity || !has_idempotency_capacity;
        if under_pressure && matches!(record.delivery(), DeliverySemantic::BestEffort) {
            self.dropped = self.dropped.saturating_add(1);
            return Ok(GatewayAdmission::Dropped { idempotency_key });
        }
        if self.depth() >= self.capacity {
            self.rejected = self.rejected.saturating_add(1);
            return Err(GatewayError::CommandCapacityExceeded {
                capacity: self.capacity,
            });
        }
        if !has_idempotency_capacity {
            self.rejected = self.rejected.saturating_add(1);
            return Err(GatewayError::IdempotencyCapacityExceeded);
        }

        self.entries.push_back(record);
        self.admitted = self.admitted.saturating_add(1);
        Ok(GatewayAdmission::Queued { idempotency_key })
    }

    fn pop_front(&mut self) -> Option<CommandRecord> {
        self.entries.pop_front()
    }

    fn depth(&self) -> u32 {
        count(self.entries.len())
    }

    fn stats(&self) -> GatewayQueueStats {
        self.stats_at(Instant::now())
    }

    fn stats_at(&self, sampled_at: Instant) -> GatewayQueueStats {
        GatewayQueueStats {
            depth: self.depth(),
            oldest_pending_age_micros: self
                .entries
                .iter()
                .map(|record| record.queued_at)
                .min()
                .map(|queued_at| elapsed_micros(sampled_at, queued_at)),
            admitted: self.admitted,
            superseded: self.superseded,
            dropped: self.dropped,
            rejected: self.rejected,
        }
    }
}

fn elapsed_micros(sampled_at: Instant, started_at: Instant) -> u64 {
    duration_micros(sampled_at.saturating_duration_since(started_at))
}

fn duration_micros(duration: Duration) -> u64 {
    u64::try_from(duration.as_micros()).unwrap_or(u64::MAX)
}

fn execute_query(
    snapshot: &WorldSnapshot,
    query: &SceneQuery,
    limits: &RuntimeLimits,
) -> Result<SceneQueryResult, GatewayError> {
    if query.scene_revision != snapshot.revision() {
        return Err(GatewayError::QueryRevisionMismatch {
            requested: query.scene_revision,
            actual: snapshot.revision(),
        });
    }
    let selected_ids: BTreeSet<_> = query.entity_ids.iter().copied().collect();
    let selected_kinds: BTreeSet<_> = query.component_kinds.iter().copied().collect();
    let matches = snapshot
        .entities()
        .iter()
        .filter(|entity| selected_ids.is_empty() || selected_ids.contains(&entity.entity_id()))
        .count();
    if count(matches) > query.limit.get() {
        return Err(GatewayError::QueryResultCapacityExceeded {
            actual: count(matches),
            limit: query.limit.get(),
        });
    }

    let entities = snapshot
        .entities()
        .iter()
        .filter(|entity| selected_ids.is_empty() || selected_ids.contains(&entity.entity_id()))
        .map(|entity| SceneEntityView {
            entity_id: entity.entity_id(),
            parent_id: entity.parent_id(),
            components: entity
                .components()
                .iter()
                .filter(|component| {
                    selected_kinds.is_empty() || selected_kinds.contains(&component.kind())
                })
                .cloned()
                .collect(),
        })
        .collect();
    let result = SceneQueryResult {
        schema_version: SchemaVersion::V1,
        scene_revision: snapshot.revision(),
        entities,
    };
    result
        .validate_with_limits(limits)
        .map_err(GatewayError::InvalidQueryResult)?;
    Ok(result)
}

fn count(length: usize) -> u32 {
    u32::try_from(length).unwrap_or(u32::MAX)
}

#[cfg(test)]
mod tests {
    use cogniform_protocol::{
        CodecError, ComponentKind, ConflictPolicy, DeliverySemantic, FrameId, IdempotencyKey,
        PatchBudget, ScenePatch, SceneRevision, SchemaVersion, TransactionId,
    };

    use super::*;

    fn patch(nonce: u128, delivery: DeliverySemantic) -> ScenePatch {
        ScenePatch {
            schema_version: SchemaVersion::V1,
            transaction_id: TransactionId::new(nonce).unwrap(),
            idempotency_key: IdempotencyKey::new(nonce + 100).unwrap(),
            base_revision: SceneRevision::INITIAL,
            conflict_policy: ConflictPolicy::RequireExactBase,
            delivery,
            declared_budget: PatchBudget::default(),
            operations: vec![cogniform_protocol::SceneOperation::Delete(
                cogniform_protocol::DeleteEntity {
                    entity_id: cogniform_protocol::StableEntityId::new(nonce + 1_000).unwrap(),
                },
            )],
        }
    }

    fn record(command: GatewayCommand) -> CommandRecord {
        CommandRecord::new(command, &RuntimeLimits::default()).unwrap()
    }

    fn record_at(command: GatewayCommand, queued_at: Instant) -> CommandRecord {
        CommandRecord::new_at(command, &RuntimeLimits::default(), queued_at).unwrap()
    }

    #[test]
    fn queue_delivery_semantics_remain_bounded_under_pressure() {
        let mut queue = CommandQueue::new(NonZeroU32::new(2).unwrap());
        let durable = GatewayCommand::Patch(patch(1, DeliverySemantic::MustApply));
        assert!(matches!(
            queue.admit(record(durable.clone()), true).unwrap(),
            GatewayAdmission::Queued { .. }
        ));
        assert!(matches!(
            queue.admit(record(durable), true).unwrap(),
            GatewayAdmission::AlreadyQueued { .. }
        ));

        let replaceable = GatewayCommand::Patch(patch(
            2,
            DeliverySemantic::LatestWins {
                supersession_key: SceneText::new("camera/main").unwrap(),
            },
        ));
        queue.admit(record(replaceable), true).unwrap();
        let replacement = GatewayCommand::Patch(patch(
            3,
            DeliverySemantic::LatestWins {
                supersession_key: SceneText::new("camera/main").unwrap(),
            },
        ));
        assert!(matches!(
            queue.admit(record(replacement), false).unwrap(),
            GatewayAdmission::Superseded { .. }
        ));
        assert_eq!(queue.depth(), 2);

        let dropped = GatewayCommand::Patch(patch(4, DeliverySemantic::BestEffort));
        assert!(matches!(
            queue.admit(record(dropped), false).unwrap(),
            GatewayAdmission::Dropped { .. }
        ));
        let rejected = GatewayCommand::Patch(patch(5, DeliverySemantic::MustApply));
        assert!(matches!(
            queue.admit(record(rejected), true),
            Err(GatewayError::CommandCapacityExceeded { capacity: 2 })
        ));
        let stats = queue.stats();
        assert_eq!(stats.depth, 2);
        assert!(stats.oldest_pending_age_micros.is_some());
        assert_eq!(stats.admitted, 2);
        assert_eq!(stats.superseded, 1);
        assert_eq!(stats.dropped, 1);
        assert_eq!(stats.rejected, 1);
    }

    #[test]
    fn queue_age_tracks_oldest_retained_admission_without_payload_identity() {
        let started_at = Instant::now();
        let mut queue = CommandQueue::new(NonZeroU32::new(2).unwrap());
        assert_eq!(queue.stats_at(started_at).oldest_pending_age_micros, None);

        let durable = GatewayCommand::Patch(patch(30, DeliverySemantic::MustApply));
        queue
            .admit(record_at(durable.clone(), started_at), true)
            .unwrap();
        assert!(matches!(
            queue
                .admit(
                    record_at(durable, started_at + Duration::from_micros(3)),
                    true,
                )
                .unwrap(),
            GatewayAdmission::AlreadyQueued { .. }
        ));

        let replaceable = |nonce| {
            GatewayCommand::Patch(patch(
                nonce,
                DeliverySemantic::LatestWins {
                    supersession_key: SceneText::new("camera/age").unwrap(),
                },
            ))
        };
        queue
            .admit(
                record_at(replaceable(31), started_at + Duration::from_micros(2)),
                true,
            )
            .unwrap();
        assert!(matches!(
            queue
                .admit(
                    record_at(replaceable(32), started_at + Duration::from_micros(7)),
                    false,
                )
                .unwrap(),
            GatewayAdmission::Superseded { .. }
        ));
        let sampled_at = started_at + Duration::from_micros(11);
        assert_eq!(
            queue.stats_at(sampled_at).oldest_pending_age_micros,
            Some(11)
        );

        let dropped = GatewayCommand::Patch(patch(33, DeliverySemantic::BestEffort));
        assert!(matches!(
            queue
                .admit(
                    record_at(dropped, started_at + Duration::from_micros(9)),
                    false,
                )
                .unwrap(),
            GatewayAdmission::Dropped { .. }
        ));
        assert_eq!(
            queue.stats_at(sampled_at).oldest_pending_age_micros,
            Some(11)
        );

        let rejected = GatewayCommand::Patch(patch(34, DeliverySemantic::MustApply));
        assert!(matches!(
            queue.admit(
                record_at(rejected, started_at + Duration::from_micros(10)),
                true,
            ),
            Err(GatewayError::CommandCapacityExceeded { capacity: 2 })
        ));
        assert_eq!(
            queue.stats_at(sampled_at).oldest_pending_age_micros,
            Some(11)
        );

        queue.pop_front().unwrap();
        assert_eq!(
            queue.stats_at(sampled_at).oldest_pending_age_micros,
            Some(4)
        );
        queue.pop_front().unwrap();
        assert_eq!(queue.stats_at(sampled_at).oldest_pending_age_micros, None);
        assert_eq!(duration_micros(Duration::MAX), u64::MAX);
    }

    #[test]
    fn idempotency_conflicts_and_capacity_are_explicit() {
        let mut queue = CommandQueue::new(NonZeroU32::new(2).unwrap());
        let original = GatewayCommand::Patch(patch(10, DeliverySemantic::MustApply));
        let mut conflicting = original.clone();
        if let GatewayCommand::Patch(patch) = &mut conflicting {
            patch.transaction_id = TransactionId::new(999).unwrap();
        }
        queue.admit(record(original), true).unwrap();
        assert!(matches!(
            queue.admit(record(conflicting), true),
            Err(GatewayError::IdempotencyConflict { .. })
        ));
        assert!(matches!(
            queue.admit(
                record(GatewayCommand::Patch(patch(
                    11,
                    DeliverySemantic::MustApply,
                ))),
                false
            ),
            Err(GatewayError::IdempotencyCapacityExceeded)
        ));
    }

    #[test]
    fn command_fingerprints_obey_wire_bounds_and_queue_debug_redacts_payloads() {
        let limits = RuntimeLimits {
            max_encoded_bytes: core::num::NonZeroU64::new(1).unwrap(),
            ..RuntimeLimits::default()
        };
        assert!(matches!(
            CommandRecord::new(
                GatewayCommand::Patch(patch(20, DeliverySemantic::MustApply)),
                &limits,
            ),
            Err(GatewayError::InvalidCommandEncoding(
                CodecError::EncodedSizeExceeded { .. }
            ))
        ));

        let private_marker = SceneText::new("private-command-marker").unwrap();
        let mut queue = CommandQueue::new(NonZeroU32::new(1).unwrap());
        queue
            .admit(
                record(GatewayCommand::Patch(patch(
                    21,
                    DeliverySemantic::LatestWins {
                        supersession_key: private_marker.clone(),
                    },
                ))),
                true,
            )
            .unwrap();
        let debug = format!("{queue:?}");
        assert!(!debug.contains(private_marker.as_str()));
        assert!(debug.contains("depth: 1"));
    }

    #[test]
    fn exact_queries_are_sorted_filtered_and_fail_instead_of_truncating() {
        let mut world =
            cogniform_world::AuthoritativeWorld::new(cogniform_world::WorldConfig::default());
        let create = ScenePatch {
            schema_version: SchemaVersion::V1,
            transaction_id: TransactionId::new(1).unwrap(),
            idempotency_key: IdempotencyKey::new(2).unwrap(),
            base_revision: SceneRevision::INITIAL,
            conflict_policy: ConflictPolicy::RequireExactBase,
            delivery: DeliverySemantic::MustApply,
            declared_budget: PatchBudget::default(),
            operations: vec![cogniform_protocol::SceneOperation::Create(
                cogniform_protocol::CreateEntity {
                    entity_id: cogniform_protocol::StableEntityId::new(5).unwrap(),
                    components: vec![cogniform_protocol::ComponentValue::Name(
                        cogniform_protocol::NameComponent {
                            value: SceneText::new("five").unwrap(),
                        },
                    )],
                },
            )],
        };
        world
            .apply_patch(&create, FrameId::new(1).unwrap())
            .unwrap();
        let snapshot = world.snapshot().unwrap();
        let query = SceneQuery {
            schema_version: SchemaVersion::V1,
            scene_revision: snapshot.revision(),
            entity_ids: Vec::new(),
            component_kinds: vec![ComponentKind::Name],
            limit: NonZeroU32::new(1).unwrap(),
        };
        let result = execute_query(&snapshot, &query, &RuntimeLimits::default()).unwrap();
        assert_eq!(result.entities.len(), 1);
        assert_eq!(result.entities[0].components.len(), 1);

        let mut too_small = query;
        too_small.limit = NonZeroU32::new(1).unwrap();
        let second_patch = ScenePatch {
            schema_version: SchemaVersion::V1,
            transaction_id: TransactionId::new(3).unwrap(),
            idempotency_key: IdempotencyKey::new(4).unwrap(),
            base_revision: world.revision(),
            conflict_policy: ConflictPolicy::RequireExactBase,
            delivery: DeliverySemantic::MustApply,
            declared_budget: PatchBudget::default(),
            operations: vec![cogniform_protocol::SceneOperation::Create(
                cogniform_protocol::CreateEntity {
                    entity_id: cogniform_protocol::StableEntityId::new(6).unwrap(),
                    components: Vec::new(),
                },
            )],
        };
        world
            .apply_patch(&second_patch, FrameId::new(2).unwrap())
            .unwrap();
        let snapshot = world.snapshot().unwrap();
        too_small.scene_revision = snapshot.revision();
        assert!(matches!(
            execute_query(&snapshot, &too_small, &RuntimeLimits::default()),
            Err(GatewayError::QueryResultCapacityExceeded {
                actual: 2,
                limit: 1
            })
        ));
    }
}
