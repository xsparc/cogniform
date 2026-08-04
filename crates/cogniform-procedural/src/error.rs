use core::fmt;

use cogniform_protocol::ValidationError;

/// Bounded deterministic procedure failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProcedureError {
    /// Delivery text or its declared budget exceeds the active text bound.
    TextCapacityExceeded {
        /// Exact delivery text bytes supplied by the request.
        actual: u64,
        /// Sender-declared text-byte bound.
        declared: u64,
        /// Active runtime text-byte bound.
        runtime: u64,
    },
    /// Decoded patch size or its declared budget exceeds the active bound.
    DecodedCapacityExceeded {
        /// Exact logical decoded bytes for the generated patch.
        actual: u64,
        /// Sender-declared decoded-byte bound.
        declared: u64,
        /// Active runtime decoded-byte bound.
        runtime: u64,
    },
    /// The requested output entity count exceeds a declared or configured bound.
    EntityLimitExceeded {
        /// Requested entity count.
        actual: u64,
        /// Effective procedure limit.
        limit: u32,
    },
    /// The requested output would exceed patch operation or component bounds.
    PatchCapacityExceeded {
        /// Required operation count.
        operations: u64,
        /// Required component count.
        components: u64,
    },
    /// A generated transform could not be represented as finite protocol data.
    NonFiniteTransform {
        /// Deterministic row-major entity index.
        entity_index: u32,
    },
    /// Collision-bounded stable identity derivation was exhausted.
    EntityIdDerivationExhausted {
        /// Deterministic row-major entity index.
        entity_index: u32,
        /// Configured collision-resolution attempts.
        attempts: u32,
    },
    /// The resulting ordinary scene patch violated a public protocol invariant.
    InvalidPatch(ValidationError),
}

impl fmt::Display for ProcedureError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TextCapacityExceeded {
                actual,
                declared,
                runtime,
            } => write!(
                formatter,
                "procedure delivery has {actual} text bytes; declared limit is {declared} and runtime limit is {runtime}"
            ),
            Self::DecodedCapacityExceeded {
                actual,
                declared,
                runtime,
            } => write!(
                formatter,
                "procedure patch has {actual} decoded bytes; declared limit is {declared} and runtime limit is {runtime}"
            ),
            Self::EntityLimitExceeded { actual, limit } => {
                write!(
                    formatter,
                    "procedure requests {actual} entities; limit is {limit}"
                )
            }
            Self::PatchCapacityExceeded {
                operations,
                components,
            } => write!(
                formatter,
                "procedure requires {operations} operations and {components} components"
            ),
            Self::NonFiniteTransform { entity_index } => {
                write!(
                    formatter,
                    "procedure entity {entity_index} has a non-finite transform"
                )
            }
            Self::EntityIdDerivationExhausted {
                entity_index,
                attempts,
            } => write!(
                formatter,
                "procedure entity {entity_index} exhausted {attempts} identity attempts"
            ),
            Self::InvalidPatch(error) => {
                write!(formatter, "procedure produced an invalid patch: {error}")
            }
        }
    }
}

impl std::error::Error for ProcedureError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::InvalidPatch(error) => Some(error),
            Self::TextCapacityExceeded { .. }
            | Self::DecodedCapacityExceeded { .. }
            | Self::EntityLimitExceeded { .. }
            | Self::PatchCapacityExceeded { .. }
            | Self::NonFiniteTransform { .. }
            | Self::EntityIdDerivationExhausted { .. } => None,
        }
    }
}
