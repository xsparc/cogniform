use core::fmt;

use cogniform_protocol::ObservationKind;

/// Fail-closed error from observation payload envelope validation or coding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ObservationEnvelopeError {
    /// The causal metadata failed its public protocol contract.
    InvalidMetadata,
    /// The payload variant does not match the metadata kind.
    KindMismatch {
        /// Kind declared by metadata.
        metadata: ObservationKind,
        /// Kind represented by the payload or envelope.
        payload: ObservationKind,
    },
    /// The payload item count disagrees with metadata dimensions or the header.
    ItemCountMismatch {
        /// Required count.
        expected: u64,
        /// Observed count.
        actual: u64,
    },
    /// A visibility payload exceeds its explicit entry bound.
    VisibilityEntryLimitExceeded {
        /// Observed entries.
        actual: u64,
        /// Configured maximum entries.
        limit: u32,
    },
    /// Visibility counts exceed the configured observation pixel bound.
    VisibilityPixelLimitExceeded {
        /// Observed aggregate visible pixels.
        actual: u64,
        /// Configured maximum pixels.
        limit: u64,
    },
    /// A payload value violates its finite, range, unit, presence, or order rule.
    InvalidPayloadValue {
        /// Payload kind being decoded.
        kind: ObservationKind,
        /// Zero-based item index.
        index: u64,
    },
    /// The complete envelope exceeds the caller's byte bound.
    EnvelopeLimitExceeded {
        /// Observed or required bytes.
        actual: u64,
        /// Configured maximum bytes.
        limit: u64,
    },
    /// Checked header or payload size arithmetic overflowed.
    SizeOverflow,
    /// Memory reservation failed after all declared bounds passed.
    AllocationFailed,
    /// The input cannot contain one complete version-one header.
    Truncated {
        /// Minimum or header-declared complete size.
        expected: u64,
        /// Available bytes.
        actual: u64,
    },
    /// Bytes remain after the exact header-declared envelope.
    TrailingBytes {
        /// Exact required size.
        expected: u64,
        /// Available bytes.
        actual: u64,
    },
    /// The fixed envelope magic does not match this format.
    InvalidMagic,
    /// The envelope version is not supported by this implementation.
    UnsupportedVersion {
        /// Version read from the header.
        found: u16,
    },
    /// The payload kind tag is unknown.
    UnsupportedKind {
        /// Tag read from the header.
        found: u8,
    },
    /// Reserved header bits or absent-value padding are non-zero.
    NonCanonicalEncoding,
    /// The header payload byte count disagrees with the fixed kind layout.
    PayloadLengthMismatch {
        /// Required bytes for the declared kind and count.
        expected: u64,
        /// Header-declared bytes.
        actual: u64,
    },
    /// The SHA-256 binding over header, metadata, and payload does not match.
    IntegrityMismatch,
}

impl fmt::Display for ObservationEnvelopeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidMetadata => formatter.write_str("observation metadata is invalid"),
            Self::KindMismatch { metadata, payload } => write!(
                formatter,
                "observation metadata kind {metadata:?} does not match payload kind {payload:?}"
            ),
            Self::ItemCountMismatch { expected, actual } => write!(
                formatter,
                "observation payload item count {actual} does not match required count {expected}"
            ),
            Self::VisibilityEntryLimitExceeded { actual, limit } => write!(
                formatter,
                "visibility payload entry count {actual} exceeds limit {limit}"
            ),
            Self::VisibilityPixelLimitExceeded { actual, limit } => write!(
                formatter,
                "visibility payload pixel count {actual} exceeds limit {limit}"
            ),
            Self::InvalidPayloadValue { kind, index } => {
                write!(
                    formatter,
                    "observation payload {kind:?} item {index} is invalid"
                )
            }
            Self::EnvelopeLimitExceeded { actual, limit } => write!(
                formatter,
                "observation payload envelope size {actual} exceeds limit {limit}"
            ),
            Self::SizeOverflow => formatter.write_str("observation payload size overflow"),
            Self::AllocationFailed => formatter.write_str("observation payload allocation failed"),
            Self::Truncated { expected, actual } => write!(
                formatter,
                "observation payload envelope is truncated: expected {expected} bytes, found {actual}"
            ),
            Self::TrailingBytes { expected, actual } => write!(
                formatter,
                "observation payload envelope has trailing bytes: expected {expected}, found {actual}"
            ),
            Self::InvalidMagic => formatter.write_str("invalid observation payload magic"),
            Self::UnsupportedVersion { found } => {
                write!(formatter, "unsupported observation payload version {found}")
            }
            Self::UnsupportedKind { found } => {
                write!(formatter, "unsupported observation payload kind {found}")
            }
            Self::NonCanonicalEncoding => {
                formatter.write_str("non-canonical observation payload encoding")
            }
            Self::PayloadLengthMismatch { expected, actual } => write!(
                formatter,
                "observation payload length {actual} does not match required length {expected}"
            ),
            Self::IntegrityMismatch => {
                formatter.write_str("observation payload integrity mismatch")
            }
        }
    }
}

impl std::error::Error for ObservationEnvelopeError {}
