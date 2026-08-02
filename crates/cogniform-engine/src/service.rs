use cogniform_protocol::{
    ImaginationEnvelope, ScenePatch, SceneQuery, SceneQueryResult, SceneRevision,
};
use cogniform_replay::ReplayVerification;
use cogniform_world::LogicalSceneHash;

use crate::{
    CogniformEngine, EngineConfig, GatewayAdmission, GatewayConfig, GatewayQueueStats,
    GatewayResponse, LocalGateway, LocalServiceError, Observation, ObservationRequest,
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
}

impl LocalServiceConfig {
    /// Creates a bounded service configuration for one fixed offscreen target.
    #[must_use]
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            engine: EngineConfig::new(width, height),
            gateway: GatewayConfig::default(),
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

/// Local typed service over one gateway-owned engine.
///
/// The service creates no socket, listener, filesystem persistence, or remote
/// transport. Callers drive command processing and observation polling through
/// bounded non-blocking methods.
pub struct LocalService {
    gateway: LocalGateway,
}

impl std::fmt::Debug for LocalService {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("LocalService")
            .field("status", &self.status())
            .finish_non_exhaustive()
    }
}

impl LocalService {
    /// Initializes the bounded headless engine and local typed gateway.
    pub async fn new(config: LocalServiceConfig) -> Result<Self, LocalServiceError> {
        let LocalServiceConfig { engine, gateway } = config;
        validate_engine_config(&engine)?;
        validate_gateway_config(
            gateway,
            &engine.world.runtime_limits,
            engine.world.max_idempotency_records.get(),
        )?;
        let engine = CogniformEngine::new(engine).await?;
        let gateway = LocalGateway::new(engine, gateway)?;
        Ok(Self { gateway })
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
