use core::fmt;
use std::io;

use cogniform_observation::ObservationEnvelopeError;

/// Stable local-frame region used in truncated-input diagnostics.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LocalFrameSection {
    /// Fixed version-one header.
    Header,
    /// Schema-owned control or canonical metadata bytes.
    Control,
    /// Observation payload-envelope bytes.
    Bulk,
}

/// Stable I/O operation category without reader, writer, or payload details.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LocalFrameIoOperation {
    /// Reading the fixed header.
    ReadHeader,
    /// Reading the control section.
    ReadControl,
    /// Reading the bulk section.
    ReadBulk,
    /// Writing a completely encoded frame.
    WriteFrame,
}

/// Fail-closed local stream framing, validation, or I/O error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LocalFrameError {
    /// A control frame contained no schema-owned bytes.
    EmptyControl,
    /// The complete frame exceeds its configured byte bound.
    FrameLimitExceeded {
        /// Required or observed bytes.
        actual: u64,
        /// Configured maximum bytes.
        limit: u64,
    },
    /// The control section exceeds its configured byte bound.
    ControlLimitExceeded {
        /// Required or observed bytes.
        actual: u64,
        /// Configured maximum bytes.
        limit: u64,
    },
    /// The bulk section exceeds its configured byte bound.
    BulkLimitExceeded {
        /// Required or observed bytes.
        actual: u64,
        /// Configured maximum bytes.
        limit: u64,
    },
    /// Checked length arithmetic overflowed.
    SizeOverflow,
    /// A bounded body or output reservation failed.
    AllocationFailed,
    /// Input ended after a frame had started.
    Truncated {
        /// Region that ended early.
        section: LocalFrameSection,
        /// Exact required bytes for this region.
        expected: u64,
        /// Bytes read from this region.
        actual: u64,
    },
    /// A borrowed complete frame contains bytes after its declared body.
    TrailingBytes {
        /// Exact declared complete bytes.
        expected: u64,
        /// Borrowed input bytes.
        actual: u64,
    },
    /// The fixed local-frame magic does not match.
    InvalidMagic,
    /// The local-frame version is not supported.
    UnsupportedVersion {
        /// Header version.
        found: u16,
    },
    /// The frame kind tag is unknown.
    UnsupportedKind {
        /// Header kind tag.
        found: u8,
    },
    /// A reserved header field is non-zero.
    NonCanonicalHeader,
    /// Correlation identity zero is reserved.
    InvalidCorrelationId,
    /// The frame kind and section lengths are inconsistent.
    InvalidSectionLayout,
    /// The frame digest does not match the exact header and body.
    IntegrityMismatch,
    /// Observation metadata was invalid or not in its exact canonical form.
    InvalidObservationMetadata,
    /// The nested CF038 payload envelope rejected the bulk section.
    ObservationEnvelope(ObservationEnvelopeError),
    /// A caller-owned reader or writer failed; no implementation detail is retained.
    Io {
        /// Stable operation category.
        operation: LocalFrameIoOperation,
        /// Stable standard-library error kind.
        kind: io::ErrorKind,
    },
}

impl fmt::Display for LocalFrameError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyControl => formatter.write_str("local control frame is empty"),
            Self::FrameLimitExceeded { actual, limit } => {
                write!(formatter, "local frame size {actual} exceeds limit {limit}")
            }
            Self::ControlLimitExceeded { actual, limit } => write!(
                formatter,
                "local frame control size {actual} exceeds limit {limit}"
            ),
            Self::BulkLimitExceeded { actual, limit } => {
                write!(
                    formatter,
                    "local frame bulk size {actual} exceeds limit {limit}"
                )
            }
            Self::SizeOverflow => formatter.write_str("local frame size overflow"),
            Self::AllocationFailed => formatter.write_str("local frame allocation failed"),
            Self::Truncated {
                section,
                expected,
                actual,
            } => write!(
                formatter,
                "local frame {section:?} is truncated: expected {expected} bytes, found {actual}"
            ),
            Self::TrailingBytes { expected, actual } => write!(
                formatter,
                "local frame has trailing bytes: expected {expected}, found {actual}"
            ),
            Self::InvalidMagic => formatter.write_str("invalid local frame magic"),
            Self::UnsupportedVersion { found } => {
                write!(formatter, "unsupported local frame version {found}")
            }
            Self::UnsupportedKind { found } => {
                write!(formatter, "unsupported local frame kind {found}")
            }
            Self::NonCanonicalHeader => formatter.write_str("non-canonical local frame header"),
            Self::InvalidCorrelationId => formatter.write_str("local frame correlation ID is zero"),
            Self::InvalidSectionLayout => {
                formatter.write_str("local frame kind and section lengths are inconsistent")
            }
            Self::IntegrityMismatch => formatter.write_str("local frame integrity mismatch"),
            Self::InvalidObservationMetadata => {
                formatter.write_str("local frame observation metadata is invalid or non-canonical")
            }
            Self::ObservationEnvelope(error) => write!(formatter, "{error}"),
            Self::Io { operation, kind } => {
                write!(formatter, "local frame {operation:?} failed with {kind:?}")
            }
        }
    }
}

impl std::error::Error for LocalFrameError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::ObservationEnvelope(error) => Some(error),
            _ => None,
        }
    }
}

impl From<ObservationEnvelopeError> for LocalFrameError {
    fn from(error: ObservationEnvelopeError) -> Self {
        Self::ObservationEnvelope(error)
    }
}
