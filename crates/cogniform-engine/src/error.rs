use core::fmt;

use cogniform_protocol::{SceneRevision, StableEntityId, ValidationError};
use cogniform_renderer::{RendererError, SceneUpdateError};
use cogniform_world::{WorldApplyError, WorldExtractionError};

/// Composition failure that preserves the responsible domain boundary.
#[derive(Debug)]
pub enum EngineError {
    /// Engine capacities were inconsistent across bounded domains.
    InvalidConfig {
        /// Stable configuration reason.
        reason: &'static str,
    },
    /// The authoritative world rejected a transaction before mutation.
    WorldApply(WorldApplyError),
    /// The world could not produce an immutable extraction packet.
    WorldExtraction(WorldExtractionError),
    /// The renderer rejected an extraction without partial state mutation.
    SceneUpdate(SceneUpdateError),
    /// Renderer initialization or frame submission failed.
    Renderer(RendererError),
    /// The observation worker could not be initialized.
    Observation(ObservationError),
}

impl fmt::Display for EngineError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidConfig { reason } => write!(formatter, "invalid engine config: {reason}"),
            Self::WorldApply(error) => write!(formatter, "world apply failed: {error}"),
            Self::WorldExtraction(error) => write!(formatter, "world extraction failed: {error}"),
            Self::SceneUpdate(error) => write!(formatter, "renderer state update failed: {error}"),
            Self::Renderer(error) => write!(formatter, "renderer failed: {error}"),
            Self::Observation(error) => write!(formatter, "observation path failed: {error}"),
        }
    }
}

impl std::error::Error for EngineError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::InvalidConfig { .. } => None,
            Self::WorldApply(error) => Some(error),
            Self::WorldExtraction(error) => Some(error),
            Self::SceneUpdate(error) => Some(error),
            Self::Renderer(error) => Some(error),
            Self::Observation(error) => Some(error),
        }
    }
}

impl From<WorldApplyError> for EngineError {
    fn from(value: WorldApplyError) -> Self {
        Self::WorldApply(value)
    }
}

impl From<WorldExtractionError> for EngineError {
    fn from(value: WorldExtractionError) -> Self {
        Self::WorldExtraction(value)
    }
}

impl From<SceneUpdateError> for EngineError {
    fn from(value: SceneUpdateError) -> Self {
        Self::SceneUpdate(value)
    }
}

impl From<RendererError> for EngineError {
    fn from(value: RendererError) -> Self {
        Self::Renderer(value)
    }
}

impl From<ObservationError> for EngineError {
    fn from(value: ObservationError) -> Self {
        Self::Observation(value)
    }
}

/// Bounded observation admission, completion, or causality failure.
#[derive(Debug)]
pub enum ObservationError {
    /// The configured number of outstanding requests is already admitted.
    CapacityExceeded {
        /// Maximum number of outstanding requests.
        capacity: u32,
    },
    /// The worker or one of its bounded channels is unavailable.
    WorkerUnavailable,
    /// The worker thread could not be created.
    WorkerStartFailed {
        /// Operating-system diagnostic.
        reason: String,
    },
    /// GPU completion or readback failed asynchronously.
    Renderer(RendererError),
    /// The supplied latest revision was older than the frame's source revision.
    SourceRevisionAhead {
        /// Frame source revision.
        source: SceneRevision,
        /// Latest world revision supplied by the engine.
        latest: SceneRevision,
    },
    /// The request camera disagreed with the submitted frame source.
    CameraMismatch {
        /// Camera named by the request.
        requested: StableEntityId,
        /// Camera recorded by the frame.
        rendered: StableEntityId,
    },
    /// Produced causal metadata failed the public protocol contract.
    InvalidMetadata(ValidationError),
}

impl fmt::Display for ObservationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CapacityExceeded { capacity } => {
                write!(formatter, "observation capacity {capacity} is full")
            }
            Self::WorkerUnavailable => formatter.write_str("observation worker is unavailable"),
            Self::WorkerStartFailed { reason } => {
                write!(formatter, "observation worker could not start: {reason}")
            }
            Self::Renderer(error) => write!(formatter, "observation readback failed: {error}"),
            Self::SourceRevisionAhead { source, latest } => write!(
                formatter,
                "frame source revision {} is newer than latest world revision {}",
                source.get(),
                latest.get()
            ),
            Self::CameraMismatch {
                requested,
                rendered,
            } => write!(
                formatter,
                "observation requested camera {requested}, frame used {rendered}"
            ),
            Self::InvalidMetadata(error) => {
                write!(formatter, "invalid observation metadata: {error}")
            }
        }
    }
}

impl std::error::Error for ObservationError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Renderer(error) => Some(error),
            Self::InvalidMetadata(error) => Some(error),
            Self::CapacityExceeded { .. }
            | Self::WorkerUnavailable
            | Self::WorkerStartFailed { .. }
            | Self::SourceRevisionAhead { .. }
            | Self::CameraMismatch { .. } => None,
        }
    }
}
