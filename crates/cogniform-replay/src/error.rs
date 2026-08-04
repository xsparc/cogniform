use core::fmt;

use cogniform_protocol::CodecError;
use cogniform_world::{WorldApplyError, WorldInvariantError};

use crate::ReplayConfigError;

/// Stable classification for an integrity failure in a complete replay log.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ReplayIntegrityErrorKind {
    /// Entry sequences are not contiguous and one-based.
    SequenceGap,
    /// Entry revisions do not form a contiguous chain.
    RevisionGap,
    /// The stored predecessor digest does not name the preceding entry.
    PreviousEntryHashMismatch,
    /// The stored predecessor scene hash does not match the preceding result.
    PreviousSceneHashMismatch,
    /// The stored entry digest differs from its canonical contents.
    EntryHashMismatch,
}

/// Identifies the first invalid entry in a complete replay log.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReplayIntegrityError {
    entry_index: u32,
    kind: ReplayIntegrityErrorKind,
}

impl ReplayIntegrityError {
    pub(crate) const fn new(entry_index: u32, kind: ReplayIntegrityErrorKind) -> Self {
        Self { entry_index, kind }
    }

    /// Returns the zero-based invalid entry index.
    #[must_use]
    pub const fn entry_index(self) -> u32 {
        self.entry_index
    }

    /// Returns the stable integrity classification.
    #[must_use]
    pub const fn kind(self) -> ReplayIntegrityErrorKind {
        self.kind
    }
}

impl fmt::Display for ReplayIntegrityError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "replay entry {} failed integrity: {:?}",
            self.entry_index, self.kind
        )
    }
}

impl std::error::Error for ReplayIntegrityError {}

/// Stable classification for an invalid or incomplete encoded replay tail.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ReplayTailErrorKind {
    /// Replay bounds cannot represent the mandatory stream header.
    InvalidConfig(ReplayConfigError),
    /// The stream does not begin with the supported replay header.
    InvalidHeader,
    /// The stream ended inside an entry frame.
    Truncated,
    /// The encoded stream exceeds its configured total byte bound.
    LogSizeExceeded,
    /// One frame exceeds its configured byte bound.
    EntrySizeExceeded,
    /// The stream contains more entries than its configured bound.
    EntryCapacityExceeded,
    /// Fixed-width lengths or fields disagree.
    MalformedEntry,
    /// The embedded patch failed bounded protocol decoding.
    InvalidPatch(CodecError),
    /// Embedded JSON differs from the protocol's canonical representation.
    NonCanonicalPatch,
    /// The decoded entry failed its chain or digest check.
    Integrity(ReplayIntegrityErrorKind),
}

/// Describes the first invalid byte after a verified replay prefix.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplayTailError {
    verified_entries: u32,
    offset: u64,
    kind: ReplayTailErrorKind,
}

impl ReplayTailError {
    pub(crate) const fn new(verified_entries: u32, offset: u64, kind: ReplayTailErrorKind) -> Self {
        Self {
            verified_entries,
            offset,
            kind,
        }
    }

    /// Returns the number of complete verified entries before the tail.
    #[must_use]
    pub const fn verified_entries(&self) -> u32 {
        self.verified_entries
    }

    /// Returns the zero-based byte offset at which verification stopped.
    #[must_use]
    pub const fn offset(&self) -> u64 {
        self.offset
    }

    /// Returns the stable tail classification.
    #[must_use]
    pub const fn kind(&self) -> &ReplayTailErrorKind {
        &self.kind
    }
}

impl fmt::Display for ReplayTailError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "replay verification stopped at byte {} after {} entries: {:?}",
            self.offset, self.verified_entries, self.kind
        )
    }
}

impl std::error::Error for ReplayTailError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match &self.kind {
            ReplayTailErrorKind::InvalidPatch(error) => Some(error),
            ReplayTailErrorKind::InvalidConfig(error) => Some(error),
            _ => None,
        }
    }
}

/// Failure to atomically admit and record one patch.
#[derive(Debug)]
#[non_exhaustive]
pub enum RecordedApplyError {
    /// Protocol canonical encoding failed before world mutation.
    Codec(CodecError),
    /// The bounded log already contains its maximum entry count.
    EntryCapacityExceeded {
        /// Configured entry count limit.
        limit: u32,
    },
    /// The encoded entry would exceed its per-entry byte bound.
    EntrySizeExceeded {
        /// Required encoded bytes, including the frame length.
        actual: u64,
        /// Configured per-entry byte limit.
        limit: u32,
    },
    /// Appending the entry would exceed the total log byte bound.
    LogSizeExceeded {
        /// Required total encoded log bytes.
        actual: u64,
        /// Configured total log byte limit.
        limit: u32,
    },
    /// The authoritative world rejected the patch without mutation.
    World(WorldApplyError),
    /// Logical state could not be read because a private invariant failed.
    Invariant(WorldInvariantError),
}

impl fmt::Display for RecordedApplyError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Codec(error) => write!(formatter, "canonical patch encoding failed: {error}"),
            Self::EntryCapacityExceeded { limit } => {
                write!(formatter, "replay entry capacity {limit} is full")
            }
            Self::EntrySizeExceeded { actual, limit } => {
                write!(
                    formatter,
                    "replay entry needs {actual} bytes; limit is {limit}"
                )
            }
            Self::LogSizeExceeded { actual, limit } => {
                write!(
                    formatter,
                    "replay log needs {actual} bytes; limit is {limit}"
                )
            }
            Self::World(error) => write!(formatter, "world rejected recorded patch: {error}"),
            Self::Invariant(error) => write!(formatter, "world invariant failure: {error}"),
        }
    }
}

impl std::error::Error for RecordedApplyError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Codec(error) => Some(error),
            Self::World(error) => Some(error),
            Self::Invariant(error) => Some(error),
            _ => None,
        }
    }
}

/// Failure while verifying or applying an already-recorded log.
#[derive(Debug)]
#[non_exhaustive]
pub enum ReplayError {
    /// An encoded replay stream contained an invalid or incomplete tail.
    Tail(ReplayTailError),
    /// The complete in-memory log failed chain verification.
    Integrity(ReplayIntegrityError),
    /// A replayed patch was rejected at the named zero-based entry.
    World {
        /// Zero-based entry index.
        entry_index: u32,
        /// World rejection.
        source: WorldApplyError,
    },
    /// Logical state could not be read during replay.
    Invariant {
        /// Zero-based entry index.
        entry_index: u32,
        /// Private invariant failure.
        source: WorldInvariantError,
    },
    /// A recorded revision disagrees with replayed causality.
    RevisionMismatch {
        /// Zero-based entry index.
        entry_index: u32,
    },
    /// A recorded logical hash disagrees with replayed state.
    SceneHashMismatch {
        /// Zero-based entry index.
        entry_index: u32,
    },
}

impl fmt::Display for ReplayError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Tail(error) => error.fmt(formatter),
            Self::Integrity(error) => error.fmt(formatter),
            Self::World { entry_index, .. } => {
                write!(formatter, "world rejected replay entry {entry_index}")
            }
            Self::Invariant { entry_index, .. } => {
                write!(
                    formatter,
                    "world invariant failed at replay entry {entry_index}"
                )
            }
            Self::RevisionMismatch { entry_index } => {
                write!(formatter, "revision mismatch at replay entry {entry_index}")
            }
            Self::SceneHashMismatch { entry_index } => {
                write!(
                    formatter,
                    "scene hash mismatch at replay entry {entry_index}"
                )
            }
        }
    }
}

impl std::error::Error for ReplayError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Tail(error) => Some(error),
            Self::Integrity(error) => Some(error),
            Self::World { source, .. } => Some(source),
            Self::Invariant { source, .. } => Some(source),
            _ => None,
        }
    }
}

impl From<ReplayTailError> for ReplayError {
    fn from(value: ReplayTailError) -> Self {
        Self::Tail(value)
    }
}
