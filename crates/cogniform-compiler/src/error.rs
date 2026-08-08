use core::fmt;

use cogniform_compilation::CompilationValidationError;
use cogniform_protocol::{SceneRevision, ValidationError};

/// Deterministic compilation failure that prevents a normalized result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompileError {
    /// The imagination violates its public schema or configured bounds.
    InvalidImagination(ValidationError),
    /// The immutable scene view does not represent the requested base revision.
    SceneRevisionMismatch {
        /// Revision named by the imagination.
        requested: SceneRevision,
        /// Revision represented by the supplied view.
        actual: SceneRevision,
    },
    /// Collision-bounded deterministic entity-ID derivation was exhausted.
    EntityIdDerivationExhausted {
        /// Stable zero-based index in normalized entity-key order.
        entity_index: u32,
        /// Configured derivation-attempt limit.
        attempts: u32,
    },
    /// The normalized patch violated a downstream public patch invariant.
    InvalidNormalizedPatch(ValidationError),
    /// The completed report violated its versioned result contract.
    InvalidCompilationResult(CompilationValidationError),
}

impl fmt::Display for CompileError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidImagination(error) => write!(formatter, "invalid imagination: {error}"),
            Self::SceneRevisionMismatch { requested, actual } => write!(
                formatter,
                "imagination revision {} does not match scene view revision {}",
                requested.get(),
                actual.get()
            ),
            Self::EntityIdDerivationExhausted {
                entity_index,
                attempts,
            } => write!(
                formatter,
                "entity {entity_index} exhausted {attempts} deterministic identity attempts"
            ),
            Self::InvalidNormalizedPatch(error) => {
                write!(formatter, "invalid normalized patch: {error}")
            }
            Self::InvalidCompilationResult(error) => {
                write!(formatter, "invalid compilation result: {error}")
            }
        }
    }
}

impl std::error::Error for CompileError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::InvalidImagination(error) | Self::InvalidNormalizedPatch(error) => Some(error),
            Self::InvalidCompilationResult(error) => Some(error),
            Self::SceneRevisionMismatch { .. } | Self::EntityIdDerivationExhausted { .. } => None,
        }
    }
}
