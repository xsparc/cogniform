use core::fmt;

use cogniform_protocol::ContentHash;

/// Admission, capacity, or record lookup failure for the bounded asset store.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AssetError {
    /// Source bytes exceed the per-asset input bound.
    SourceSizeExceeded {
        /// Supplied source byte count.
        actual: u64,
        /// Configured per-source byte limit.
        limit: u64,
    },
    /// The supplied content address does not match exact source bytes.
    ContentHashMismatch {
        /// Caller-supplied content identity.
        expected: ContentHash,
        /// Identity computed from the exact supplied bytes.
        actual: ContentHash,
    },
    /// The fixed retained-record capacity is exhausted.
    RecordCapacityExceeded {
        /// Configured retained-record capacity.
        capacity: u32,
    },
    /// The fixed pending-import count is exhausted.
    ImportCapacityExceeded {
        /// Configured pending-import capacity.
        capacity: u32,
    },
    /// Aggregate pending source bytes would exceed the configured bound.
    PendingSourceBytesExceeded {
        /// Projected pending source bytes.
        actual: u64,
        /// Configured aggregate pending-source limit.
        limit: u64,
    },
    /// No retained record exists for the requested content hash.
    AssetNotFound {
        /// Requested immutable content identity.
        content_hash: ContentHash,
    },
    /// The requested asset has no ready or proxy decoded mesh.
    AssetNotReady {
        /// Requested immutable content identity.
        content_hash: ContentHash,
    },
    /// The requested mesh index does not exist in the decoded asset.
    MeshNotFound {
        /// Requested immutable content identity.
        content_hash: ContentHash,
        /// Requested zero-based mesh index.
        mesh_index: u32,
    },
}

impl fmt::Display for AssetError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SourceSizeExceeded { actual, limit } => {
                write!(
                    formatter,
                    "asset source has {actual} bytes; limit is {limit}"
                )
            }
            Self::ContentHashMismatch { expected, actual } => {
                write!(
                    formatter,
                    "asset hash mismatch: expected {expected}, computed {actual}"
                )
            }
            Self::RecordCapacityExceeded { capacity } => {
                write!(formatter, "asset record capacity {capacity} is full")
            }
            Self::ImportCapacityExceeded { capacity } => {
                write!(formatter, "asset import capacity {capacity} is full")
            }
            Self::PendingSourceBytesExceeded { actual, limit } => write!(
                formatter,
                "pending asset sources require {actual} bytes; limit is {limit}"
            ),
            Self::AssetNotFound { content_hash } => {
                write!(formatter, "asset {content_hash} is not retained")
            }
            Self::AssetNotReady { content_hash } => {
                write!(formatter, "asset {content_hash} has no decoded mesh")
            }
            Self::MeshNotFound {
                content_hash,
                mesh_index,
            } => write!(formatter, "asset {content_hash} has no mesh {mesh_index}"),
        }
    }
}

impl std::error::Error for AssetError {}
