use core::fmt;

use serde::{Deserialize, Serialize};

/// Classifies invalid scalar values before they enter a protocol message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValueErrorKind {
    /// An opaque identifier was zero.
    ZeroIdentifier,
    /// An opaque identifier was not 32 lowercase hexadecimal characters.
    InvalidIdentifierEncoding,
    /// A text value was empty.
    EmptyText,
    /// A text value contained a NUL character.
    TextContainsNul,
    /// A floating-point value was NaN or infinite.
    NonFiniteNumber,
    /// A floating-point value was outside the inclusive unit interval.
    OutsideUnitInterval,
    /// A floating-point value was negative.
    NegativeNumber,
    /// A floating-point value was zero or negative.
    NonPositiveNumber,
    /// A numeric identifier was zero.
    ZeroNumericIdentifier,
    /// A scene revision could not be incremented.
    RevisionOverflow,
    /// A schema version was zero.
    ZeroSchemaVersion,
}

/// Reports why a scalar protocol value could not be constructed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ValueError {
    kind: ValueErrorKind,
}

impl ValueError {
    pub(crate) const fn new(kind: ValueErrorKind) -> Self {
        Self { kind }
    }

    /// Returns the stable error classification.
    #[must_use]
    pub const fn kind(&self) -> ValueErrorKind {
        self.kind
    }
}

impl fmt::Display for ValueError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self.kind {
            ValueErrorKind::ZeroIdentifier => "opaque identifiers must be non-zero",
            ValueErrorKind::InvalidIdentifierEncoding => {
                "opaque identifiers must use 32 lowercase hexadecimal characters"
            }
            ValueErrorKind::EmptyText => "text values must not be empty",
            ValueErrorKind::TextContainsNul => "text values must not contain NUL characters",
            ValueErrorKind::NonFiniteNumber => "numbers must be finite",
            ValueErrorKind::OutsideUnitInterval => "numbers must be between zero and one",
            ValueErrorKind::NegativeNumber => "numbers must not be negative",
            ValueErrorKind::NonPositiveNumber => "numbers must be greater than zero",
            ValueErrorKind::ZeroNumericIdentifier => "numeric identifiers must be non-zero",
            ValueErrorKind::RevisionOverflow => "scene revision overflow",
            ValueErrorKind::ZeroSchemaVersion => "schema versions must be non-zero",
        };
        formatter.write_str(message)
    }
}

impl std::error::Error for ValueError {}

/// Stable machine-readable diagnostic codes used by validation and receipts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum DiagnosticCode {
    /// The message schema is not supported by this build.
    UnsupportedSchema,
    /// The patch contains no operations.
    EmptyPatch,
    /// A runtime or declared operation limit was exceeded.
    OperationLimitExceeded,
    /// A runtime or declared component limit was exceeded.
    ComponentLimitExceeded,
    /// A runtime or declared text limit was exceeded.
    TextLimitExceeded,
    /// A runtime or declared logical decoded-byte limit was exceeded.
    DecodedSizeLimitExceeded,
    /// A diagnostics collection exceeded its configured limit.
    DiagnosticLimitExceeded,
    /// A queue capacity exceeded the runtime limit.
    QueueCapacityExceeded,
    /// An imagination contains no entities.
    EmptyImagination,
    /// An imagination entity collection exceeded its declared or runtime limit.
    ImaginationEntityLimitExceeded,
    /// An imagination relation collection exceeded its declared or runtime limit.
    ImaginationRelationLimitExceeded,
    /// An imagination constraint collection exceeded its declared or runtime limit.
    ImaginationConstraintLimitExceeded,
    /// An imagination declares the same local entity key more than once.
    DuplicateImaginationEntity,
    /// A scene query contains duplicate entity or component filters.
    DuplicateQueryFilter,
    /// A scene query or result exceeded its entity limit.
    QueryEntityLimitExceeded,
    /// A scene query result is not in canonical stable order.
    NonCanonicalQueryResult,
    /// A component value violates its domain invariant.
    InvalidComponentValue,
    /// A create operation contains the same component kind more than once.
    DuplicateComponent,
    /// An entity was assigned itself as parent.
    SelfParent,
    /// A receipt does not describe one accepted revision transition.
    InvalidReceiptRevision,
    /// A receipt operation count is inconsistent with an accepted patch.
    InvalidReceiptOperationCount,
    /// Observation dimensions are missing or unexpected for the observation kind.
    InvalidObservationDimensions,
    /// Observation dimensions overflow or exceed the pixel budget.
    ObservationPixelLimitExceeded,
    /// Observation staleness metadata is inconsistent with its revisions.
    InvalidObservationStaleness,
}

/// A typed message-validation failure with a stable field path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ValidationError {
    code: DiagnosticCode,
    field: &'static str,
}

impl ValidationError {
    pub(crate) const fn new(code: DiagnosticCode, field: &'static str) -> Self {
        Self { code, field }
    }

    /// Returns the stable diagnostic code.
    #[must_use]
    pub const fn code(&self) -> DiagnosticCode {
        self.code
    }

    /// Returns the schema field associated with the failure.
    #[must_use]
    pub const fn field(&self) -> &'static str {
        self.field
    }
}

impl fmt::Display for ValidationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{:?} at {}", self.code, self.field)
    }
}

impl std::error::Error for ValidationError {}

/// Coarse JSON parser categories that do not expose unbounded parser strings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JsonErrorCategory {
    /// The JSON syntax is malformed.
    Syntax,
    /// The JSON value cannot be represented by the requested schema.
    Data,
    /// The JSON document ended unexpectedly.
    EndOfFile,
    /// An I/O error occurred while parsing.
    Io,
}

/// Errors returned by bounded canonical JSON encoding and decoding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CodecError {
    /// The encoded message exceeds the configured byte limit.
    EncodedSizeExceeded {
        /// Observed encoded byte count.
        actual: u64,
        /// Maximum permitted encoded byte count.
        limit: u64,
    },
    /// The JSON nesting depth exceeds the configured limit.
    NestingLimitExceeded {
        /// Observed JSON nesting depth.
        actual: u16,
        /// Maximum permitted JSON nesting depth.
        limit: u16,
    },
    /// JSON parsing failed without retaining input or an unbounded error string.
    MalformedJson {
        /// Coarse parser error category.
        category: JsonErrorCategory,
        /// One-based line number reported by the parser.
        line: usize,
        /// One-based column number reported by the parser.
        column: usize,
    },
    /// The parsed message violates a protocol invariant or configured limit.
    InvalidMessage(ValidationError),
    /// Serialization failed for a validated protocol value.
    SerializationFailed,
}

impl fmt::Display for CodecError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EncodedSizeExceeded { actual, limit } => {
                write!(
                    formatter,
                    "encoded message has {actual} bytes; limit is {limit}"
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
            Self::SerializationFailed => formatter.write_str("canonical JSON serialization failed"),
        }
    }
}

impl std::error::Error for CodecError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::InvalidMessage(error) => Some(error),
            _ => None,
        }
    }
}
