use core::fmt;

use cogniform_protocol::{JsonErrorCategory, ValidationError};

/// Stable classifications for invalid local-session values.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LocalSessionValidationKind {
    /// The local-session schema version is not supported.
    UnsupportedVersion,
    /// Advertised receive limits are internally inconsistent.
    InvalidLimits,
    /// A nested core protocol value is invalid.
    InvalidProtocolValue,
    /// Patch admission metadata is internally inconsistent.
    InvalidPatchAdmission,
    /// Patch completion metadata is internally inconsistent.
    InvalidPatchCompletion,
}

/// A bounded local-session validation failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LocalSessionValidationError {
    kind: LocalSessionValidationKind,
    field: &'static str,
    protocol_error: Option<ValidationError>,
}

impl LocalSessionValidationError {
    pub(crate) const fn new(kind: LocalSessionValidationKind, field: &'static str) -> Self {
        Self {
            kind,
            field,
            protocol_error: None,
        }
    }

    pub(crate) const fn protocol(field: &'static str, error: ValidationError) -> Self {
        Self {
            kind: LocalSessionValidationKind::InvalidProtocolValue,
            field,
            protocol_error: Some(error),
        }
    }

    /// Returns the stable validation classification.
    #[must_use]
    pub const fn kind(&self) -> LocalSessionValidationKind {
        self.kind
    }

    /// Returns the stable session-schema field path.
    #[must_use]
    pub const fn field(&self) -> &'static str {
        self.field
    }

    /// Returns the nested protocol error when a core value was invalid.
    #[must_use]
    pub const fn protocol_error(&self) -> Option<ValidationError> {
        self.protocol_error
    }
}

impl fmt::Display for LocalSessionValidationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(error) = self.protocol_error {
            write!(
                formatter,
                "invalid protocol value at {}: {error}",
                self.field
            )
        } else {
            write!(formatter, "{:?} at {}", self.kind, self.field)
        }
    }
}

impl std::error::Error for LocalSessionValidationError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.protocol_error
            .as_ref()
            .map(|error| error as &(dyn std::error::Error + 'static))
    }
}

/// Bounded canonical codec and frame-adaptation failures.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LocalSessionError {
    /// The local frame configuration cannot admit a non-empty control message.
    InvalidConfig,
    /// A complete control message exceeded the effective frame or protocol limit.
    MessageLimitExceeded {
        /// Observed bytes, including the canonical trailing LF when known.
        actual: u64,
        /// Effective maximum control-message bytes.
        limit: u64,
    },
    /// JSON nesting exceeded the configured pre-decode limit.
    NestingLimitExceeded {
        /// Observed nesting depth.
        actual: u16,
        /// Configured maximum nesting depth.
        limit: u16,
    },
    /// JSON parsing failed without retaining input or an unbounded parser string.
    MalformedJson {
        /// Coarse parser classification.
        category: JsonErrorCategory,
        /// One-based parser line.
        line: usize,
        /// One-based parser column.
        column: usize,
    },
    /// The typed message violates a local-session or nested protocol invariant.
    InvalidMessage(LocalSessionValidationError),
    /// The input is valid JSON but not the exact canonical representation.
    NonCanonicalMessage,
    /// A client decoder received a server shape or the reverse.
    WrongDirection,
    /// A session decoder received an observation frame instead of control bytes.
    WrongFrameKind,
    /// Canonical serialization failed for a validated value.
    SerializationFailed,
    /// A bounded output allocation could not be satisfied.
    AllocationFailed,
}

impl fmt::Display for LocalSessionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidConfig => formatter
                .write_str("local frame configuration cannot admit a non-empty control message"),
            Self::MessageLimitExceeded { actual, limit } => {
                write!(
                    formatter,
                    "control message has {actual} bytes; limit is {limit}"
                )
            }
            Self::NestingLimitExceeded { actual, limit } => {
                write!(
                    formatter,
                    "JSON nesting depth is {actual}; limit is {limit}"
                )
            }
            Self::MalformedJson {
                category,
                line,
                column,
            } => write!(formatter, "{category:?} JSON error at {line}:{column}"),
            Self::InvalidMessage(error) => error.fmt(formatter),
            Self::NonCanonicalMessage => {
                formatter.write_str("control message is not canonical JSON followed by LF")
            }
            Self::WrongDirection => formatter.write_str("control message has the wrong direction"),
            Self::WrongFrameKind => formatter.write_str("expected a local control frame"),
            Self::SerializationFailed => {
                formatter.write_str("canonical control-message serialization failed")
            }
            Self::AllocationFailed => {
                formatter.write_str("bounded control-message allocation failed")
            }
        }
    }
}

impl std::error::Error for LocalSessionError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::InvalidMessage(error) => Some(error),
            _ => None,
        }
    }
}
