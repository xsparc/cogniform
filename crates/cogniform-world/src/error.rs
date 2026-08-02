use core::fmt;

use cogniform_protocol::{
    ComponentKind, IdempotencyKey, SceneRevision, StableEntityId, TransactionId, ValidationError,
};

/// A rejected patch that left the authoritative world unchanged.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum WorldApplyError {
    /// The patch failed protocol-level validation.
    InvalidPatch(ValidationError),
    /// The request was prepared against a revision other than the current one.
    BaseRevisionMismatch {
        /// Current authoritative revision.
        current: SceneRevision,
        /// Revision supplied by the patch.
        supplied: SceneRevision,
    },
    /// The current revision cannot be incremented.
    RevisionOverflow,
    /// An accepted idempotency key was reused with another transaction.
    IdempotencyKeyConflict {
        /// Reused idempotency key.
        idempotency_key: IdempotencyKey,
        /// Transaction recorded for the key.
        recorded_transaction: TransactionId,
        /// Transaction supplied by the patch.
        supplied_transaction: TransactionId,
    },
    /// The bounded idempotency record store cannot admit another key.
    IdempotencyCapacityExceeded {
        /// Configured maximum record count.
        limit: u32,
    },
    /// Applying a create operation would exceed the entity capacity.
    EntityCapacityExceeded {
        /// Zero-based operation index.
        operation_index: u32,
        /// Entity named by the operation.
        entity_id: StableEntityId,
        /// Configured maximum live-entity count.
        limit: u32,
    },
    /// A create operation names an entity that is already live.
    EntityAlreadyExists {
        /// Zero-based operation index.
        operation_index: u32,
        /// Entity named by the operation.
        entity_id: StableEntityId,
    },
    /// An operation names an entity that is not live at that point in the patch.
    EntityNotFound {
        /// Zero-based operation index.
        operation_index: u32,
        /// Entity named by the operation.
        entity_id: StableEntityId,
    },
    /// A remove operation names a component absent at that point in the patch.
    ComponentNotFound {
        /// Zero-based operation index.
        operation_index: u32,
        /// Entity named by the operation.
        entity_id: StableEntityId,
        /// Missing component kind.
        component: ComponentKind,
    },
    /// The operation belongs to a later approved world slice.
    UnsupportedOperation {
        /// Zero-based operation index.
        operation_index: u32,
        /// Entity named by the operation.
        entity_id: StableEntityId,
    },
    /// Private ECS state no longer agrees with the stable-ID index.
    InvariantViolation(WorldInvariantError),
}

impl fmt::Display for WorldApplyError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidPatch(error) => write!(formatter, "invalid scene patch: {error}"),
            Self::BaseRevisionMismatch { current, supplied } => write!(
                formatter,
                "base revision mismatch: current {}, supplied {}",
                current.get(),
                supplied.get()
            ),
            Self::RevisionOverflow => formatter.write_str("scene revision overflow"),
            Self::IdempotencyKeyConflict { .. } => {
                formatter.write_str("idempotency key belongs to another transaction")
            }
            Self::IdempotencyCapacityExceeded { limit } => {
                write!(formatter, "idempotency record capacity {limit} is full")
            }
            Self::EntityCapacityExceeded { limit, .. } => {
                write!(formatter, "live entity capacity {limit} would be exceeded")
            }
            Self::EntityAlreadyExists { .. } => {
                formatter.write_str("entity already exists at this operation")
            }
            Self::EntityNotFound { .. } => {
                formatter.write_str("entity does not exist at this operation")
            }
            Self::ComponentNotFound { .. } => {
                formatter.write_str("component does not exist at this operation")
            }
            Self::UnsupportedOperation { .. } => {
                formatter.write_str("operation is not supported by this world version")
            }
            Self::InvariantViolation(error) => {
                write!(formatter, "world invariant failure: {error}")
            }
        }
    }
}

impl std::error::Error for WorldApplyError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::InvalidPatch(error) => Some(error),
            Self::InvariantViolation(error) => Some(error),
            _ => None,
        }
    }
}

/// Stable classification for a private ECS/index consistency failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum WorldInvariantErrorKind {
    /// ECS and stable-index entity counts differ.
    EntityCountMismatch,
    /// An indexed ECS handle is no longer live.
    MissingStorageEntity,
    /// An ECS entity's private stable-ID marker differs from its index key.
    StableIdMismatch,
}

/// Reports a private ECS/index consistency failure without exposing an ECS handle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorldInvariantError {
    kind: WorldInvariantErrorKind,
    entity_id: Option<StableEntityId>,
}

impl WorldInvariantError {
    pub(crate) const fn new(
        kind: WorldInvariantErrorKind,
        entity_id: Option<StableEntityId>,
    ) -> Self {
        Self { kind, entity_id }
    }

    /// Returns the stable failure classification.
    #[must_use]
    pub const fn kind(&self) -> WorldInvariantErrorKind {
        self.kind
    }

    /// Returns the affected stable entity when one is known.
    #[must_use]
    pub const fn entity_id(&self) -> Option<StableEntityId> {
        self.entity_id
    }
}

impl fmt::Display for WorldInvariantError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.kind {
            WorldInvariantErrorKind::EntityCountMismatch => {
                formatter.write_str("entity count mismatch")
            }
            WorldInvariantErrorKind::MissingStorageEntity => {
                formatter.write_str("indexed storage entity is missing")
            }
            WorldInvariantErrorKind::StableIdMismatch => {
                formatter.write_str("stable-ID marker mismatch")
            }
        }
    }
}

impl std::error::Error for WorldInvariantError {}
