use cogniform_assets::{
    AssetAdmission, AssetMeshKey, AssetProcessOutcome, AssetRecord, AssetStore, AssetStoreConfig,
    AssetStoreStats,
};
use cogniform_procedural::{ProcedureArtifact, ProcedureRequest, execute};
use cogniform_protocol::{
    ContentHash, FrameId, ImaginationEnvelope, ScenePatch, SceneQuery, SceneQueryResult,
    SceneRevision, StableEntityId,
};
use cogniform_renderer::{AssetUploadAdmission, AssetUploadOutcome, RendererAssetStats};
use cogniform_replay::ReplayVerification;
use cogniform_world::LogicalSceneHash;

use crate::{
    AdapterSummary, CogniformEngine, EngineConfig, EngineRecoveryPoint, GatewayAdmission,
    GatewayConfig, GatewayQueueStats, GatewayResponse, LocalGateway, LocalRevertError,
    LocalServiceError, Observation, ObservationRequest,
};
use crate::{
    engine::validate_config as validate_engine_config,
    gateway::validate_config as validate_gateway_config,
};

/// Complete bounded configuration for one local in-process service.
#[derive(Debug, Clone)]
pub struct LocalServiceConfig {
    /// Authoritative world, replay, renderer, and observation configuration.
    pub engine: EngineConfig,
    /// Command queue and result-retention configuration.
    pub gateway: GatewayConfig,
    /// Caller-driven verified source and decoded CPU mesh bounds.
    pub asset_store: AssetStoreConfig,
}

impl LocalServiceConfig {
    /// Creates a bounded service configuration for one fixed offscreen target.
    #[must_use]
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            engine: EngineConfig::new(width, height),
            gateway: GatewayConfig::default(),
            asset_store: AssetStoreConfig::default(),
        }
    }
}

/// Aggregate local service occupancy and causality state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LocalServiceStatus {
    /// Current authoritative world revision.
    pub scene_revision: SceneRevision,
    /// Latest revision fully consumed by the renderer.
    pub renderer_revision: SceneRevision,
    /// Bounded mutating-command queue counters.
    pub command_queue: GatewayQueueStats,
    /// Accepted command results retained for idempotent replay.
    pub completed_results: u32,
    /// Queued, active, or completed observations not yet delivered.
    pub outstanding_observations: u32,
    /// Fixed total observation capacity.
    pub observation_capacity: u32,
    /// Accepted entries in the bounded replay log.
    pub replay_entries: u32,
    /// Exact encoded replay bytes, including the stream header.
    pub replay_bytes: u64,
}

/// Aggregate CPU and renderer-owned asset occupancy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LocalAssetStatus {
    /// Retained source-record, import-queue, and decoded CPU mesh occupancy.
    pub store: AssetStoreStats,
    /// Renderer upload reservations and immutable GPU mesh residency.
    pub renderer: RendererAssetStats,
}

/// Deterministic built-in procedure output admitted as one ordinary patch.
#[derive(Debug, Clone, PartialEq)]
pub struct ProcedureSubmission {
    /// Existing gateway admission outcome for the generated patch.
    pub admission: GatewayAdmission,
    /// Stable identities in deterministic procedure emission order.
    pub entity_ids: Vec<StableEntityId>,
}

/// Explicit effects of one successful in-place historical revert.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LocalRevertReceipt {
    /// Source revision replaced by the operation.
    pub previous_revision: SceneRevision,
    /// Exact historical revision now authoritative.
    pub target_revision: SceneRevision,
    /// Replay entries removed from the live branch tail.
    pub removed_replay_entries: u64,
    /// First renderer frame identity available to the replacement.
    pub next_frame_id: FrameId,
    /// Gateway response-cache entries intentionally cleared.
    pub cleared_completed_results: u32,
    /// Service-owned asset records intentionally cleared.
    pub cleared_asset_records: u32,
    /// Service-owned decoded CPU asset bytes intentionally cleared.
    pub cleared_cpu_asset_bytes: u64,
    /// Renderer-owned resident asset meshes intentionally cleared.
    pub cleared_resident_asset_meshes: u32,
    /// Renderer-owned resident asset bytes intentionally cleared.
    pub cleared_gpu_asset_bytes: u64,
}

/// Local typed service over one gateway-owned engine and bounded asset store.
///
/// The service creates no socket, listener, filesystem persistence, or remote
/// transport. Callers drive command processing and observation polling through
/// bounded non-blocking methods.
pub struct LocalService {
    config: LocalServiceConfig,
    gateway: LocalGateway,
    assets: AssetStore,
}

impl std::fmt::Debug for LocalService {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("LocalService")
            .field("status", &self.status())
            .field("asset_status", &self.asset_status())
            .finish_non_exhaustive()
    }
}

impl LocalService {
    /// Initializes the bounded headless engine and local typed gateway.
    pub async fn new(config: LocalServiceConfig) -> Result<Self, LocalServiceError> {
        let retained_config = config.clone();
        let LocalServiceConfig {
            engine,
            gateway,
            asset_store,
        } = config;
        validate_engine_config(&engine)?;
        validate_gateway_config(
            gateway,
            &engine.world.runtime_limits,
            engine.world.max_idempotency_records.get(),
        )?;
        let engine = CogniformEngine::new(engine).await?;
        let gateway = LocalGateway::new(engine, gateway)?;
        Ok(Self {
            config: retained_config,
            gateway,
            assets: AssetStore::new(asset_store),
        })
    }

    /// Restores a fresh local service from one complete in-memory recovery point.
    ///
    /// Transient command/result and observation queues intentionally start
    /// empty. Replay validation and world reconstruction finish before GPU
    /// initialization, and any invalid tail rejects the complete point.
    pub async fn restore(
        config: LocalServiceConfig,
        recovery: &EngineRecoveryPoint,
    ) -> Result<Self, LocalServiceError> {
        let retained_config = config.clone();
        let LocalServiceConfig {
            engine,
            gateway,
            asset_store,
        } = config;
        validate_engine_config(&engine)?;
        validate_gateway_config(
            gateway,
            &engine.world.runtime_limits,
            engine.world.max_idempotency_records.get(),
        )?;
        let world = CogniformEngine::prepare_restore(&engine, recovery)?;
        let available_world_records = world.world().max_idempotency_records().saturating_sub(
            u32::try_from(world.world().idempotency_record_count()).unwrap_or(u32::MAX),
        );
        validate_gateway_config(
            gateway,
            &engine.world.runtime_limits,
            available_world_records,
        )?;
        let engine =
            CogniformEngine::restore_prepared(engine, recovery.next_frame_id(), world).await?;
        let gateway = LocalGateway::new(engine, gateway)?;
        Ok(Self {
            config: retained_config,
            gateway,
            assets: AssetStore::new(asset_store),
        })
    }

    /// Returns the backend-neutral selected adapter summary for diagnostics.
    #[must_use]
    pub fn adapter(&self) -> &AdapterSummary {
        self.gateway.engine().renderer().adapter()
    }

    /// Verifies exact source identity and admits bytes to the bounded import queue.
    ///
    /// Admission performs no decoding, renderer upload, world mutation, or
    /// external I/O. The source bytes remain service-owned until one explicit
    /// import step consumes them.
    pub fn enqueue_asset_source(
        &mut self,
        expected_hash: ContentHash,
        source: Vec<u8>,
    ) -> Result<AssetAdmission, LocalServiceError> {
        self.assets
            .enqueue(expected_hash, source)
            .map_err(Into::into)
    }

    /// Decodes at most one queued asset source into service-owned CPU meshes.
    pub fn process_next_asset_import(&mut self) -> Option<AssetProcessOutcome> {
        self.assets.process_next()
    }

    /// Returns one immutable retained lifecycle record without source bytes.
    #[must_use]
    pub fn asset_record(&self, content_hash: ContentHash) -> Option<&AssetRecord> {
        self.assets.record(content_hash)
    }

    /// Prepares one ready CPU mesh and reserves renderer upload capacity.
    ///
    /// The immutable upload value crosses the engine boundary, but neither the
    /// asset store nor renderer-owned GPU handles are exposed.
    pub fn enqueue_asset_upload(
        &mut self,
        key: AssetMeshKey,
    ) -> Result<AssetUploadAdmission, LocalServiceError> {
        let job = self.assets.upload_job(key)?;
        self.gateway
            .engine_mut()
            .enqueue_asset_upload(job)
            .map_err(Into::into)
    }

    /// Processes at most one renderer-owned asset upload.
    pub fn process_next_asset_upload(&mut self) -> Option<AssetUploadOutcome> {
        self.gateway.engine_mut().process_next_asset_upload()
    }

    /// Returns bounded asset occupancy without source bytes or backend handles.
    #[must_use]
    pub fn asset_status(&self) -> LocalAssetStatus {
        LocalAssetStatus {
            store: self.assets.stats(),
            renderer: self.gateway.engine().renderer_asset_stats(),
        }
    }

    /// Admits one validated explicit patch under its declared delivery semantic.
    pub fn submit_patch(
        &mut self,
        patch: ScenePatch,
    ) -> Result<GatewayAdmission, LocalServiceError> {
        self.gateway.submit_patch(patch).map_err(Into::into)
    }

    /// Admits one validated imagination under its declared delivery semantic.
    pub fn submit_imagination(
        &mut self,
        imagination: ImaginationEnvelope,
    ) -> Result<GatewayAdmission, LocalServiceError> {
        self.gateway
            .submit_imagination(imagination)
            .map_err(Into::into)
    }

    /// Executes one pure bounded built-in procedure and admits its ordinary patch.
    ///
    /// Procedure preparation is synchronous, deterministic, and has no ambient
    /// filesystem, network, clock, entropy, world, or renderer authority. The
    /// returned gateway admission preserves the patch's delivery and idempotency
    /// semantics. World mutation still occurs only through [`Self::process_next`].
    pub fn submit_procedure(
        &mut self,
        request: &ProcedureRequest,
    ) -> Result<ProcedureSubmission, LocalServiceError> {
        let limits = self.gateway.engine().runtime_limits();
        let ProcedureArtifact { patch, entity_ids } = execute(request, &limits)?;
        let admission = self.gateway.submit_patch(patch)?;
        Ok(ProcedureSubmission {
            admission,
            entity_ids,
        })
    }

    /// Processes at most one admitted mutating command.
    pub fn process_next(&mut self) -> Result<Option<GatewayResponse>, LocalServiceError> {
        self.gateway.process_next().map_err(Into::into)
    }

    /// Executes one exact-revision bounded logical query immediately.
    pub fn query(&self, query: &SceneQuery) -> Result<SceneQueryResult, LocalServiceError> {
        self.gateway.query(query).map_err(Into::into)
    }

    /// Submits one bounded revision-linked observation request.
    pub fn request_observation(
        &mut self,
        request: ObservationRequest,
    ) -> Result<(), LocalServiceError> {
        self.gateway
            .engine_mut()
            .request_observation(request)
            .map_err(Into::into)
    }

    /// Polls one completed observation without waiting.
    pub fn try_receive_observation(&self) -> Result<Option<Observation>, LocalServiceError> {
        self.gateway
            .engine()
            .try_receive_observation()
            .map_err(Into::into)
    }

    /// Returns current bounded occupancy and revision state.
    #[must_use]
    pub fn status(&self) -> LocalServiceStatus {
        let engine = self.gateway.engine();
        LocalServiceStatus {
            scene_revision: engine.revision(),
            renderer_revision: engine.renderer().scene_revision(),
            command_queue: self.gateway.queue_stats(),
            completed_results: self.gateway.completed_result_count(),
            outstanding_observations: engine.outstanding_observations(),
            observation_capacity: engine.observation_capacity(),
            replay_entries: u32::try_from(engine.replay_log().len()).unwrap_or(u32::MAX),
            replay_bytes: engine.replay_log().encoded_len(),
        }
    }

    /// Verifies the complete accepted-event revision and digest chain.
    pub fn verify_replay(&self) -> Result<ReplayVerification, LocalServiceError> {
        self.gateway.engine().verify_replay().map_err(Into::into)
    }

    /// Returns the current authoritative logical scene hash.
    pub fn logical_hash(&self) -> Result<LogicalSceneHash, LocalServiceError> {
        self.gateway.engine().logical_hash().map_err(Into::into)
    }

    /// Replays every accepted patch into a fresh world and returns its logical hash.
    pub fn replayed_logical_hash(&self) -> Result<LogicalSceneHash, LocalServiceError> {
        self.gateway
            .engine()
            .replayed_logical_hash()
            .map_err(Into::into)
    }

    /// Returns a complete owned version-one replay stream.
    #[must_use]
    pub fn replay_bytes(&self) -> Vec<u8> {
        self.gateway.engine().replay_bytes()
    }

    /// Captures complete accepted-event bytes and renderer frame continuity.
    pub fn recovery_point(&self) -> Result<EngineRecoveryPoint, LocalServiceError> {
        self.gateway.engine().recovery_point().map_err(Into::into)
    }

    /// Captures an exact historical replay prefix for a fresh-service fork.
    ///
    /// The source service is unchanged. The returned point carries its current
    /// next frame identity so a restored fork cannot reuse a frame issued
    /// before capture. Concurrent branches remain caller-coordinated.
    pub fn recovery_point_at_revision(
        &self,
        revision: SceneRevision,
    ) -> Result<EngineRecoveryPoint, LocalServiceError> {
        self.gateway
            .engine()
            .recovery_point_at_revision(revision)
            .map_err(Into::into)
    }

    /// Replaces this service with an exact older retained revision.
    ///
    /// The command, observation, import, and upload queues must be quiescent.
    /// A complete fresh replacement is restored under the retained config
    /// before assignment, so any validation, replay, adapter, or device failure
    /// leaves this service unchanged. Successful replacement intentionally
    /// clears gateway response caches and CPU/GPU asset residency; logical asset
    /// references remain in replay and require explicit rehydration.
    pub async fn revert_to_revision(
        &mut self,
        revision: SceneRevision,
    ) -> Result<LocalRevertReceipt, LocalServiceError> {
        let status = self.status();
        if revision == status.scene_revision {
            return Err(LocalRevertError::TargetIsCurrent { revision }.into());
        }
        let recovery = self.recovery_point_at_revision(revision)?;
        let assets = self.asset_status();
        if status.command_queue.depth != 0
            || status.outstanding_observations != 0
            || assets.store.pending_imports != 0
            || assets.renderer.pending_uploads != 0
        {
            return Err(LocalRevertError::NotQuiescent {
                command_depth: status.command_queue.depth,
                outstanding_observations: status.outstanding_observations,
                pending_asset_imports: assets.store.pending_imports,
                pending_asset_uploads: assets.renderer.pending_uploads,
            }
            .into());
        }

        let receipt = LocalRevertReceipt {
            previous_revision: status.scene_revision,
            target_revision: revision,
            removed_replay_entries: status.scene_revision.get().saturating_sub(revision.get()),
            next_frame_id: recovery.next_frame_id(),
            cleared_completed_results: status.completed_results,
            cleared_asset_records: assets.store.records,
            cleared_cpu_asset_bytes: assets.store.resident_cpu_bytes,
            cleared_resident_asset_meshes: assets.renderer.resident_meshes,
            cleared_gpu_asset_bytes: assets.renderer.resident_bytes,
        };
        let replacement = Self::restore(self.config.clone(), &recovery).await?;
        debug_assert_eq!(replacement.status().scene_revision, revision);
        debug_assert_eq!(replacement.status().renderer_revision, revision);
        *self = replacement;
        Ok(receipt)
    }
}

#[cfg(test)]
mod tests {
    use core::num::NonZeroU32;

    use super::*;
    use crate::GatewayError;

    #[test]
    fn invalid_gateway_capacity_fails_before_renderer_initialization() {
        let mut config = LocalServiceConfig::new(64, 64);
        config.gateway.command_capacity = NonZeroU32::new(2).unwrap();
        config.gateway.idempotency_capacity = NonZeroU32::new(1).unwrap();

        assert!(matches!(
            pollster::block_on(LocalService::new(config)),
            Err(LocalServiceError::Gateway(error))
                if matches!(error.as_ref(), GatewayError::InvalidConfig { .. })
        ));
    }
}
