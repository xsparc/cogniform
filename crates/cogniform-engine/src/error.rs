use core::fmt;

use cogniform_compiler::CompileError;
use cogniform_protocol::{
    CodecError, IdempotencyKey, SceneRevision, StableEntityId, ValidationError,
};
use cogniform_renderer::{RendererError, SceneUpdateError};
use cogniform_world::{WorldApplyError, WorldExtractionError, WorldInvariantError};

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

/// Bounded local gateway admission, compilation, query, or apply failure.
#[derive(Debug)]
pub enum GatewayError {
    /// Gateway capacities are incompatible with engine/runtime bounds.
    InvalidConfig {
        /// Stable configuration reason.
        reason: &'static str,
    },
    /// A command or query violates its public schema or configured limits.
    InvalidCommand(ValidationError),
    /// A validated typed command cannot be represented within canonical wire bounds.
    InvalidCommandEncoding(CodecError),
    /// The fixed uncommitted command queue is full.
    CommandCapacityExceeded {
        /// Maximum uncommitted command count.
        capacity: u32,
    },
    /// The bounded queued-plus-completed idempotency capacity is exhausted.
    IdempotencyCapacityExceeded,
    /// One idempotency key was reused for a different typed command.
    IdempotencyConflict {
        /// Conflicting key, retained as a typed value rather than formatted input.
        idempotency_key: IdempotencyKey,
    },
    /// Deterministic semantic compilation failed before world mutation.
    Compiler(CompileError),
    /// Engine composition or world application failed.
    Engine(Box<EngineError>),
    /// A read-only world snapshot could not be produced.
    WorldView(WorldInvariantError),
    /// A query requested a revision other than the current immutable view.
    QueryRevisionMismatch {
        /// Revision named by the query.
        requested: SceneRevision,
        /// Revision represented by the world snapshot.
        actual: SceneRevision,
    },
    /// A complete query result would exceed the caller's explicit limit.
    QueryResultCapacityExceeded {
        /// Matching entity count.
        actual: u32,
        /// Maximum result count declared by the query.
        limit: u32,
    },
    /// A produced query result violated canonical protocol invariants.
    InvalidQueryResult(ValidationError),
}

impl fmt::Display for GatewayError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidConfig { reason } => write!(formatter, "invalid gateway config: {reason}"),
            Self::InvalidCommand(error) => write!(formatter, "invalid gateway command: {error}"),
            Self::InvalidCommandEncoding(error) => {
                write!(formatter, "invalid gateway command encoding: {error}")
            }
            Self::CommandCapacityExceeded { capacity } => {
                write!(formatter, "gateway command capacity {capacity} is full")
            }
            Self::IdempotencyCapacityExceeded => {
                formatter.write_str("gateway idempotency capacity is full")
            }
            Self::IdempotencyConflict { .. } => {
                formatter.write_str("idempotency key is already bound to another command")
            }
            Self::Compiler(error) => write!(formatter, "imagination compiler failed: {error}"),
            Self::Engine(error) => write!(formatter, "gateway engine operation failed: {error}"),
            Self::WorldView(error) => write!(formatter, "gateway scene view failed: {error}"),
            Self::QueryRevisionMismatch { requested, actual } => write!(
                formatter,
                "query revision {} does not match current revision {}",
                requested.get(),
                actual.get()
            ),
            Self::QueryResultCapacityExceeded { actual, limit } => write!(
                formatter,
                "query matches {actual} entities; result limit is {limit}"
            ),
            Self::InvalidQueryResult(error) => {
                write!(formatter, "invalid gateway query result: {error}")
            }
        }
    }
}

impl std::error::Error for GatewayError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::InvalidCommand(error) | Self::InvalidQueryResult(error) => Some(error),
            Self::InvalidCommandEncoding(error) => Some(error),
            Self::Compiler(error) => Some(error),
            Self::Engine(error) => Some(error),
            Self::WorldView(error) => Some(error),
            Self::InvalidConfig { .. }
            | Self::CommandCapacityExceeded { .. }
            | Self::IdempotencyCapacityExceeded
            | Self::IdempotencyConflict { .. }
            | Self::QueryRevisionMismatch { .. }
            | Self::QueryResultCapacityExceeded { .. } => None,
        }
    }
}

impl From<CompileError> for GatewayError {
    fn from(value: CompileError) -> Self {
        Self::Compiler(value)
    }
}

impl From<EngineError> for GatewayError {
    fn from(value: EngineError) -> Self {
        Self::Engine(Box::new(value))
    }
}

impl From<WorldInvariantError> for GatewayError {
    fn from(value: WorldInvariantError) -> Self {
        Self::WorldView(value)
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
