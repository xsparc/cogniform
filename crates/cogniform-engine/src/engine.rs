use core::num::NonZeroU32;

use cogniform_assets::AssetUploadJob;
use cogniform_protocol::{
    ApplyReceipt, ApplyStatus, ContentHash, FrameId, RuntimeLimits, ScenePatch, SceneRevision,
};
use cogniform_renderer::{
    AssetUploadAdmission, AssetUploadOutcome, HeadlessRenderer, RendererAssetEviction,
    RendererAssetStats, RendererConfig,
};
use cogniform_replay::{
    RecordedApplyError, RecordedWorld, ReplayConfig, ReplayError, ReplayLog, ReplayVerification,
};
use cogniform_world::{AuthoritativeWorld, LogicalSceneHash, WorldConfig};

use crate::{EngineError, EngineRecoveryPoint, Observation, ObservationQueue, ObservationRequest};

/// Bounds and domain configuration for one local engine instance.
#[derive(Debug, Clone)]
pub struct EngineConfig {
    /// Authoritative world bounds.
    pub world: WorldConfig,
    /// Fixed-size renderer and readback bounds.
    pub renderer: RendererConfig,
    /// Bounded accepted-event replay log.
    pub replay: ReplayConfig,
    /// Total queued, active, or completed observations allowed at once.
    pub observation_capacity: NonZeroU32,
}

impl EngineConfig {
    /// Creates a bounded local configuration for one offscreen target size.
    #[must_use]
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            world: WorldConfig::default(),
            renderer: RendererConfig::new(width, height),
            replay: ReplayConfig::default(),
            observation_capacity: NonZeroU32::new(2).expect("constant is non-zero"),
        }
    }

    /// Sets the total outstanding observation capacity.
    #[must_use]
    pub const fn with_observation_capacity(mut self, capacity: NonZeroU32) -> Self {
        self.observation_capacity = capacity;
        self
    }
}

/// Local composition of one authoritative world, renderer, and observation path.
pub struct CogniformEngine {
    world: RecordedWorld,
    renderer: HeadlessRenderer,
    observations: ObservationQueue,
}

impl std::fmt::Debug for CogniformEngine {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("CogniformEngine")
            .field("revision", &self.world.world().revision())
            .field("renderer", &self.renderer)
            .field("observations", &self.observations)
            .finish_non_exhaustive()
    }
}

impl CogniformEngine {
    /// Initializes all bounded local domains without creating a window or service.
    pub async fn new(config: EngineConfig) -> Result<Self, EngineError> {
        validate_config(&config)?;
        let limits = config.world.runtime_limits;
        let world =
            RecordedWorld::new(config.world, config.replay).map_err(EngineError::ReplayConfig)?;
        let renderer = HeadlessRenderer::new(config.renderer).await?;
        let observations = ObservationQueue::new(config.observation_capacity, limits)?;
        Ok(Self {
            world,
            renderer,
            observations,
        })
    }

    /// Restores one complete verified recovery point into fresh bounded domains.
    ///
    /// Replay decoding, integrity verification, and authoritative-world replay
    /// complete before adapter selection or GPU initialization. Any invalid tail
    /// rejects the whole point; the verified prefix is never adopted here.
    pub async fn restore(
        config: EngineConfig,
        recovery: &EngineRecoveryPoint,
    ) -> Result<Self, EngineError> {
        let world = Self::prepare_restore(&config, recovery)?;
        Self::restore_prepared(config, recovery.next_frame_id(), world).await
    }

    pub(crate) fn prepare_restore(
        config: &EngineConfig,
        recovery: &EngineRecoveryPoint,
    ) -> Result<RecordedWorld, EngineError> {
        validate_config(config)?;
        let limits = config.world.runtime_limits;
        let loaded = ReplayLog::load_prefix(recovery.replay_bytes(), config.replay, &limits);
        let (log, tail) = loaded.into_parts();
        if let Some(error) = tail {
            return Err(EngineError::Replay(ReplayError::from(error)));
        }
        validate_recovery_frame(&log, recovery.next_frame_id())?;
        RecordedWorld::restore(config.world, log).map_err(EngineError::Replay)
    }

    pub(crate) async fn restore_prepared(
        config: EngineConfig,
        next_frame_id: FrameId,
        mut world: RecordedWorld,
    ) -> Result<Self, EngineError> {
        let limits = config.world.runtime_limits;
        let mut renderer =
            HeadlessRenderer::new_with_next_frame_id(config.renderer, next_frame_id).await?;
        if world.world().revision() != SceneRevision::INITIAL {
            let extraction = world.take_render_extraction()?;
            let summary = renderer.apply_extraction(&extraction)?;
            debug_assert_eq!(summary.scene_revision, world.world().revision());
        }
        let observations = ObservationQueue::new(config.observation_capacity, limits)?;
        Ok(Self {
            world,
            renderer,
            observations,
        })
    }

    /// Returns the current authoritative scene revision.
    #[must_use]
    pub const fn revision(&self) -> SceneRevision {
        self.world.world().revision()
    }

    /// Returns read-only access to the authoritative world contract.
    #[must_use]
    pub const fn world(&self) -> &AuthoritativeWorld {
        self.world.world()
    }

    /// Returns read-only access to renderer metadata and extracted-state counters.
    #[must_use]
    pub const fn renderer(&self) -> &HeadlessRenderer {
        &self.renderer
    }

    /// Reserves bounded renderer capacity for one immutable decoded asset mesh.
    ///
    /// The renderer remains the sole owner of its upload queue and GPU state;
    /// callers receive only typed admission and occupancy values.
    pub fn enqueue_asset_upload(
        &mut self,
        job: AssetUploadJob,
    ) -> Result<AssetUploadAdmission, EngineError> {
        self.renderer.enqueue_asset_upload(job).map_err(Into::into)
    }

    /// Processes at most one renderer-owned asset upload.
    pub fn process_next_asset_upload(&mut self) -> Option<AssetUploadOutcome> {
        self.renderer.process_next_asset_upload()
    }

    /// Explicitly releases all renderer-owned state for one content hash.
    pub fn evict_asset(&mut self, content_hash: ContentHash) -> RendererAssetEviction {
        self.renderer.evict_asset(content_hash)
    }

    /// Returns bounded renderer asset occupancy without exposing GPU handles.
    #[must_use]
    pub fn renderer_asset_stats(&self) -> RendererAssetStats {
        self.renderer.asset_stats()
    }

    /// Applies one atomic patch and immediately consumes its compact extraction.
    pub fn apply_patch(&mut self, patch: &ScenePatch) -> Result<ApplyReceipt, EngineError> {
        let estimated_visible_frame = self.renderer.next_frame_id()?;
        let receipt = match self.world.apply_patch(patch, estimated_visible_frame) {
            Ok(receipt) => receipt,
            Err(RecordedApplyError::World(error)) => return Err(EngineError::WorldApply(error)),
            Err(error) => return Err(EngineError::ReplayRecord(error)),
        };
        if receipt.status == ApplyStatus::IdempotentReplay {
            return Ok(receipt);
        }
        let extraction = self.world.take_render_extraction()?;
        let summary = self.renderer.apply_extraction(&extraction)?;
        debug_assert_eq!(summary.scene_revision, self.revision());
        debug_assert_eq!(self.renderer.scene_revision(), self.revision());
        Ok(receipt)
    }

    /// Returns the bounded append-only log of newly accepted patches.
    #[must_use]
    pub const fn replay_log(&self) -> &ReplayLog {
        self.world.log()
    }

    /// Returns an owned version-one encoding of the accepted-event log.
    #[must_use]
    pub fn replay_bytes(&self) -> Vec<u8> {
        self.world.log().to_bytes()
    }

    /// Captures complete replay bytes and the next unreserved frame identity.
    pub fn recovery_point(&self) -> Result<EngineRecoveryPoint, EngineError> {
        Ok(EngineRecoveryPoint::from_parts(
            self.replay_bytes(),
            self.renderer.next_frame_id()?,
        ))
    }

    /// Captures one exact historical replay prefix for a fresh-service fork.
    ///
    /// The point uses the source renderer's current next frame identity rather
    /// than reconstructing a historical renderer marker, so the fork cannot
    /// reuse a frame issued before capture. Concurrent branches must coordinate
    /// their independently advancing frame identities outside the engine.
    pub fn recovery_point_at_revision(
        &self,
        revision: SceneRevision,
    ) -> Result<EngineRecoveryPoint, EngineError> {
        let replay_bytes = self
            .world
            .log()
            .to_bytes_through_revision(revision)
            .map_err(EngineError::ReplayRevision)?;
        Ok(EngineRecoveryPoint::from_parts(
            replay_bytes,
            self.renderer.next_frame_id()?,
        ))
    }

    /// Verifies the complete accepted-event hash and revision chain.
    pub fn verify_replay(&self) -> Result<ReplayVerification, EngineError> {
        self.world
            .log()
            .verify()
            .map_err(|error| EngineError::Replay(ReplayError::Integrity(error)))
    }

    /// Returns the current authoritative logical scene hash.
    pub fn logical_hash(&self) -> Result<LogicalSceneHash, EngineError> {
        self.world
            .world()
            .logical_hash()
            .map_err(EngineError::WorldInvariant)
    }

    /// Replays every accepted event into a fresh world and returns its logical hash.
    pub fn replayed_logical_hash(&self) -> Result<LogicalSceneHash, EngineError> {
        self.world
            .replay()
            .map_err(EngineError::Replay)?
            .logical_hash()
            .map_err(EngineError::WorldInvariant)
    }

    /// Submits one extracted-scene observation without waiting for readback or consumers.
    pub fn request_observation(&mut self, request: ObservationRequest) -> Result<(), EngineError> {
        let permit = self.observations.try_reserve()?;
        let pending = self.renderer.submit_scene(request.camera_id)?;
        self.observations
            .submit_reserved(permit, pending, request)?;
        Ok(())
    }

    /// Polls one completed observation without waiting.
    pub fn try_receive_observation(&self) -> Result<Option<Observation>, EngineError> {
        self.observations
            .try_receive(self.revision())
            .map_err(EngineError::from)
    }

    /// Returns the fixed total outstanding observation capacity.
    #[must_use]
    pub fn observation_capacity(&self) -> u32 {
        self.observations.capacity()
    }

    /// Returns the number of queued, active, or completed observation requests.
    #[must_use]
    pub fn outstanding_observations(&self) -> u32 {
        self.observations.outstanding()
    }

    /// Returns the active bounded public protocol limits.
    #[must_use]
    pub const fn runtime_limits(&self) -> RuntimeLimits {
        self.world.world().runtime_limits()
    }

    /// Returns the accepted idempotency-result capacity shared with the gateway.
    #[must_use]
    pub const fn max_idempotency_records(&self) -> u32 {
        self.world.world().max_idempotency_records()
    }

    /// Returns the number of accepted world idempotency records already retained.
    #[must_use]
    pub fn idempotency_record_count(&self) -> u32 {
        u32::try_from(self.world.world().idempotency_record_count()).unwrap_or(u32::MAX)
    }
}

fn validate_recovery_frame(log: &ReplayLog, next_frame_id: FrameId) -> Result<(), EngineError> {
    let recorded_frame_id = log
        .entries()
        .iter()
        .map(cogniform_replay::ReplayEntry::estimated_visible_frame)
        .max();
    if let Some(recorded_frame_id) = recorded_frame_id
        && next_frame_id < recorded_frame_id
    {
        return Err(EngineError::RecoveryFrameBehindReplay {
            next_frame_id,
            recorded_frame_id,
        });
    }
    Ok(())
}

pub(crate) fn validate_config(config: &EngineConfig) -> Result<(), EngineError> {
    config
        .replay
        .validate()
        .map_err(EngineError::ReplayConfig)?;
    if config.world.max_entities.get() > config.renderer.max_scene_entities.get() {
        return Err(EngineError::InvalidConfig {
            reason: "renderer entity capacity must cover the maximum live world entity count",
        });
    }
    if config.observation_capacity.get() > config.renderer.readback_capacity.get() {
        return Err(EngineError::InvalidConfig {
            reason: "observation capacity cannot exceed the fixed readback pool capacity",
        });
    }
    if config.observation_capacity.get() > config.world.runtime_limits.max_queue_capacity.get() {
        return Err(EngineError::InvalidConfig {
            reason: "observation capacity exceeds the active protocol queue limit",
        });
    }
    if config.renderer.width > config.world.runtime_limits.max_observation_width.get()
        || config.renderer.height > config.world.runtime_limits.max_observation_height.get()
        || u64::from(config.renderer.width) * u64::from(config.renderer.height)
            > config.world.runtime_limits.max_observation_pixels.get()
    {
        return Err(EngineError::InvalidConfig {
            reason: "renderer target exceeds the active protocol observation limits",
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use cogniform_replay::ReplayTailErrorKind;

    use super::*;

    #[test]
    fn cross_domain_capacities_fail_before_renderer_initialization() {
        let mut config = EngineConfig::new(64, 64);
        config.replay.max_log_bytes = NonZeroU32::new(1).unwrap();
        assert!(matches!(
            validate_config(&config),
            Err(EngineError::ReplayConfig(_))
        ));

        let mut config = EngineConfig::new(64, 64);
        config.observation_capacity = NonZeroU32::new(3).unwrap();
        assert!(matches!(
            validate_config(&config),
            Err(EngineError::InvalidConfig { .. })
        ));

        let mut config = EngineConfig::new(64, 64);
        config.renderer.max_scene_entities = NonZeroU32::new(1).unwrap();
        assert!(matches!(
            validate_config(&config),
            Err(EngineError::InvalidConfig { .. })
        ));
    }

    #[test]
    fn invalid_recovery_stream_fails_before_renderer_initialization() {
        let recovery =
            EngineRecoveryPoint::from_parts(b"invalid".to_vec(), FrameId::new(1).unwrap());
        let error = pollster::block_on(CogniformEngine::restore(
            EngineConfig::new(64, 64),
            &recovery,
        ))
        .expect_err("invalid replay bytes must reject the whole recovery point");
        assert!(matches!(
            error,
            EngineError::Replay(ReplayError::Tail(error))
                if matches!(error.kind(), ReplayTailErrorKind::InvalidHeader)
        ));
    }
}
