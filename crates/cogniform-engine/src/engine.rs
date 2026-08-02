use core::num::NonZeroU32;

use cogniform_protocol::{ApplyReceipt, ApplyStatus, RuntimeLimits, ScenePatch, SceneRevision};
use cogniform_renderer::{HeadlessRenderer, RendererConfig};
use cogniform_world::{AuthoritativeWorld, WorldConfig};

use crate::{EngineError, Observation, ObservationQueue, ObservationRequest};

/// Bounds and domain configuration for one local engine instance.
#[derive(Debug, Clone)]
pub struct EngineConfig {
    /// Authoritative world bounds.
    pub world: WorldConfig,
    /// Fixed-size renderer and readback bounds.
    pub renderer: RendererConfig,
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
    world: AuthoritativeWorld,
    renderer: HeadlessRenderer,
    observations: ObservationQueue,
}

impl std::fmt::Debug for CogniformEngine {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("CogniformEngine")
            .field("revision", &self.world.revision())
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
        let world = AuthoritativeWorld::new(config.world);
        let renderer = HeadlessRenderer::new(config.renderer).await?;
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
        self.world.revision()
    }

    /// Returns read-only access to the authoritative world contract.
    #[must_use]
    pub const fn world(&self) -> &AuthoritativeWorld {
        &self.world
    }

    /// Returns read-only access to renderer metadata and extracted-state counters.
    #[must_use]
    pub const fn renderer(&self) -> &HeadlessRenderer {
        &self.renderer
    }

    /// Applies one atomic patch and immediately consumes its compact extraction.
    pub fn apply_patch(&mut self, patch: &ScenePatch) -> Result<ApplyReceipt, EngineError> {
        let estimated_visible_frame = self.renderer.next_frame_id()?;
        let receipt = self.world.apply_patch(patch, estimated_visible_frame)?;
        if receipt.status == ApplyStatus::IdempotentReplay {
            return Ok(receipt);
        }
        let extraction = self.world.take_render_extraction()?;
        let summary = self.renderer.apply_extraction(&extraction)?;
        debug_assert_eq!(summary.scene_revision, self.world.revision());
        debug_assert_eq!(self.renderer.scene_revision(), self.world.revision());
        Ok(receipt)
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
            .try_receive(self.world.revision())
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
        self.world.runtime_limits()
    }

    /// Returns the accepted idempotency-result capacity shared with the gateway.
    #[must_use]
    pub const fn max_idempotency_records(&self) -> u32 {
        self.world.max_idempotency_records()
    }

    /// Returns the number of accepted world idempotency records already retained.
    #[must_use]
    pub fn idempotency_record_count(&self) -> u32 {
        u32::try_from(self.world.idempotency_record_count()).unwrap_or(u32::MAX)
    }
}

fn validate_config(config: &EngineConfig) -> Result<(), EngineError> {
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
    use super::*;

    #[test]
    fn cross_domain_capacities_fail_before_renderer_initialization() {
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
}
